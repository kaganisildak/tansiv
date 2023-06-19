pub fn get_destination_ipv4(packet: &[u8]) -> Result<libc::in_addr_t, &'static str> {
    // Assumes IPv4
    if packet.len()<20 {
        Err("malformed packet")
    } else if (packet[0]>>4)!=4 { // IPv4 protocol version
        Err("Not an IPv4 packet")
    } else {
        let ipv4_bytes = &packet[12..16];
        eprintln!("Destination IPv4 is {}", *bytemuck::from_bytes::<libc::in_addr_t>(ipv4_bytes));
        return Ok(*bytemuck::from_bytes(ipv4_bytes));
        // If I understand correctly in_addr_t is in network byte order
    }
}
