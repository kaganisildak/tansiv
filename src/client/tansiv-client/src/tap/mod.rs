//#[macro_use]
//extern crate nix;

pub mod packet;

use libc;
use bytemuck;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd; // requires rust 1.66.0 at least

ioctl_write_ptr!(tunsetiff, b'T', 202, libc::c_int); // from /usr/include/linux/if_tun.h

const DEV_NET_TUN : &str = "/dev/net/tun";
pub const PACKET_MAX_SIZE : usize = 1500+14; // Ethernet II frames, excluding things like jumbo frames

#[derive(Debug)]
pub enum TapError {
    NixError(nix::Error),
    IoError(std::io::Error),
    ErrorMessage(&'static str),
}
impl From<nix::Error> for TapError {
    fn from(item: nix::Error) -> Self {
        Self::NixError(item)
    }
}
impl From<std::io::Error> for TapError {
    fn from(item: std::io::Error) -> Self {
        Self::IoError(item)
    }
}

pub fn get_rw_tap_file(tap_interface_name: &str, nonblocking: bool) -> Result<std::fs::File, TapError> {
    let mut ifr = libc::ifreq {
        ifr_name : Default::default(),
        ifr_ifru : libc::__c_anonymous_ifr_ifru {
            ifru_flags :
                libc::IFF_TAP as libc::c_short |
                libc::IFF_NO_PI as libc::c_short // Don’t include Protocol Information,
                                                 // only raw ethernet frames
        }
    };
    if !(tap_interface_name.len()<ifr.ifr_name.len()) {
        return Err(TapError::ErrorMessage("tap_interface_name too long"));
    }
    ifr.ifr_name[..tap_interface_name.len()].copy_from_slice(
        bytemuck::try_cast_slice(tap_interface_name.as_bytes()).unwrap() // u8 to i8
    );

    let mut tun_file = OpenOptions::new().read(true).write(true).open(DEV_NET_TUN)?;

    unsafe{tunsetiff(tun_file.as_raw_fd(), &ifr as *const _ as *const i32)}?;

    if nonblocking {
        nix::fcntl::fcntl(tun_file.as_raw_fd(), nix::fcntl::FcntlArg::F_SETFL(nix::fcntl::OFlag::O_NONBLOCK))?;
    }

    Ok(tun_file)
}
