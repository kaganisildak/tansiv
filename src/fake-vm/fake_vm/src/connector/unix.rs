use crate::buffer_pool::BufferPool;
use std::io::Result;
use std::os::unix::net::UnixStream;
use super::{Connector, Endianness, MsgIn, MsgOut};

#[derive(Debug)]
pub(crate) struct UnixConnector {
    actor: UnixStream,
    output_buffer: Vec<u8>,
}

impl UnixConnector {
    fn inner_new(config: &crate::Config) -> Result<UnixConnector> {
        let actor_stream = UnixStream::connect(&config.actor_socket)?;

        let output_buffer_size = MsgOut::max_header_size();
        let mut output_buffer = Vec::with_capacity(output_buffer_size);
        output_buffer.resize(output_buffer_size, 0);

        Ok(UnixConnector {
            actor: actor_stream,
            output_buffer: output_buffer,
        })
    }
}

impl Connector for UnixConnector {
    fn new(config: &crate::Config) -> Result<(UnixConnector, BufferPool)> {
        let connector = UnixConnector::inner_new(config)?;

        let input_buffer_size = usize::max(MsgIn::max_header_size(), crate::MAX_PACKET_SIZE);
        let input_buffer_pool = BufferPool::new(input_buffer_size, config.num_buffers.get());

        Ok((connector, input_buffer_pool))
    }

    fn recv<'a, 'b>(&'a mut self, input_buffer: &'b mut [u8]) -> Result<MsgIn<'b>> {
        MsgIn::recv(&mut self.actor, input_buffer, Endianness::Native)
    }

    fn send<'a, 'b>(&'a mut self, msg: MsgOut<'b>) -> Result<()> {
        let stream = &mut self.actor;
        let buffer = self.output_buffer.as_mut_slice();
        msg.send(stream, buffer, Endianness::Native)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use binser::Endianness;
    use crate::from_io_result;
    use crate::connector::{MsgIn, MsgOut};
    use log::{error, info};
    use std::fmt;
    use std::io::Read;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;

    #[derive(Debug)]
    pub struct Error {
        error: crate::error::Error,
        context: &'static str,
    }

    impl Error {
        pub fn new(error: crate::error::Error, context: &'static str) -> Error {
            Error {
                error: error,
                context: context,
            }
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}: ", self.context)?;
            self.error.fmt(f)
        }
    }

    impl std::error::Error for Error {}

    pub type TestResult<T> = std::result::Result<T, Error>;

    pub fn test_prepare_connect<F>(path: &PathBuf, server_fn: F)
        where F: FnOnce(UnixListener) -> (),
              F: Send + 'static {
        std::fs::remove_file(path).ok();
        let server = UnixListener::bind(path).expect(&format!("Could not create server socket '{:?}'", path));
        std::thread::spawn(move || server_fn(server));
    }

    pub fn test_cleanup_connect(path: &PathBuf) {
        std::fs::remove_file(path).expect(&format!("Server socket '{:?}' could not be removed", path))
    }

    pub fn test_actor<F>(server: UnixListener, actor_fn: F) -> ()
        where F: FnOnce(&mut UnixStream) -> TestResult<()> {
        info!("Server listening at address {:?}", server);
        match server.accept() {
            Ok((mut client, address)) => {
                info!("New client: {:?}", address);
                match actor_fn(&mut client) {
                    Err(e) => error!("Actor failed: {:?}", e),
                    _ => {
                        // Just drain until the VM ends, do not make it fail when sending messages
                        for _ in client.bytes() {
                        }
                    },
                }
            },
            Err(e) => error!("Failed to accept connection: {:?}", e),
        }
    }

    pub fn test_actor_check<T>(result: crate::Result<T>, context: &'static str) -> TestResult<T> {
        result.map_err(|e| Error::new(e, context))
    }

    pub fn test_dummy_actor(server: UnixListener) {
        test_actor(server, |_| Ok(()))
    }

    pub fn test_actor_send<'a>(client: &mut UnixStream, msg: MsgIn<'a>) -> TestResult<()> {
        let mut buffer = [0u8; crate::MAX_PACKET_SIZE];
        test_actor_check(from_io_result(msg.send(client, &mut buffer, Endianness::Native)), "Send failed")
    }

    pub fn test_actor_recv<'a>(client: &mut UnixStream, buffer: &'a mut [u8]) -> TestResult<MsgOut<'a>> {
        test_actor_check(from_io_result(MsgOut::recv(client, buffer, Endianness::Native)), "Recv failed")
    }
}

