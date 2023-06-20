use std::process;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};
use once_cell::sync::OnceCell;
use crate::AfterDeadline;

const epoch_date : &str = "1970-01-01T00:00:00";

const MYPOLL_TAP  : u64 = 0;
const MYPOLL_STOP : u64 = 1;

fn print_usage(args: Vec<String>) {
    eprintln!("Usage: {} <tansiv socket> <unique sequence number> <container tap ipv4 address> <docker image name>", args[0]);
}

fn handle_tap_read(context : &crate::Context, tap_file : &Mutex<std::fs::File>) {
    let mut buf : [u8; crate::tap::MTU] = [0; crate::tap::MTU];
    let bytes_read = tap_file.lock().unwrap().read(&mut buf).unwrap();
    let packet = &buf[..bytes_read];
    let dest = match crate::tap::packet::get_destination_ipv4(packet) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    match context.send(dest, packet) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
}
// see deadline_handler in timer/qemu
fn handle_stop(context : &crate::Context) -> bool {
    if !context.flush_one_stopper_byte() {
        panic!()
    }
    match context.at_deadline() {
        AfterDeadline::NextDeadline(deadline) => {
            context.timer_context.set_next_deadline(deadline);
            false
        },
        AfterDeadline::EndSimulation => true
    }
}

pub fn run () {
    static context : OnceCell<Arc<crate::Context>> = OnceCell::new();
    static tap_file : OnceCell<Mutex<std::fs::File>> = OnceCell::new();
    static address_in_addr : OnceCell<u32> = OnceCell::new();

    let mut args: Vec<String> = std::env::args().collect();
    if args.len()<=4 {
        print_usage(args);
        std::process::exit(1);
    }
    let docker_image = args.remove(4);
    let ipv4_address_and_range = args.remove(3);
    let ipv4_address_only = match ipv4_address_and_range.split('/').next() {
        Some(result) => result,
        None => {
            eprintln!("IPv4 given is not in CIDR notation");
            std::process::exit(1);
        }
    };
    address_in_addr.set(match crate::vsg_address::from_str(&ipv4_address_only) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            print_usage(args);
            std::process::exit(1);
        }
    }).expect("main::run called multiple times?");
    let seqnum : u32 = match args[2].parse() {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            print_usage(args);
            std::process::exit(1);
        }
    };
    let tansiv_socket = args.remove(1);

    match crate::docker::trunc_shared_offset_file(seqnum) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let docker_container_id = match crate::docker::run_dockerpy(seqnum, &ipv4_address_and_range, &docker_image) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    };

    tap_file.set(Mutex::new(match crate::tap::get_rw_tap_file(&crate::docker::get_tap_interface_name(seqnum)) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    })).expect("main::run called multiple times?");

    let tansiv_args = [
        "--actor", &tansiv_socket,
        "--name", &ipv4_address_only,
        "--initial_time", epoch_date,
        "--docker_container_id", &docker_container_id,
        "--docker_sequence_number", &seqnum.to_string()
    ];

    context.set(match
        crate::init(
            tansiv_args,
            Box::new(|| { // receive callback
                let real_context = context.get().unwrap();
                let mut writable_tap = tap_file.get().unwrap().lock().unwrap();
                let mut buf = [0u8; crate::tap::MTU];
                while real_context.poll().is_some() {
                    let (src, dst, contents) = real_context.recv(&mut buf).unwrap();
                    if dst==*address_in_addr.get().unwrap() {
                        writable_tap.write(contents).unwrap(); // TODO: could this block when sending/receiving too many packets?
                    }
                }
            }),
            Box::new(|_| {})
        ) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("{}", e);
                process::exit(1);
            }
        }
    ).expect("main::run called multiple times?");

    let real_context = &context.get().unwrap();
    match real_context.start() {
        Ok(_) => (), // TODO: is the duration here useful?
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    let pollfd = match epoll::create(false) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    match epoll::ctl(pollfd, epoll::ControlOptions::EPOLL_CTL_ADD, tap_file.get().unwrap().lock().unwrap().as_raw_fd(), epoll::Event{events: epoll::Events::EPOLLIN.bits(), data: MYPOLL_TAP}) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };
    match epoll::ctl(pollfd, epoll::ControlOptions::EPOLL_CTL_ADD, context.get().unwrap().get_stopper_fd(), epoll::Event{events: epoll::Events::EPOLLIN.bits(), data: MYPOLL_STOP}) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    };

    loop {
        let mut epoll_events : [epoll::Event; 2] = [epoll::Event{data: 0, events: 0}; 2];
        match epoll::wait(pollfd, -1, &mut epoll_events) {
            Err(e) => eprintln!("{}", e),
            Ok(n) => match n {
                0 => eprintln!("epoll::wait returned 0 events"),
                1 => {
                    if epoll_events[0].data==MYPOLL_TAP {
                        handle_tap_read(real_context, tap_file.get().unwrap())
                    } else {
                        if handle_stop(real_context) { break; }
                    }
                },
                2 => { // presumably this means that both are available
                    handle_tap_read(real_context, tap_file.get().unwrap());
                    //if handle_stop(real_context) { break; } // handle it next time, once there
                    //are no more packets, having 2 here means the container is already stopped:
                    //no need to rush
                }
                _ => panic!("epoll::wait returned an incorrect number of events")
            }
        }
    }
}
