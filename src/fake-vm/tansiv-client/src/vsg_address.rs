use std::net::Ipv4Addr;

pub fn from_str(ip: &str) -> std::result::Result<libc::in_addr_t , std::net::AddrParseError> {
    use std::str::FromStr;
    let ipv4 = Ipv4Addr::from_str(ip)?;
    Ok(Into::<u32>::into(ipv4).to_be())
}
