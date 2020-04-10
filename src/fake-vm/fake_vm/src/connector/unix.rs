use crate::buffer_pool::BufferPool;
use std::io::Result;
use std::os::unix::net::UnixStream;
use super::{Connector, Endianness, MsgIn, MsgOut};

#[derive(Debug)]
pub(crate) struct UnixConnector {
    // No concurrency
    actor: UnixStream,
    // No concurrency
    scratch_buffer: Vec<u8>,
    // Concurrency: Buffers are:
    // - allocated and filled by the deadline handler,
    // - kept around and freed by application code.
    // BufferPool uses interior mutability for concurrent allocation and freeing of buffers.
    input_buffer_pool: BufferPool,
}

impl Connector for UnixConnector {
    fn new(config: &crate::Config) -> Result<UnixConnector> {
        let actor_stream = UnixStream::connect(&config.actor_socket)?;

        let scratch_buffer_size = usize::max(MsgIn::max_header_size(), MsgOut::max_header_size());
        let mut scratch_buffer = Vec::with_capacity(scratch_buffer_size);
        scratch_buffer.resize(scratch_buffer_size, 0);

        let input_buffer_pool = BufferPool::new(crate::MAX_PACKET_SIZE, config.num_buffers.get());

        Ok(UnixConnector {
            actor: actor_stream,
            scratch_buffer: scratch_buffer,
            input_buffer_pool: input_buffer_pool,
        })
    }

    fn recv(&mut self) -> Result<MsgIn> {
        let stream = &mut self.actor;
        let buffer = self.scratch_buffer.as_mut_slice();
        let buffer_pool = &self.input_buffer_pool;
        MsgIn::recv(stream, buffer, buffer_pool, Endianness::Native)
    }

