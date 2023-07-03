pub mod main;

use nix::sys::socket;
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::fs::OpenOptions;
use nix::sys::mman;
use std::os::fd::RawFd;
use std::process::Child;
use std::os::fd::{FromRawFd, AsRawFd};
use std::convert::TryInto;
use std::io::{Read, Write};
use bytemuck;
use bytemuck::Zeroable;

const dockerpy_path : &str = "../../bin/docker.py";
const offset_shm_prefix : &str = "/dev/shm/tansiv-time";
const ldpreload_path : &str = "ldpreloadoffset/lib.so";
const ldpreload_dest : &str = "/tansiv-preload.so";
const tap_prefix : &str = "tapt";
const cgroup_docker_prefix : &str = "/sys/fs/cgroup/unified/docker/";
const stopper_path : &str = "../../container_stopper/container_stopper";

const timespec_size : usize = std::mem::size_of::<libc::timespec>();

const nix_mmap_any_address : *mut core::ffi::c_void = 0 as *mut core::ffi::c_void; // <0.26
//const nix_mmap_any_address : Option<core::num::NonZeroUsize> = None; // >=0.26

pub fn get_offset_shm_path(seqnum: u32) -> String {
    offset_shm_prefix.to_owned() + "-" + &seqnum.to_string()
}
pub fn get_tap_interface_name(seqnum: u32) -> String {
    tap_prefix.to_owned() + &seqnum.to_string()
}
pub fn get_cgroup_freeze_path(container_id: &str) -> String {
    cgroup_docker_prefix.to_owned() + container_id + "/cgroup.freeze"
}

// Ensures that the offset shm file exists
pub fn trunc_shared_offset_file(seqnum: u32) -> Result<(), std::io::Error> {
    std::fs::File::create(get_offset_shm_path(seqnum)).map(|_| ()) // or .err
}

#[derive(Debug)]
pub enum DockerPyError {
    IoError(std::io::Error),
    StringError(std::string::FromUtf8Error),
    SubprocessFail(String),
    ErrorDescription(String),
}
impl From<&str> for DockerPyError {
    fn from (item: &str) -> Self {
        Self::ErrorDescription(item.to_owned())
    }
}
impl From<std::io::Error> for DockerPyError {
    fn from(item: std::io::Error) -> Self {
        Self::IoError(item)
    }
}
impl From<std::string::FromUtf8Error> for DockerPyError {
    fn from(item: std::string::FromUtf8Error) -> Self {
        Self::StringError(item)
    }
}

// Returns container ID
pub fn run_dockerpy(seqnum: u32, ipv4: &str, docker_image: &str) -> Result<String, DockerPyError> {
    let tap_name : &str = &get_tap_interface_name(seqnum);
    let binding = std::fs::canonicalize(ldpreload_path)?;
    let ldpreload_source : &str = binding.to_str().ok_or("invalid path to preloaded library")?;
    let output =
        std::process::Command::
            new("python3").args([
                dockerpy_path,
                "--create-tap",
                tap_name,
                "--create-docker-network",
                &("tansiv-".to_owned() + tap_name),
                "--use-ip",
                ipv4,
                "--docker-mounts",
                &("type=bind,".to_owned() +
                    "source=" + &get_offset_shm_path(seqnum) + "," +
                    "destination=" + offset_shm_prefix + ",readonly=true"),
                &("type=bind,".to_owned() +
                    "source=" + ldpreload_source + "," +
                    "destination=" + ldpreload_dest + ",readonly=true"),
                "--docker-image",
                docker_image,
                "--docker-program",
                "sleep inf" // other commands can be specified using docker exec
            ]).output()?;
    std::io::stderr().lock().write_all(&output.stderr);
    if !output.status.success() {
        return Err(DockerPyError::SubprocessFail(String::from_utf8(output.stderr)?));
    }
    return Ok(String::from_utf8(output.stdout)?.lines().next().ok_or("docker.py didn’t output anything")?.to_owned());
}

fn serialize_socket_fd(fd : RawFd) -> String {
    fd.to_string()
}

#[derive(Debug)]
pub enum StopperStartError {
    NixError(nix::Error),
    IoError(std::io::Error),
    ErrorMessage(String),
}
impl From<nix::Error> for StopperStartError {
    fn from(item: nix::Error) -> Self {
        Self::NixError(item)
    }
}
impl From<std::io::Error> for StopperStartError {
    fn from(item: std::io::Error) -> Self {
        Self::IoError(item)
    }
}
impl From<&str> for StopperStartError {
    fn from(item: &str) -> Self {
        Self::ErrorMessage(item.to_owned())
    }
}