#[cfg(test)]
mod test {
    use binser::{ToBytes, ToStream, SizedAsBytes};
    use crate::{Config, connector::*};
    use log::info;
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use structopt::StructOpt;
    use super::{test_helpers::*, *};

    #[test]
    fn valid_server_path() {
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]).unwrap();

        let connector = UnixConnector::inner_new(&config);
        assert!(connector.is_ok());
        assert_eq!(connector.unwrap().output_buffer.len(), MsgOut::max_header_size());

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn invalid_server_path() {
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let config = Config::from_iter_safe(&["-amust not exist", "-t1970-01-02T00:00:00"]).unwrap();

        assert!(UnixConnector::inner_new(&config).is_err());

        test_cleanup_connect(&server_path);
    }

    #[test]
    fn valid_input_buffer_size() {
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, test_dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]).unwrap();

        let (_, input_buffer_pool) = UnixConnector::new(&config).unwrap();
        let input_buffer = input_buffer_pool.allocate_buffer(crate::MAX_PACKET_SIZE).unwrap();
        assert_eq!(input_buffer.len(), usize::max(MsgIn::max_header_size(), crate::MAX_PACKET_SIZE));

        test_cleanup_connect(&server_path);
    }

    fn run_client_server<C, S>(client_fn: C, server_fn: S)
        where C: FnOnce(UnixConnector, &mut [u8]) -> (),
              S: FnOnce(UnixListener) -> (),
              S: Send + 'static {
        let server_path = PathBuf::from("titi");
        test_prepare_connect(&server_path, server_fn);
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]).unwrap();
        let (connector, input_buffer_pool) = UnixConnector::new(&config).unwrap();
        let mut input_buffer = BufferPool::allocate_buffer(&input_buffer_pool, crate::MAX_PACKET_SIZE).unwrap();

        client_fn(connector, &mut input_buffer);

        test_cleanup_connect(&server_path);
    }

    fn send_partial_msg_type(socket: &mut UnixStream) {
        let mut buffer = [0; MsgInType::NUM_BYTES];
        MsgInType::GoToDeadline.to_bytes(&mut buffer, Endianness::Native).expect("Failed to serialize message type");
        socket.write_all(&buffer[..(MsgInType::NUM_BYTES - 1)]).expect("Failed to send partial message type");
    }

    fn recv_partial_msg_type_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_partial_msg_type(&mut client);
    }

    #[test]
    fn recv_partial_msg_type() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_msg_type_srv)
    }

    fn send_invalid_msg_type(socket: &mut UnixStream) {
        let invalid_type = (MsgInType::GoToDeadline as u32 + 1) * (MsgInType::DeliverPacket as u32 + 1) + 1;
        let mut buffer = [0; u32::NUM_BYTES];
        invalid_type.to_stream(socket, &mut buffer, Endianness::Native).expect("Failed to send message type");
    }

    fn recv_invalid_msg_type_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_invalid_msg_type(&mut client);
    }

    #[test]
    fn recv_invalid_msg_type() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        recv_invalid_msg_type_srv)
    }

    static GO_TO_DEADLINE: GoToDeadline = GoToDeadline {
        deadline: Time {
            seconds: 2,
            useconds: 100,
        },
    };

    fn send_partial_go_to_deadline(socket: &mut UnixStream, msg: GoToDeadline) {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let mut buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::GoToDeadline;

        msg_type.to_stream(socket, buffer, Endianness::Native).expect("Failed to send message type");
        msg.to_bytes(&mut buffer, Endianness::Native).expect("Failed to serialize go_to_deadline");
        socket.write_all(&buffer[..(GoToDeadline::NUM_BYTES - 1)]).expect("Failed to send partial go_to_deadline");
    }

    fn recv_partial_go_to_deadline_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_partial_go_to_deadline(&mut client, GO_TO_DEADLINE);
    }

    #[test]
    fn recv_partial_go_to_deadline() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_go_to_deadline_srv)
    }

    // If types used to represent seconds change, the test will just not compile.
    // If types used to represent seconds have to differ between network representation and
    // internal representation, then uncomment and adapt the next test case.
    #[test]
    fn go_to_deadline_seconds_not_overflowable() {
        use std::time::Duration;
        use super::super::GoToDeadline;
        use super::super::MsgIn;

        #[allow(unused_variables)]
        let net_should_use_u64 = GoToDeadline { deadline: Time { seconds: 0u64, useconds: 0u64, } };
        #[allow(unused_variables)]
        let internal_should_use_u64 = MsgIn::GoToDeadline(Duration::new(0u64, 0u32));
    }

    // fn recv_go_to_deadline_oob_seconds_srv(server: UnixListener) {
        // let (mut client, address) = server.accept().expect("Failed to accept connection");
        // info!("New client: {:?}", address);

        // let msg = GoToDeadline {
            // deadline: Time {
                // seconds: std::u64::MAX,
                // useconds: 0,
            // },
        // };
        // send_go_to_deadline(&mut client, msg);
    // }

    // #[test]
    // fn recv_go_to_deadline_oob_seconds() {
        // run_client_server(|mut connector, input_buffer| {
            // let error = connector.recv(input_buffer).unwrap_err();
            // assert_eq!(error.kind(), ErrorKind::InvalidData);
        // },
        // recv_go_to_deadline_oob_seconds_srv)
    // }

    fn recv_go_to_deadline_oob_useconds_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);

        let msg = GoToDeadline {
            deadline: Time {
                seconds: 0,
                useconds: std::u64::MAX,
            },
        };
        send_go_to_deadline(&mut client, msg);
    }

    #[test]
    fn recv_go_to_deadline_oob_useconds() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        recv_go_to_deadline_oob_useconds_srv)
    }

    fn send_go_to_deadline(socket: &mut UnixStream, msg: GoToDeadline) {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::GoToDeadline;

        msg_type.to_stream(socket, buffer, Endianness::Native).expect("Failed to send message type");
        msg.to_stream(socket, buffer, Endianness::Native).expect("Failed to send deadline");
    }

    fn recv_go_to_deadline_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_go_to_deadline(&mut client, GO_TO_DEADLINE);
    }

    #[test]
    fn recv_go_to_deadline() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            assert!(msg.is_ok());
            let msg = msg.unwrap();
            match msg {
                MsgIn::GoToDeadline(deadline) => {
                    let seconds = deadline.as_secs();
                    assert_eq!(seconds, GO_TO_DEADLINE.deadline.seconds);
                    let useconds = deadline.subsec_micros();
                    assert_eq!(useconds as u64, GO_TO_DEADLINE.deadline.useconds);
                },
                _ => assert!(false),
            }
        },
        recv_go_to_deadline_srv)
    }

    static DELIVER_PACKET: DeliverPacket = DeliverPacket {
        packet: Packet {
            size: 42,
        },
    };
    static PACKET_PAYLOAD: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";

    fn send_partial_deliver_packet_header(socket: &mut UnixStream, msg: DeliverPacket) {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let mut buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::DeliverPacket;

        msg_type.to_stream(socket, buffer, Endianness::Native).expect("Failed to send message type");
        msg.to_bytes(&mut buffer, Endianness::Native).expect("Failed to serialize deliver_packet header");
        socket.write_all(&buffer[..(DeliverPacket::NUM_BYTES - 1)]).expect("Failed to send partial deliver_packet header");
    }

    fn recv_partial_deliver_packet_header_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_partial_deliver_packet_header(&mut client, DELIVER_PACKET);
    }

    #[test]
    fn recv_partial_deliver_packet_header() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_deliver_packet_header_srv)
    }

    fn recv_partial_deliver_packet_payload_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_deliver_packet(&mut client, DELIVER_PACKET, &PACKET_PAYLOAD[1..]);
    }

    #[test]
    fn recv_partial_deliver_packet_payload() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_deliver_packet_payload_srv)
    }

    fn recv_deliver_packet_payload_too_big_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);

        let msg = DeliverPacket {
            packet: Packet {
                size: (crate::MAX_PACKET_SIZE + 1) as u32,
            },
        };
        let mut big_payload = vec!(0; msg.packet.size as usize);
        big_payload[msg.packet.size as usize - 1] = 1;
        send_deliver_packet(&mut client, msg, &big_payload);
    }

    #[test]
    fn recv_deliver_packet_payload_too_big() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        recv_deliver_packet_payload_too_big_srv)
    }

    fn send_deliver_packet(socket: &mut UnixStream, msg: DeliverPacket, payload: &[u8]) {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::DeliverPacket;

        msg_type.to_stream(socket, buffer, Endianness::Native).expect("Failed to send message type");
        msg.to_stream(socket, buffer, Endianness::Native).expect("Failed to send deliver_packet header");
        socket.write_all(payload).expect("Failed to send payload");
    }

    fn recv_deliver_packet_srv(server: UnixListener) {
        assert_eq!(PACKET_PAYLOAD.len(), DELIVER_PACKET.packet.size as usize);
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);
        send_deliver_packet(&mut client, DELIVER_PACKET, &PACKET_PAYLOAD);
    }

    #[test]
    fn recv_deliver_packet() {
        run_client_server(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            assert!(msg.is_ok());
            let msg = msg.unwrap();
            match msg {
                MsgIn::DeliverPacket(payload) => {
                    assert_eq!(payload, PACKET_PAYLOAD)
                },
                _ => assert!(false),
            }
        },
        recv_deliver_packet_srv)
    }

    fn recv_at_deadline(client: &mut UnixStream) {
        let mut buffer = vec!(0; MsgOut::max_header_size());
        let buffer = buffer.as_mut_slice();
        let msg_type = MsgOutType::from_stream(client, buffer, Endianness::Native).expect("Failed to receive message type");
        assert_eq!(MsgOutType::AtDeadline, msg_type);
    }

    fn send_at_deadline_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);

        recv_at_deadline(&mut client);
    }

    #[test]
    fn send_at_deadline() {
        run_client_server(|mut connector, _| {
            connector.send(MsgOut::AtDeadline).expect("Failed to send at_deadline")
        },
        send_at_deadline_srv)
    }

    fn recv_send_packet(client: &mut UnixStream, buffer: &mut [u8]) -> SendPacket {
        let msg_type = MsgOutType::from_stream(client, buffer, Endianness::Native).expect("Failed to receive message type");
        assert_eq!(MsgOutType::SendPacket, msg_type);

        let msg = SendPacket::from_stream(client, buffer, Endianness::Native).expect("Failed to receive send_packet header");
        if let Some(buffer) = buffer.get_mut(..(msg.packet.size as usize)) {
            client.read_exact(buffer).expect("Failed to receive payload");
        } else {
            panic!("buffer too small to receive payload");
        }

        msg
    }

    fn make_ref_send_packet() -> MsgOut<'static> {
        MsgOut::SendPacket(Duration::new(3, 200),
                       b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF")
    }

    fn send_send_packet_srv(server: UnixListener) {
        let (mut client, address) = server.accept().expect("Failed to accept connection");
        info!("New client: {:?}", address);

        let mut buffer = vec!(0; usize::max(MsgOut::max_header_size(), crate::MAX_PACKET_SIZE));
        let msg = recv_send_packet(&mut client, &mut buffer);
        if let MsgOut::SendPacket(ref_send_time, ref_payload) = make_ref_send_packet() {
            let seconds = ref_send_time.as_secs();
            let useconds = ref_send_time.subsec_micros();
            assert_eq!(msg.send_time.seconds, seconds);
            assert_eq!(msg.send_time.useconds, useconds as u64);
            let payload_len = msg.packet.size as usize;
            assert_eq!(&buffer[..payload_len], ref_payload);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn send_send_packet() {
        run_client_server(|mut connector, _| {
            connector.send(make_ref_send_packet()).expect("Failed to send send_packet")
        },
        send_send_packet_srv)
    }
}
