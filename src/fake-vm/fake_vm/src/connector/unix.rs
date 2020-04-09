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
    use crate::connector::{MsgIn, MsgOut};
    use log::{error, info};
    use std::fmt;
    use std::io::Read;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};

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

    pub struct TestActor {
        socket_path: PathBuf,
        pid: nix::unistd::Pid,
    }

    // Application-side API
    impl TestActor {
        pub fn new<P: AsRef<Path> + std::fmt::Debug, F>(path: P, actor_fn: F) -> TestActor
            where F: FnOnce(&mut UnixStream) -> TestResult<()> {
            use nix::unistd::{fork, ForkResult};

            Self::remove_file_if_present(&path).expect(&format!("Server socket path '{:?}' is busy", &path));
            let server = UnixListener::bind(&path).expect(&format!("Could not create server socket '{:?}'", &path));
            let fork_res = fork().expect("Forking server failed");
            match fork_res {
                ForkResult::Child => {
                    TestActor::run(server, actor_fn);
                    // The server socket is deleted when TestActorDescriptor is dropped
                    // std::fs::remove_file(&path).expect(&format!("Server socket '{:?}' could not be removed", &path));
                    std::process::exit(0)
                },
                ForkResult::Parent { child: child_pid, .. } => {
                    TestActor {
                        socket_path: path.as_ref().to_path_buf(),
                        pid: child_pid,
                    }
                },
            }
        }

        fn remove_file_if_present<P: AsRef<Path> + std::fmt::Debug>(path: P) -> std::io::Result<()> {
            std::fs::remove_file(&path).or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(e),
            })
        }
    }

    // Application-side API
    impl Drop for TestActor {
        fn drop(&mut self) {
            use nix::sys::signal::{kill, Signal};
            use nix::sys::wait::{waitpid, WaitPidFlag};

            Self::remove_file_if_present(&self.socket_path).expect(&format!("Server socket '{:?}' could not be removed", &self.socket_path));
            #[allow(unused_must_use)] {
                kill(self.pid, Signal::SIGTERM);
                // Not required for concurrency but avoids interleaving traces
                waitpid(self.pid, Some(WaitPidFlag::WEXITED));
            }
        }
    }

    // Actor-side API
    impl TestActor {
        fn run<F>(server: UnixListener, actor_fn: F) -> ()
            where F: FnOnce(&mut UnixStream) -> TestResult<()> {
            info!("Server listening at address {:?}", server);
            match server.accept() {
                Ok((mut client, address)) => {
                    info!("New client: {:?}", address);
                    match actor_fn(&mut client) {
                        Err(e) => error!("Actor failed: {:?}", e),
                        _ => {
                            // Just drain until the VM ends, do not make it fail when sending messages
                            if client.shutdown(std::net::Shutdown::Write).is_err() {
                                error!("Shutdown failed")
                            } else {
                                for _ in client.bytes() {
                                }
                            }
                        },
                    }
                },
                Err(e) => error!("Failed to accept connection: {:?}", e),
            }
        }

        pub fn check<T>(result: crate::Result<T>, context: &'static str) -> TestResult<T> {
            result.map_err(|e| Error::new(e, context))
        }

        pub fn check_io<T>(result: std::io::Result<T>, context: &'static str) -> TestResult<T> {
            Self::check(crate::from_io_result(result), context)
        }

        pub fn check_eq<T: PartialEq>(left: T, right: T, context: &'static str) -> TestResult<()> {
            Self::check_io(if left == right {
                Ok(())
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Values do not match"))
            }, context)
        }

        pub fn dummy_actor(_client: &mut UnixStream) -> TestResult<()> {
            Ok(())
        }

        pub fn send<'a>(client: &mut UnixStream, msg: MsgIn<'a>) -> TestResult<()> {
            let mut buffer = [0u8; crate::MAX_PACKET_SIZE];
            Self::check_io(msg.send(client, &mut buffer, Endianness::Native), "Send failed")
        }

        pub fn recv<'a>(client: &mut UnixStream, buffer: &'a mut [u8]) -> TestResult<MsgOut<'a>> {
            Self::check_io(MsgOut::recv(client, buffer, Endianness::Native), "Recv failed")
        }
    }
}