//TODO: There exists https://doc.rust-lang.org/std/os/unix/net/struct.UnixStream.html#method.pair
pub fn start_stopper(seqnum: u32, container_id: &str) -> Result<(Child, UnixStream), StopperStartError> {
    // Create the socketpair
    let protocol_zero : socket::SockProtocol = unsafe {
        // socket(2) states that
        // Normally only a single protocol exists to support a particular
        // sockettype within a given protocol family, in which case protocol
        // can be specified as 0.
        // SockProtocol is an enum mapping to the libc crate’s protocols
        // represented as an i32, but doesn’t include a 0 value
        std::mem::transmute::<i32, socket::SockProtocol>(0)
    };
    let (socket_a,socket_b) = socket::socketpair(socket::AddressFamily::Unix, socket::SockType::Stream, protocol_zero, socket::SockFlag::empty())?;
    // Set one of the sockets to close in the spawned process as it won’t
    // be used by it.
    fcntl(socket_a, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))?; // or .except?
    let mut pairstream = unsafe { UnixStream::from_raw_fd(socket_a) };
    let child =
        std::process::Command::new(stopper_path)
            .args([
                  get_cgroup_freeze_path(container_id),
                  serialize_socket_fd(socket_b),
                  get_offset_shm_path(seqnum)
            ]).spawn()?;
    // Close the second socket, now used by the child
    nix::unistd::close(socket_b).unwrap(); // should we kill the child here?

    // Wait for mmap file to be ready
    loop {
        let mut buf : [u8; 1] = Default::default();
        match pairstream.read(&mut buf) {
            Ok(1) => break,
            Ok(0) => return Err("Stopper didn’t signal readiness".into()),
            Err(e) => match e.kind() {
                std::io::ErrorKind::Interrupted => (),
                _ => return Err(e.into()),
            },
            _ => return Err("read returned incorrect value".into()),
        }
    }


    return Ok((child, pairstream));
}

#[derive(Debug)]
pub enum MmapError {
    NixError(nix::Error),
    IoError(std::io::Error),
}
impl From<nix::Error> for MmapError {
    fn from(item: nix::Error) -> Self {
        Self::NixError(item)
    }
}
impl From<std::io::Error> for MmapError {
    fn from(item: std::io::Error) -> Self {
        Self::IoError(item)
    }
}

// TODO: could implement munmap as Drop trait
pub struct SharedTimespec {
    pointer: *const libc::timespec,
}
impl SharedTimespec {
    pub fn get_timespec(&self) -> libc::timespec {
        unsafe { self.pointer.read_volatile() }
    }
}
impl std::fmt::Debug for SharedTimespec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedTimespec")
         .finish_non_exhaustive()
    }
}
unsafe impl std::marker::Send for SharedTimespec {} // TODO: this might break horribly

//pub fn wait_and_mmap_offset(seqnum: u32) -> Result<SharedTimespec, MmapError> {
//    let binding = get_offset_shm_path(seqnum);
//    let offset_shm_path = Path::new(&binding);
//    while !offset_shm_path.try_exists()? { };
//    let offset_shm_file = OpenOptions::new().read(true).open(offset_shm_path)?;
//    unsafe {
//        return Ok(SharedTimespec{
//            pointer : mman::mmap(nix_mmap_any_address, timespec_size, mman::ProtFlags::PROT_READ, mman::MapFlags::MAP_SHARED, offset_shm_file.as_raw_fd(), 0)? as *const libc::timespec
//        });
//    }
//}
//
pub fn mmap_offset(seqnum: u32) -> Result<SharedTimespec, MmapError> {
    // Assumes stopper already prepared the file
    let offset_shm_file = OpenOptions::new().read(true).open(get_offset_shm_path(seqnum))?;
    unsafe {
        return Ok(SharedTimespec{
            pointer : mman::mmap(nix_mmap_any_address, timespec_size, mman::ProtFlags::PROT_READ, mman::MapFlags::MAP_SHARED, offset_shm_file.as_raw_fd(), 0)? as *const libc::timespec
        });
    }
}

#[derive(Debug)]
pub enum WriteTimespecError {
    TryFromError(std::num::TryFromIntError),
    WriteError(std::io::Error),
}
impl From<std::num::TryFromIntError> for WriteTimespecError {
    fn from(item: std::num::TryFromIntError) -> Self{
        Self::TryFromError(item)
    }
}
impl From<std::io::Error> for WriteTimespecError {
    fn from(item: std::io::Error) -> Self{
        Self::WriteError(item)
    }
}
//TODO: maybe we only need mem transmute?
#[derive(Copy)]
#[derive(Clone)]
#[repr(C)]
struct SerializableTimespec {
    tv_sec:  libc::time_t,
    tv_nsec: libc::c_long,
}
unsafe impl bytemuck::Pod for SerializableTimespec {} // technically this isn’t supposed to be done if there is padding, I’m not sure this is a problem, but there is no padding on x86 witha 64bit time_t
unsafe impl bytemuck::Zeroable for SerializableTimespec {
    fn zeroed() -> Self {
        Self {
            tv_sec: 0,
            tv_nsec: 0
        }
    }
}
pub fn write_timespec<T: std::io::Write>(duration: &std::time::Duration, out: &mut T) -> Result<(), WriteTimespecError>{
    let seconds : libc::time_t = duration.as_secs().try_into()?;
    let timespec = SerializableTimespec {
        tv_sec: seconds as libc::time_t,
        tv_nsec: duration.subsec_nanos() as libc::c_long,
    };
    let buf = bytemuck::bytes_of(&timespec);
    out.write_all(buf)?;
    return Ok(());
}
