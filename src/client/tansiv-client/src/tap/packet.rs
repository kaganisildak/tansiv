// Enables basic length checking, disabling this can cause the functions
// to panic upon encountering a forged (invalid) packet.
const SOME_CHECKS : bool = true;
// More checks on validity of packets, and of correct use of the functions.
// SOME_CHECKS should be enough if the checks have been done by previous
// function calls, as intended (for example, get_ethertype checks the length
// is consistent with an Ethernet II frame, so the check can be omitted in
// other functions)
const MORE_CHECKS : bool = true;

#[non_exhaustive]
#[derive(PartialEq)]
pub enum EtherType {
    Other,
    IPv4,
    ARP,
}

pub fn get_ethertype(packet: &[u8]) -> Result<EtherType, &'static str> {
    if SOME_CHECKS && packet.len()<14 {
        return Err("malformed packet");
    }
    Ok(
        match (packet[12],packet[13]) {
            (0x08,0x00) => EtherType::IPv4,
            (0x08,0x06) => EtherType::ARP,
            _ => EtherType::Other
        }
    )
}

pub fn get_source_ipv4(packet: &[u8]) -> Result<libc::in_addr_t, &'static str> {
    // Assumes Ethernet II frames and IPv4
    if MORE_CHECKS && packet.len()<14 {
        Err("malformed packet")
    } else if MORE_CHECKS && (packet[12]!=0x08 || packet[13]!=0x00) { // EtherType for IPv4
        Err("Not an IPv4 packet or not an Ethernet II frame")
    } else if SOME_CHECKS && packet.len()<14+20 {
        Err("IPv4 packet too small")
    } else {
        let ipv4_bytes = &packet[14+12..14+16];
        let mut bytes_buf : [u8; 4] = Default::default(); // needed for alignment
        bytes_buf.copy_from_slice(ipv4_bytes);
        return Ok(*bytemuck::from_bytes(&bytes_buf));
        // If I understand correctly in_addr_t is in network byte order
    }
}
pub fn get_destination_ipv4(packet: &[u8]) -> Result<libc::in_addr_t, &'static str> {
    // Assumes Ethernet II frames and IPv4
    if MORE_CHECKS && packet.len()<14 {
        Err("malformed packet")
    } else if MORE_CHECKS && (packet[12]!=0x08 || packet[13]!=0x00) { // EtherType for IPv4
        Err("Not an IPv4 packet or not an Ethernet II frame")
    } else if SOME_CHECKS && packet.len()<14+20 {
        Err("IPv4 packet too small")
    } else {
        let ipv4_bytes = &packet[14+16..14+20];
        let mut bytes_buf : [u8; 4] = Default::default(); // needed for alignment
        bytes_buf.copy_from_slice(ipv4_bytes);
        return Ok(*bytemuck::from_bytes(&bytes_buf));
        // If I understand correctly in_addr_t is in network byte order
    }
}

fn get_unique_mac_from_ip(ipv4: libc::in_addr_t) -> [u8; 6] {
    //let mut buf = [0u8; 6]; // TODO: docker seems to do this for macvlan, but mac addresses start with 02:42
    let mut buf : [u8; 6] = [0x02, 0x42, 0, 0, 0, 0];
    buf[2..6].copy_from_slice(bytemuck::bytes_of(&ipv4));
    return buf;
}

pub fn broadcast_dest_mac(packet: &mut [u8]) -> Result<(), &'static str> {
    //if packet.len()<14 {
    //    return Err("malformed packet");
    //}
    //packet.copy_from_slice(&[0xffu8; 6]);
    // attempting NOP, should be fine if get_unique_mac_from_ip is the same
    Ok(())
}

pub fn determinize_macs(packet: &mut [u8]) -> Result<(), &'static str> {
    if MORE_CHECKS && (get_ethertype(packet)?!=EtherType::IPv4 || packet.len()<14+20 || packet[14]>>4!=4) {
        return Err("This function expects a valid IPv4 packet");
    }
    let destination = get_destination_ipv4(packet).unwrap();
    let source = get_source_ipv4(packet).unwrap();
    packet[0..6].copy_from_slice(&get_unique_mac_from_ip(destination));
    packet[6..12].copy_from_slice(&get_unique_mac_from_ip(source));
    Ok(())
}

pub fn spoof_arp_response(packet: &[u8]) -> Result<[u8; 14+28], &'static str> {
    if SOME_CHECKS && packet.len()!=14+28 {
        return Err("Unsupported ARP packet length");
    }
    let mac_ethertype = [0x08u8, 0x06u8];
    if MORE_CHECKS && packet[12..14]!=mac_ethertype {
        return Err("Not actually an ARP packet");
    }
    let htype = &packet[14..14+2];
    let ptype = &packet[14+2..14+4];
    let hlen = packet[14+4];
    let plen = packet[14+5];
    let oper = &packet[14+6..14+8];
    let sender_hard  = &packet[14+8..14+14];
    let sender_proto = &packet[14+14..14+18];
    // target_hard should be ignored in requests
    let mut target_proto : [u8; 4] = Default::default();
    target_proto.copy_from_slice(&packet[14+24..14+28]);

    if MORE_CHECKS && (
        htype != [0u8, 1u8] ||
        ptype != [0x08u8, 0x00u8] ||
        hlen != 6 || plen != 4
    ) {
        return Err("Unsupported ARP packet type");
    }
    if oper != [0u8, 1u8] {
        eprintln!("{:?}", packet);
        return Err("Not an ARP request");
    }

    let target_proto_cast : libc::in_addr_t = *bytemuck::from_bytes(&target_proto);
    let target_mac = get_unique_mac_from_ip(target_proto_cast);

    let mut response = [0u8; 14+28];
    response[..6].copy_from_slice(sender_hard); // lower-layer destination address
    response[6..12].copy_from_slice(&target_mac); // lower-layer source address
    response[12..14].copy_from_slice(&mac_ethertype);
    response[14..14+2].copy_from_slice(htype);
    response[14+2..14+4].copy_from_slice(ptype);
    response[14+4] = hlen;
    response[14+5] = plen;
    response[14+7] = 2; // operation: reply
    response[14+8..14+14].copy_from_slice(&target_mac); // sender hardware address
    response[14+14..14+18].copy_from_slice(&target_proto); // sender protocol address
    response[14+18..14+24].copy_from_slice(sender_hard); // target hardware address
    response[14+24..14+28].copy_from_slice(sender_proto); // target protocol address
    Ok(response)
}
