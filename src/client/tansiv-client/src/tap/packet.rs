pub fn get_destination_ipv4(packet: &[u8]) -> Result<libc::in_addr_t, &'static str> {
    // Assumes Ethernet II frames and IPv4
    if packet.len()<14+20 {
        Err("malformed packet")
    } else if packet[12]!=0x08 || packet[13]!=0x00 { // EtherType for IPv4
        Err("Not an IPv4 packet or not an Ethernet II frame")
    } else {
        let ipv4_bytes = &packet[14+12..14+16];
        return Ok(*bytemuck::from_bytes(ipv4_bytes));
        // If I understand correctly in_addr_t is in network byte order
    }
}
