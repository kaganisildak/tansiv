use crate::buffer_pool::BufferPool;
use crate::bytes_buffer::BytesBuffer;
use crate::connector::MsgFbInitializer;
use crate::flatbuilder_buffer::FbBuilderInitializer;
use flatbuffers::FlatBufferBuilder;
use std::io::Result;
use std::os::unix::net::UnixStream;
use super::{Connector, MsgIn, MsgOut};


#[derive(Debug)]
pub(crate) struct UnixConnector {
    // No concurrency
    actor: UnixStream,
    // Concurrency: Buffers are:
    // - allocated and filled by the deadline handler,
    // - kept around and freed by application code.
    // BufferPool uses interior mutability for concurrent allocation and freeing of buffers.
    input_buffer_pool: BufferPool<BytesBuffer>,
    // No concurrency
    scratch_builder: FlatBufferBuilder<'static>,
}

impl Connector for UnixConnector {
    fn new(config: &crate::Config) -> Result<UnixConnector> {
        let actor_stream = UnixStream::connect(&config.actor_socket)?;

        let input_buffer_pool = BufferPool::new(crate::MAX_PACKET_SIZE, config.num_buffers.get());
        Ok(UnixConnector {
            actor: actor_stream,
            input_buffer_pool: input_buffer_pool,
            scratch_builder: MsgFbInitializer::init(crate::MAX_PACKET_SIZE)
        })
    }

    fn recv(&mut self) -> Result<MsgIn> {
        let stream = &mut self.actor;
        let buffer_pool = &self.input_buffer_pool;
        MsgIn::recv(stream, buffer_pool)
    }