#[cfg(test)]
mod test {
    use binser::{ToBytes, ToStream, SizedAsBytes};
    use crate::{Config, connector::*};
    use std::os::unix::net::UnixStream;
    use structopt::StructOpt;
    use super::test_helpers::*;

    #[test]
    fn valid_server_path() {
        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]).unwrap();

        let connector = UnixConnector::inner_new(&config);
        assert!(connector.is_ok());
        assert_eq!(connector.unwrap().output_buffer.len(), MsgOut::max_header_size());

        drop(actor);
    }

    #[test]
    fn invalid_server_path() {
        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-amust not exist", "-t1970-01-02T00:00:00"]).unwrap();

        assert!(UnixConnector::inner_new(&config).is_err());

        drop(actor);
    }

    #[test]
    fn valid_input_buffer_size() {
        let actor = TestActor::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]).unwrap();

        let (_, input_buffer_pool) = UnixConnector::new(&config).unwrap();
        let input_buffer = input_buffer_pool.allocate_buffer(crate::MAX_PACKET_SIZE).unwrap();
        assert_eq!(input_buffer.len(), usize::max(MsgIn::max_header_size(), crate::MAX_PACKET_SIZE));

        drop(actor);
    }

    fn run_client_and_actor<C, A>(client_fn: C, actor_fn: A)
        where C: FnOnce(UnixConnector, &mut [u8]) -> (),
              A: FnOnce(&mut UnixStream) -> TestResult<()>,
              A: Send + 'static {
        let actor = TestActor::new("titi", actor_fn);
        let config = Config::from_iter_safe(&["-atiti", "-t1970-01-02T00:00:00"]).unwrap();
        let (connector, input_buffer_pool) = UnixConnector::new(&config).unwrap();
        let mut input_buffer = BufferPool::allocate_buffer(&input_buffer_pool, crate::MAX_PACKET_SIZE).unwrap();

        client_fn(connector, &mut input_buffer);

        drop(actor);
    }

    fn send_partial_msg_type(socket: &mut UnixStream) -> TestResult<()> {
        let mut buffer = [0; MsgInType::NUM_BYTES];
        TestActor::check_io(MsgInType::GoToDeadline.to_bytes(&mut buffer, Endianness::Native), "Failed to serialize message type")?;
        TestActor::check_io(socket.write_all(&buffer[..(MsgInType::NUM_BYTES - 1)]), "Failed to send partial message type")
    }

    #[test]
    fn recv_partial_msg_type() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        send_partial_msg_type)
    }

    fn send_invalid_msg_type(socket: &mut UnixStream) -> TestResult<()> {
        let invalid_type = (MsgInType::GoToDeadline as u32 + 1) * (MsgInType::DeliverPacket as u32 + 1) + 1;
        let mut buffer = [0; u32::NUM_BYTES];
        TestActor::check_io(invalid_type.to_stream(socket, &mut buffer, Endianness::Native), "Failed to send message type")
    }

    #[test]
    fn recv_invalid_msg_type() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        send_invalid_msg_type)
    }

    static GO_TO_DEADLINE: GoToDeadline = GoToDeadline {
        deadline: Time {
            seconds: 2,
            useconds: 100,
        },
    };

    fn send_partial_go_to_deadline(socket: &mut UnixStream) -> TestResult<()> {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let mut buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::GoToDeadline;

        TestActor::check_io(msg_type.to_stream(socket, buffer, Endianness::Native), "Failed to send message type")?;
        TestActor::check_io(GO_TO_DEADLINE.to_bytes(&mut buffer, Endianness::Native), "Failed to serialize GO_TO_DEADLINE")?;
        TestActor::check_io(socket.write_all(&buffer[..(GoToDeadline::NUM_BYTES - 1)]), "Failed to send partial go_to_deadline")
    }

    #[test]
    fn recv_partial_go_to_deadline() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        send_partial_go_to_deadline)
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

    // fn recv_go_to_deadline_oob_seconds_actor(client: &mut UnixStream) -> TestResult<()> {
        // let msg = GoToDeadline {
            // deadline: Time {
                // seconds: std::u64::MAX,
                // useconds: 0,
            // },
        // };
        // send_go_to_deadline(&mut client, msg)
    // }

    // #[test]
    // fn recv_go_to_deadline_oob_seconds() {
        // run_client_and_actor(|mut connector, input_buffer| {
            // let error = connector.recv(input_buffer).unwrap_err();
            // assert_eq!(error.kind(), ErrorKind::InvalidData);
        // },
        // recv_go_to_deadline_oob_seconds_actor)
    // }

    fn recv_go_to_deadline_oob_useconds_actor(client: &mut UnixStream) -> TestResult<()> {
        let msg = GoToDeadline {
            deadline: Time {
                seconds: 0,
                useconds: std::u64::MAX,
            },
        };
        send_go_to_deadline(client, msg)
    }

    #[test]
    fn recv_go_to_deadline_oob_useconds() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        recv_go_to_deadline_oob_useconds_actor)
    }

    fn send_go_to_deadline(socket: &mut UnixStream, msg: GoToDeadline) -> TestResult<()> {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::GoToDeadline;

        TestActor::check_io(msg_type.to_stream(socket, buffer, Endianness::Native), "Failed to send message type")?;
        TestActor::check_io(msg.to_stream(socket, buffer, Endianness::Native), "Failed to send deadline")
    }

    fn recv_go_to_deadline_actor(client: &mut UnixStream) -> TestResult<()> {
        send_go_to_deadline(client, GO_TO_DEADLINE)
    }

    #[test]
    fn recv_go_to_deadline() {
        run_client_and_actor(|mut connector, input_buffer| {
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
        recv_go_to_deadline_actor)
    }

    static DELIVER_PACKET: DeliverPacket = DeliverPacket {
        packet: Packet {
            size: 42,
        },
    };
    static PACKET_PAYLOAD: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";

    fn send_partial_deliver_packet_header(socket: &mut UnixStream, msg: DeliverPacket) -> TestResult<()> {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let mut buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::DeliverPacket;

        TestActor::check_io(msg_type.to_stream(socket, buffer, Endianness::Native), "Failed to send message type")?;
        TestActor::check_io(msg.to_bytes(&mut buffer, Endianness::Native), "Failed to serialize deliver_packet header")?;
        TestActor::check_io(socket.write_all(&buffer[..(DeliverPacket::NUM_BYTES - 1)]), "Failed to send partial deliver_packet header")
    }

    fn recv_partial_deliver_packet_header_actor(client: &mut UnixStream) -> TestResult<()> {
        send_partial_deliver_packet_header(client, DELIVER_PACKET)
    }

    #[test]
    fn recv_partial_deliver_packet_header() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_deliver_packet_header_actor)
    }

    fn recv_partial_deliver_packet_payload_actor(client: &mut UnixStream) -> TestResult<()> {
        send_deliver_packet(client, DELIVER_PACKET, &PACKET_PAYLOAD[1..])
    }

    #[test]
    fn recv_partial_deliver_packet_payload() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_deliver_packet_payload_actor)
    }

    fn recv_deliver_packet_payload_too_big_actor(client: &mut UnixStream) -> TestResult<()> {
        let msg = DeliverPacket {
            packet: Packet {
                size: (crate::MAX_PACKET_SIZE + 1) as u32,
            },
        };
        let mut big_payload = vec!(0; msg.packet.size as usize);
        big_payload[msg.packet.size as usize - 1] = 1;
        send_deliver_packet(client, msg, &big_payload)
    }

    #[test]
    fn recv_deliver_packet_payload_too_big() {
        run_client_and_actor(|mut connector, input_buffer| {
            let msg = connector.recv(input_buffer);
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        recv_deliver_packet_payload_too_big_actor)
    }

    fn send_deliver_packet(socket: &mut UnixStream, msg: DeliverPacket, payload: &[u8]) -> TestResult<()> {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::DeliverPacket;

        TestActor::check_io(msg_type.to_stream(socket, buffer, Endianness::Native), "Failed to send message type")?;
        TestActor::check_io(msg.to_stream(socket, buffer, Endianness::Native), "Failed to send deliver_packet header")?;
        TestActor::check_io(socket.write_all(payload), "Failed to send payload")
    }

    fn recv_deliver_packet_actor(client: &mut UnixStream) -> TestResult<()> {
        assert_eq!(PACKET_PAYLOAD.len(), DELIVER_PACKET.packet.size as usize);
        send_deliver_packet(client, DELIVER_PACKET, &PACKET_PAYLOAD)
    }

    #[test]
    fn recv_deliver_packet() {
        run_client_and_actor(|mut connector, input_buffer| {
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
        recv_deliver_packet_actor)
    }

    fn recv_msg_out_type(client: &mut UnixStream, expected_type: MsgOutType) -> TestResult<MsgOutType> {
        let mut buffer = vec!(0; MsgOutType::NUM_BYTES);
        let buffer = buffer.as_mut_slice();
        let msg_type = TestActor::check_io(MsgOutType::from_stream(client, buffer, Endianness::Native), "Failed to receive message type")?;
        TestActor::check_io(if expected_type == msg_type {
            Ok(msg_type)
        } else {
            Err(std::io::Error::new(ErrorKind::InvalidData, "Wrong message type"))
        }, "Received wrong message type")
    }

    fn recv_at_deadline(client: &mut UnixStream) -> TestResult<()> {
        recv_msg_out_type(client, MsgOutType::AtDeadline).and(Ok(()))
    }

    #[test]
    fn send_at_deadline() {
        run_client_and_actor(|mut connector, _| {
            connector.send(MsgOut::AtDeadline).expect("Failed to send at_deadline")
        },
        recv_at_deadline)
    }

    fn recv_send_packet(client: &mut UnixStream, buffer: &mut [u8]) -> TestResult<SendPacket> {
        let _ = recv_msg_out_type(client, MsgOutType::SendPacket)?;

        let msg = TestActor::check_io(SendPacket::from_stream(client, buffer, Endianness::Native), "Failed to receive send_packet header")?;
        TestActor::check_io(if let Some(buffer) = buffer.get_mut(..(msg.packet.size as usize)) {
            TestActor::check_io(client.read_exact(buffer), "Failed to receive payload")?;
            Ok(msg)
        } else {
            Err(std::io::Error::new(ErrorKind::UnexpectedEof, "Buffer too small"))
        }, "Buffer too small to receive payload")
    }

    fn make_ref_send_packet() -> MsgOut<'static> {
        MsgOut::SendPacket(Duration::new(3, 200), 0, 1,
                       b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF")
    }

    fn send_send_packet_actor(client: &mut UnixStream) -> TestResult<()> {
        let mut buffer = vec!(0; usize::max(MsgOut::max_header_size(), crate::MAX_PACKET_SIZE));
        let msg = recv_send_packet(client, &mut buffer)?;
        if let MsgOut::SendPacket(ref_send_time, _, _, ref_payload) = make_ref_send_packet() {
            let seconds = ref_send_time.as_secs();
            let useconds = ref_send_time.subsec_micros();
            TestActor::check_eq(msg.send_time.seconds, seconds, "Received wrong value for Time::seconds")?;
            TestActor::check_eq(msg.send_time.useconds, useconds as u64, "Received wrong value for Time::useconds")?;
            let payload_len = msg.packet.size as usize;
            TestActor::check_eq(&buffer[..payload_len], ref_payload, "Received wrong payload")
        } else {
            unreachable!()
        }
    }

    #[test]
    fn send_send_packet() {
        run_client_and_actor(|mut connector, _| {
            connector.send(make_ref_send_packet()).expect("Failed to send send_packet")
        },
        send_send_packet_actor)
    }
}