    fn send(&mut self, msg: MsgOut) -> Result<()> {
        let stream = &mut self.actor;
        let buffer = self.scratch_buffer.as_mut_slice();
        msg.send(stream, buffer, Endianness::Native)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use binser::Endianness;
    use crate::buffer_pool::BufferPool;
    use crate::connector::{MsgIn, MsgOut};
    use log::{error, info};
    use std::fmt;
    use std::io::Read;
    use std::ops::{Deref, DerefMut};
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

    // Application-side API
    pub struct TestActorDesc {
        socket_path: PathBuf,
        pid: nix::unistd::Pid,
    }

    // Application-side API
    impl TestActorDesc {
        pub fn new<P: AsRef<Path> + std::fmt::Debug, F>(path: P, actor_fn: F) -> TestActorDesc
            where F: FnOnce(&mut TestActor) -> TestResult<()> {
            use nix::unistd::{fork, ForkResult};

            Self::remove_file_if_present(&path).expect(&format!("Server socket path '{:?}' is busy", &path));
            let server = UnixListener::bind(&path).expect(&format!("Could not create server socket '{:?}'", &path));
            let fork_res = fork().expect("Forking server failed");
            match fork_res {
                ForkResult::Child => {
                    TestActor::run(server, actor_fn);
                    // The server socket is deleted when TestActorDesc is dropped
                    // std::fs::remove_file(&path).expect(&format!("Server socket '{:?}' could not be removed", &path));
                    std::process::exit(0)
                },
                ForkResult::Parent { child: child_pid, .. } => {
                    TestActorDesc {
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

    impl Drop for TestActorDesc {
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
    pub struct TestActor {
        client: UnixStream,
        scratch_buffer: Vec<u8>,
        input_buffer_pool: BufferPool,
    }

    impl TestActor {
        fn new(client: UnixStream) -> TestActor {
            let scratch_buffer_size = usize::max(MsgIn::max_header_size(), MsgOut::max_header_size());
            let mut scratch_buffer = Vec::with_capacity(scratch_buffer_size);
            scratch_buffer.resize(scratch_buffer_size, 0);

            TestActor {
                client: client,
                scratch_buffer: scratch_buffer,
                // TODO: Do not hardcode a limit of 100 buffers
                input_buffer_pool: BufferPool::new(crate::MAX_PACKET_SIZE, 100),
            }
        }

        fn run<F>(server: UnixListener, actor_fn: F) -> ()
            where F: FnOnce(&mut TestActor) -> TestResult<()> {
            info!("Server listening at address {:?}", server);

            match server.accept() {
                Ok((client, address)) => {
                    let mut actor = TestActor::new(client);

                    info!("New client: {:?}", address);
                    match actor_fn(&mut actor) {
                        Err(e) => error!("Actor failed: {:?}", e),
                        _ => {
                            // Just drain until the VM ends, do not make it fail when sending messages
                            if actor.client.shutdown(std::net::Shutdown::Write).is_err() {
                                error!("Shutdown failed")
                            } else {
                                for _ in actor.client.bytes() {
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

        pub fn dummy_actor(_actor: &mut TestActor) -> TestResult<()> {
            Ok(())
        }

        pub fn send(&mut self, msg: MsgIn) -> TestResult<()> {
            let stream = &mut self.client;
            let buffer = self.scratch_buffer.as_mut_slice();
            Self::check_io(msg.send(stream, buffer, Endianness::Native), "Send failed")
        }

        pub fn recv(&mut self) -> TestResult<MsgOut> {
            let stream = &mut self.client;
            let buffer = self.scratch_buffer.as_mut_slice();
            let buffer_pool = &self.input_buffer_pool;
            Self::check_io(MsgOut::recv(stream, buffer, buffer_pool, Endianness::Native), "Recv failed")
        }
    }

    impl Deref for TestActor {
        type Target = UnixStream;

        fn deref(&self) -> &UnixStream {
            &self.client
        }
    }

    impl DerefMut for TestActor {
        fn deref_mut(&mut self) -> &mut UnixStream {
            &mut self.client
        }
    }
}

#[cfg(test)]
mod test {
    use binser::{ToBytes, ToStream, SizedAsBytes};
    use crate::{Config, connector::*};
    use std::ops::{Deref, DerefMut};
    use std::os::unix::net::UnixStream;
    use structopt::StructOpt;
    use super::test_helpers::*;

    #[test]
    fn valid_server_path() {
        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();

        let connector = UnixConnector::new(&config);
        assert!(connector.is_ok());
        assert_eq!(connector.unwrap().scratch_buffer.len(), usize::max(MsgIn::max_header_size(), MsgOut::max_header_size()));

        drop(actor);
    }

    #[test]
    fn invalid_server_path() {
        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-amust not exist", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();

        assert!(UnixConnector::new(&config).is_err());

        drop(actor);
    }

    #[test]
    fn valid_input_buffer_size() {
        use std::ops::DerefMut;

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();

        let connector = UnixConnector::new(&config).unwrap();
        // Check the length as a borrowed mutable slice because borrowing as an immutable slice
        // initially returns an empty slice
        let mut input_buffer = connector.input_buffer_pool.allocate_buffer(crate::MAX_PACKET_SIZE).unwrap();
        assert_eq!(input_buffer.deref_mut().len(), crate::MAX_PACKET_SIZE);

        drop(actor);
    }

    fn run_client_and_actor<C, A>(client_fn: C, actor_fn: A)
        where C: FnOnce(UnixConnector) -> (),
              A: FnOnce(&mut TestActor) -> TestResult<()>,
              A: Send + 'static {
        let actor = TestActorDesc::new("titi", actor_fn);
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();
        let connector = UnixConnector::new(&config).unwrap();

        client_fn(connector);

        drop(actor);
    }

    fn send_partial_msg_type(actor: &mut TestActor) -> TestResult<()> {
        let mut buffer = [0; MsgInType::NUM_BYTES];
        TestActor::check_io(MsgInType::GoToDeadline.to_bytes(&mut buffer, Endianness::Native), "Failed to serialize message type")?;
        TestActor::check_io(actor.write_all(&buffer[..(MsgInType::NUM_BYTES - 1)]), "Failed to send partial message type")
    }

    #[test]
    fn recv_partial_msg_type() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        send_partial_msg_type)
    }

    fn send_invalid_msg_type(actor: &mut TestActor) -> TestResult<()> {
        let invalid_type = (MsgInType::GoToDeadline as u32 + 1) * (MsgInType::DeliverPacket as u32 + 1) + 1;
        let mut buffer = [0; u32::NUM_BYTES];
        TestActor::check_io(invalid_type.to_stream(actor.deref_mut(), &mut buffer, Endianness::Native), "Failed to send message type")
    }

    #[test]
    fn recv_invalid_msg_type() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
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

    fn send_partial_go_to_deadline(actor: &mut TestActor) -> TestResult<()> {
        let mut buffer = vec!(0; MsgIn::max_header_size());
        let mut buffer = buffer.as_mut_slice();
        let msg_type = MsgInType::GoToDeadline;

        TestActor::check_io(msg_type.to_stream(actor.deref_mut(), buffer, Endianness::Native), "Failed to send message type")?;
        TestActor::check_io(GO_TO_DEADLINE.to_bytes(&mut buffer, Endianness::Native), "Failed to serialize GO_TO_DEADLINE")?;
        TestActor::check_io(actor.write_all(&buffer[..(GoToDeadline::NUM_BYTES - 1)]), "Failed to send partial go_to_deadline")
    }

    #[test]
    fn recv_partial_go_to_deadline() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
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

    // fn recv_go_to_deadline_oob_seconds_actor(actor: &mut TestActor) -> TestResult<()> {
        // let msg = GoToDeadline {
            // deadline: Time {
                // seconds: std::u64::MAX,
                // useconds: 0,
            // },
        // };
        // send_go_to_deadline(&mut actor.client, msg)
    // }

    // #[test]
    // fn recv_go_to_deadline_oob_seconds() {
        // run_client_and_actor(|mut connector| {
            // let error = connector.recv().unwrap_err();
            // assert_eq!(error.kind(), ErrorKind::InvalidData);
        // },
        // recv_go_to_deadline_oob_seconds_actor)
    // }

    fn recv_go_to_deadline_oob_useconds_actor(actor: &mut TestActor) -> TestResult<()> {
        let msg = GoToDeadline {
            deadline: Time {
                seconds: 0,
                useconds: std::u64::MAX,
            },
        };
        send_go_to_deadline(actor, msg)
    }

    #[test]
    fn recv_go_to_deadline_oob_useconds() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
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

    fn recv_go_to_deadline_actor(actor: &mut TestActor) -> TestResult<()> {
        send_go_to_deadline(actor, GO_TO_DEADLINE)
    }

    #[test]
    fn recv_go_to_deadline() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
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

    fn recv_partial_deliver_packet_header_actor(actor: &mut TestActor) -> TestResult<()> {
        send_partial_deliver_packet_header(actor, DELIVER_PACKET)
    }

    #[test]
    fn recv_partial_deliver_packet_header() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_deliver_packet_header_actor)
    }

    fn recv_partial_deliver_packet_payload_actor(actor: &mut TestActor) -> TestResult<()> {
        send_deliver_packet(actor, DELIVER_PACKET, &PACKET_PAYLOAD[1..])
    }

    #[test]
    fn recv_partial_deliver_packet_payload() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::UnexpectedEof),
            }
        },
        recv_partial_deliver_packet_payload_actor)
    }

    fn recv_deliver_packet_payload_too_big_actor(actor: &mut TestActor) -> TestResult<()> {
        let msg = DeliverPacket {
            packet: Packet {
                size: (crate::MAX_PACKET_SIZE + 1) as u32,
            },
        };
        let mut big_payload = vec!(0; msg.packet.size as usize);
        big_payload[msg.packet.size as usize - 1] = 1;
        send_deliver_packet(actor, msg, &big_payload)
    }

    #[test]
    fn recv_deliver_packet_payload_too_big() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
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

    fn recv_deliver_packet_actor(actor: &mut TestActor) -> TestResult<()> {
        assert_eq!(PACKET_PAYLOAD.len(), DELIVER_PACKET.packet.size as usize);
        send_deliver_packet(actor, DELIVER_PACKET, &PACKET_PAYLOAD)
    }

    #[test]
    fn recv_deliver_packet() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
            assert!(msg.is_ok());
            let msg = msg.unwrap();
            match msg {
                MsgIn::DeliverPacket(payload) => {
                    assert_eq!(payload.deref(), PACKET_PAYLOAD)
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

    fn recv_at_deadline(actor: &mut TestActor) -> TestResult<()> {
        recv_msg_out_type(actor, MsgOutType::AtDeadline).and(Ok(()))
    }

    #[test]
    fn send_at_deadline() {
        run_client_and_actor(|mut connector| {
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

    fn make_ref_send_packet() -> MsgOut {
        let msg = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";
        let mut buffer = BufferPool::new(msg.len(), 1).allocate_buffer(msg.len()).expect("allocate_buffer failed");
        buffer.copy_from_slice(msg);

        MsgOut::SendPacket(Duration::new(3, 200), 0, 1, buffer)
    }

    fn send_send_packet_actor(actor: &mut TestActor) -> TestResult<()> {
        let mut buffer = vec!(0; usize::max(MsgOut::max_header_size(), crate::MAX_PACKET_SIZE));
        let msg = recv_send_packet(actor, &mut buffer)?;
        if let MsgOut::SendPacket(ref_send_time, _, _, ref_payload) = make_ref_send_packet() {
            let seconds = ref_send_time.as_secs();
            let useconds = ref_send_time.subsec_micros();
            TestActor::check_eq(msg.send_time.seconds, seconds, "Received wrong value for Time::seconds")?;
            TestActor::check_eq(msg.send_time.useconds, useconds as u64, "Received wrong value for Time::useconds")?;
            let payload_len = msg.packet.size as usize;
            TestActor::check_eq(&buffer[..payload_len], ref_payload.deref(), "Received wrong payload")
        } else {
            unreachable!()
        }
    }

    #[test]
    fn send_send_packet() {
        run_client_and_actor(|mut connector| {
            connector.send(make_ref_send_packet()).expect("Failed to send send_packet")
        },
        send_send_packet_actor)
    }
}