    fn send(&mut self, msg: MsgOut) -> Result<()> {
        let stream = &mut self.actor;
        let scratch_builder = &mut self.scratch_builder;
        // TODO(msimonin): test that we reset the buffer correctly when sending several messages in a row:w
        scratch_builder.reset();
        msg.send(stream, scratch_builder)
    }
}

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use crate::buffer_pool::BufferPool;
    use crate::bytes_buffer::BytesBuffer;
    use crate::connector::{FbBuffer, MsgIn, MsgOut};
    use log::{error, info};
    use std::fmt;
    use std::io::Read;
    use std::ops::{Deref, DerefMut};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};

    /// Actor uses this special exit code
    /// when an assertion failed in the testing logic
    static ACTOR_FAILURE_EXIT_CODE: i32 = 123;

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
        // Set to None in wait to indicate that the child is gone forever
        pid: Option<nix::unistd::Pid>,
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
                   let exit_code = match TestActor::run(server, actor_fn) {
                        Err(_) => ACTOR_FAILURE_EXIT_CODE,
                        _ => 0
                    };
                    // The server socket is deleted when TestActorDesc is dropped
                    // std::fs::remove_file(&path).expect(&format!("Server socket '{:?}' could not be removed", &path));
                    std::process::exit(exit_code)
                },
                ForkResult::Parent { child: child_pid, .. } => {
                    TestActorDesc {
                        socket_path: path.as_ref().to_path_buf(),
                        pid: Some(child_pid),
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

        /// Wait the actor to terminate
        ///
        /// NOTE(msimonin): This isn't idempotent (will return an Err the second time)
        pub fn wait(&mut self) ->  Result<i32, ()> {
            use nix::sys::wait::{waitpid, WaitStatus};

            match self.pid {
                // the child actor is already gone
                None => Err(()),
                // wait for it
                Some(pid) => {
                    match waitpid(pid, None).unwrap() {
                        WaitStatus::Exited(_, status) => {
                            self.pid = None;
                            Ok(status)
                        }
                        _ => Err(())
                    }
                }
            }
        }

    }

    impl Drop for TestActorDesc {
        fn drop(&mut self) {
            use nix::sys::signal::{kill, Signal};
            use nix::sys::wait::{waitpid};
            Self::remove_file_if_present(&self.socket_path).expect(&format!("Server socket '{:?}' could not be removed", &self.socket_path));
            match self.pid {
                None => (),
                Some(pid) => {
                    #[allow(unused_must_use)] {
                        kill(pid, Signal::SIGTERM);
                        // Not required for concurrency but avoids interleaving traces
                        waitpid(pid, None);
                    }
                }
            }
        }
    }

    // Actor-side API
    pub struct TestActor {
        client: UnixStream,
        input_buffer_pool: BufferPool<BytesBuffer>,
        input_fb_buffer_pool: BufferPool<FbBuffer>,
    }

    impl TestActor {
        fn new(client: UnixStream) -> TestActor {

            TestActor {
                client: client,
                // TODO: Do not hardcode a limit of 100 buffers
                input_buffer_pool: BufferPool::new(crate::MAX_PACKET_SIZE, 100),
                input_fb_buffer_pool: BufferPool::<FbBuffer>::new(crate::MAX_PACKET_SIZE, 100),
            }
        }

        fn run<F>(server: UnixListener, actor_fn: F) -> TestResult<()>
            where F: FnOnce(&mut TestActor) -> TestResult<()> {
            info!("Server listening at address {:?}", server);

            match server.accept() {
                Ok((client, address)) => {
                    let mut actor = TestActor::new(client);

                    info!("New client: {:?}", address);
                    match actor_fn(&mut actor) {
                        Err(e) => {
                            error!("Actor failed: {:?}", e);
                            // send back the error
                            Err(e)
                        }
                        Ok(_) => {
                            // Just drain until the VM ends, do not make it fail when sending messages
                            if actor.client.shutdown(std::net::Shutdown::Write).is_err() {
                                error!("Shutdown failed")
                            } else {
                                for _ in actor.client.bytes() {
                                }
                            }
                            // send back the success
                            Ok(())
                        },
                    }
                },
                Err(e) => {
                    error!("Failed to accept connection: {:?}", e);
                    Err(Error::new(crate::error::Error::from(e), "Failed to accept connection"))
                },
            }
        }

        pub fn check<T, E: Into<crate::error::Error>>(result: std::result::Result<T, E>, context: &'static str) -> TestResult<T> {
            result.map_err(|e| Error::new(Into::<crate::error::Error>::into(e), context))
        }

        pub fn check_eq<T: PartialEq>(left: T, right: T, context: &'static str) -> TestResult<()> {
            Self::check(if left == right {
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
            let fb_buffer_pool = &self.input_fb_buffer_pool;
            Self::check(msg.send(stream, fb_buffer_pool), "Send failed")
        }

        pub fn recv(&mut self) -> TestResult<MsgOut> {
            let stream = &mut self.client;
            let buffer_pool = &self.input_buffer_pool;
            let fb_buffer_pool = &self.input_fb_buffer_pool;
            Self::check(MsgOut::recv(stream, buffer_pool, fb_buffer_pool), "Recv failed")
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
    use crate::{Config, connector::*};
    use std::os::unix::net::UnixStream;
    use structopt::StructOpt;
    use super::test_helpers::*;
    use crate::test_helpers::init;

    // TODO(msimonin) Recover this test
    // Its blocking for some unknown reason when draining the messages.
    #[test]
    fn valid_server_path() {
        init();
        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();

        let connector = UnixConnector::new(&config);
        assert!(connector.is_ok());

        // actor must finish gracefully its exection here
        // let status = actor.wait().unwrap();
        // assert_eq!(0, status, "Actor process reported an error");

        drop(actor);
    }

    #[test]
    fn invalid_server_path() {
        init();

        let actor = TestActorDesc::new("titi", TestActor::dummy_actor);
        let config = Config::from_iter_safe(&["-amust not exist", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();

        assert!(UnixConnector::new(&config).is_err());

        drop(actor);
    }

    #[test]
    fn valid_input_buffer_size() {
        use std::ops::DerefMut;

        init();

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
        init();

        let mut actor = TestActorDesc::new("titi", actor_fn);
        let config = Config::from_iter_safe(&["-atiti", "-n10.0.0.1", "-t1970-01-02T00:00:00"]).unwrap();
        let connector = UnixConnector::new(&config).unwrap();

        client_fn(connector);

        // we let the actor terminates its execution by waiting for it
        // if all the tests in the actor process pass its exit code is 0
        // so we check the exit code here
        let status = actor.wait().unwrap();
        assert_eq!(0, status, "Actor process reported an error");
    }

    static GO_TO_DEADLINE_SECONDS : u64 = 2;
    static GO_TO_DEADLINE_USECONDS: u64 = 100;

    // If types used to represent seconds change, the test will just not compile.
    // If types used to represent seconds have to differ between network representation and
    // internal representation, then uncomment and adapt the next test case.
    #[test]
    fn go_to_deadline_seconds_not_overflowable() {
        use std::time::Duration;
        use super::super::MsgIn;

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
        send_go_to_deadline(actor, 0, std::u64::MAX)
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

    fn send_go_to_deadline(socket: &mut UnixStream, seconds: u64, useconds: u64) -> TestResult<()> {
        // we don't want to use the create_goto_deadline helper here since we
        // want to also test a potential overflow coming from the wire
        // Reminder: as for now we have a Time(u64, u64) on the wire while were
        // using a Duration(u64, u32) in the lib
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let time = tansiv::Time::new(seconds, useconds);
        let goto_deadline = tansiv::GotoDeadline::create(&mut builder, &tansiv::GotoDeadlineArgs {
            time: Some(&time)
        });
        let msg = tansiv::FromTansivMsg::create(&mut builder, &tansiv::FromTansivMsgArgs{
            content_type: tansiv::FromTansiv::GotoDeadline,
            content: Some(goto_deadline.as_union_value()),
            ..Default::default()
        });

        builder.finish_size_prefixed(msg, None);
        TestActor::check(socket.write_all(builder.finished_data()), "Failed to go to deadline message")
    }

    fn recv_go_to_deadline_actor(actor: &mut TestActor) -> TestResult<()> {
        send_go_to_deadline(actor, GO_TO_DEADLINE_SECONDS, GO_TO_DEADLINE_USECONDS)
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
                    assert_eq!(seconds, GO_TO_DEADLINE_SECONDS);
                    let useconds = deadline.subsec_micros();
                    assert_eq!(useconds as u64, GO_TO_DEADLINE_USECONDS);
                },
                _ => assert!(false),
            }
        },
        recv_go_to_deadline_actor)
    }

    static DELIVER_PACKET_SIZE: u32 = 42;
    static DELIVER_PACKET_SRC: u32 = 0;
    static DELIVER_PACKET_DST: u32 = 1;
    static PACKET_PAYLOAD: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";

    fn recv_deliver_packet_payload_too_big_actor(actor: &mut TestActor) -> TestResult<()> {
        let size = (crate::MAX_PACKET_SIZE + 1) as u32;
        let mut big_payload = vec!(0; size as usize);
        big_payload[size as usize - 1] = 1;
        send_deliver_packet(actor, DELIVER_PACKET_SRC, DELIVER_PACKET_DST, &big_payload)
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

    fn send_deliver_packet(socket: &mut UnixStream, src: u32, dst: u32, payload: &[u8]) -> TestResult<()> {
        // we want to send a flatbuffer
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        create_deliver_packet(&mut builder, src, dst, payload);
        TestActor::check(socket.write_all(builder.finished_data()), "Failed to send payload")
    }

    fn recv_deliver_packet_actor(actor: &mut TestActor) -> TestResult<()> {
        assert_eq!(PACKET_PAYLOAD.len(), DELIVER_PACKET_SIZE as usize);
        send_deliver_packet(actor, DELIVER_PACKET_SRC, DELIVER_PACKET_DST, &PACKET_PAYLOAD)
    }

    #[test]
    fn recv_deliver_packet__() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
            assert!(msg.is_ok());
            let msg = msg.unwrap();
            match msg {
                MsgIn::DeliverPacket(m) => {
                    let payload = m.payload();
                    assert_eq!(payload, PACKET_PAYLOAD)
                },
                _ => assert!(false),
            }
        },
        recv_deliver_packet_actor)
    }

    // fn recv_msg_out_type(client: &mut UnixStream, expected_type: MsgOutType) -> TestResult<MsgOutType> {
    //     let mut buffer = vec!(0; MsgOutType::NUM_BYTES);
    //     let buffer = buffer.as_mut_slice();
    //     let msg_type = TestActor::check(MsgOutType::from_stream(client, buffer, Endianness::Native), "Failed to receive message type")?;
    //     TestActor::check(if expected_type == msg_type {
    //         Ok(msg_type)
    //     } else {
    //         Err(std::io::Error::new(ErrorKind::InvalidData, "Wrong message type"))
    //     }, "Received wrong message type")
    // }

    fn recv_gibberish_actor(actor: &mut  TestActor) -> TestResult<()> {
        let buf = [0; crate::MAX_PACKET_SIZE];
        TestActor::check((*actor).write_all(&buf), "Failed to send payload")
    }

    #[test]
    fn recv_gibberish() {
        run_client_and_actor(|mut connector| {
            let msg = connector.recv();
            match msg {
                Ok(_) => assert!(false),
                Err(e) => assert_eq!(e.kind(), ErrorKind::InvalidData),
            }
        },
        recv_gibberish_actor)
    }


    fn recv_at_deadline(actor: &mut TestActor) -> TestResult<()> {
        //recv_msg_out_type(actor, MsgOutType::AtDeadline).and(Ok(()))
        // FIXME(msimonin): This is basically a duplication of MsgOut::recv
        let msg: MsgOut = actor.recv()?;
        TestActor::check(match msg {
            MsgOut::AtDeadline => Ok(()),
             _ => Err(std::io::Error::new(ErrorKind::InvalidData, "Wrong message type"))
        }, "Received wrong message type")
    }

    #[test]
    fn send_at_deadline() {
        run_client_and_actor(|mut connector| {
            connector.send(MsgOut::AtDeadline).expect("Failed to send at_deadline")
        },
        recv_at_deadline)
    }

    #[test]
    fn send_at_deadline_twice() {
        // Test that we correctly reset the flatbuffer
        run_client_and_actor(|mut connector| {
            connector.send(MsgOut::AtDeadline).expect("Failed to send at_deadline");
            connector.send(MsgOut::AtDeadline).expect("Failed to send at_deadline")
        },
        recv_at_deadline)
    }


    // fn recv_send_packet(client: &mut UnixStream, buffer: &mut [u8]) -> TestResult<SendPacket> {
    //     let _ = recv_msg_out_type(client, MsgOutType::SendPacket)?;

    //     let msg = TestActor::check(SendPacket::from_stream(client, buffer, Endianness::Native), "Failed to receive send_packet header")?;
    //     TestActor::check(if let Some(buffer) = buffer.get_mut(..(msg.packet.size as usize)) {
    //         TestActor::check(client.read_exact(buffer), "Failed to receive payload")?;
    //         Ok(msg)
    //     } else {
    //         Err(std::io::Error::new(ErrorKind::UnexpectedEof, "Buffer too small"))
    //     }, "Buffer too small to receive payload")
    // }

    fn make_ref_send_packet() -> MsgOut {
        let msg = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEF";
        let buffer_pool = BufferPool::<FbBuilder<MsgFbInitializer>>::new(crate::MAX_PACKET_SIZE, 1);
        let buffer = buffer_pool.allocate_buffer(msg.len()).expect("allocate_buffer failed");

        let send_time = Duration::new(3, 200);
        let send_packet_builder = SendPacketBuilder::new(0u32, 1u32, send_time, msg, buffer).unwrap();
        MsgOut::SendPacket(send_packet_builder.finish(send_time))
    }

    fn send_send_packet_actor(actor: &mut TestActor) -> TestResult<()> {
        let actual_msg = actor.recv()?;
        let expected = TestActor::check(
            match make_ref_send_packet() {
            MsgOut::SendPacket(builder) => Ok(builder),
            _ => Err(std::io::Error::new(ErrorKind::InvalidData, "Wrong message type"))
        }, "Wrong message type crafter (critical)")?;

        match actual_msg {
            MsgOut::SendPacket(actual) =>  {
                // expected and actual are both of type Buffer<FbBuffer> which
                // deref at some point to a FlatBufferBuilder So to compare the
                // two we can check that we can deserialize them (unwrap) and
                // that that the deserialized data are the same.
                let expected = flatbuffers::size_prefixed_root::<tansiv::ToTansivMsg>(expected.finished_data()).unwrap();
                let actual = flatbuffers::size_prefixed_root::<tansiv::ToTansivMsg>(actual.finished_data()).unwrap();
                TestActor::check_eq(expected, actual, "Messages are differents")


            },
            _ => TestActor::check(Err(std::io::Error::new(ErrorKind::InvalidData, "Wrong message type")), "Wrong message type")
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
