#[derive(PartialEq)]
pub enum EtherType {
    Other,
    IPv4,
    ARP,
}

pub fn get_ethertype(packet: &[u8]) -> Result<EtherType, &'static str> {
    if packet.len()<14 {
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

fn copy_slice(buf: &mut [u8], start: usize, end: usize, data: &[u8]) {
    assert!(end>start);
    assert_eq!(end-start, data.len());
    assert!(end<=buf.len());

    for i in start..end {
        buf[i] = data[i-start];
    }
}

pub fn get_source_ipv4(packet: &[u8]) -> Result<libc::in_addr_t, &'static str> {
    // Assumes Ethernet II frames and IPv4
    if packet.len()<14 {
        Err("malformed packet")
    } else if packet.len()<14+20 || packet[12]!=0x08 || packet[13]!=0x00 { // EtherType for IPv4
        Err("Not an IPv4 packet or not an Ethernet II frame")
    } else {
        let ipv4_bytes = &packet[14+12..14+16];
        let mut bytes_buf : [u8; 4] = Default::default(); // needed for alignment
        copy_slice(&mut bytes_buf, 0, 4, ipv4_bytes);
        return Ok(*bytemuck::from_bytes(&bytes_buf));
        // If I understand correctly in_addr_t is in network byte order
    }
}
pub fn get_destination_ipv4(packet: &[u8]) -> Result<libc::in_addr_t, &'static str> {
    // Assumes Ethernet II frames and IPv4
    if packet.len()<14 {
        Err("malformed packet")
    } else if packet.len()<14+20 || packet[12]!=0x08 || packet[13]!=0x00 { // EtherType for IPv4
        Err("Not an IPv4 packet or not an Ethernet II frame")
    } else {
        let ipv4_bytes = &packet[14+16..14+20];
        let mut bytes_buf : [u8; 4] = Default::default(); // needed for alignment
        copy_slice(&mut bytes_buf, 0, 4, ipv4_bytes);
        return Ok(*bytemuck::from_bytes(&bytes_buf));
        // If I understand correctly in_addr_t is in network byte order
    }
}

fn get_unique_mac_from_ip(ipv4: libc::in_addr_t) -> [u8; 6] {
    let mut buf = [0u8; 6]; // TODO: docker seems to do this for macvlan, but mac addresses start with 02:42
    copy_slice(&mut buf, 2, 6, bytemuck::bytes_of(&ipv4));
    return buf;
}

pub fn broadcast_dest_mac(packet: &mut [u8]) -> Result<(), &'static str> {
    if packet.len()<14 {
        return Err("malformed packet");
    }
    copy_slice(packet, 0, 6, &[0xffu8; 6]);
    Ok(())
}

pub fn determinize_macs(packet: &mut [u8]) -> Result<(), &'static str> {
    if get_ethertype(packet)?!=EtherType::IPv4 || packet.len()<14+20 || packet[14]>>4!=4 {
        return Err("This function expects a valid IPv4 packet");
    }
    let destination = get_destination_ipv4(packet).unwrap();
    let source = get_source_ipv4(packet).unwrap();
    copy_slice(packet, 0, 6, &get_unique_mac_from_ip(destination));
    copy_slice(packet, 6, 12, &get_unique_mac_from_ip(source));
    Ok(())
}

pub fn spoof_arp_response(packet: &[u8]) -> Result<[u8; 14+28], &'static str> {
    if packet.len()!=14+28 {
        return Err("Unsupported ARP packet length");
    }
    let mac_ethertype = [0x08u8, 0x06u8];
    if packet[12..14]!=mac_ethertype {
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
    copy_slice(&mut target_proto, 0, 4, &packet[14+24..14+28]);

    if
        htype != [0u8, 1u8] ||
        ptype != [0x08u8, 0x00u8] ||
        hlen != 6 || plen != 4
    {
        return Err("Unsupported ARP packet type");
    }
    if oper != [0u8, 1u8] {
        eprintln!("{:?}", packet);
        return Err("Not an ARP request");
    }

    let target_proto_cast : libc::in_addr_t = *bytemuck::from_bytes(&target_proto);
    let target_mac = get_unique_mac_from_ip(target_proto_cast);

    let mut response = [0u8; 14+28];
    copy_slice(&mut response, 0, 6, sender_hard); // lower-layer destination address
    copy_slice(&mut response, 6, 12, &target_mac); // lower-layer source address
    copy_slice(&mut response, 12, 14, &mac_ethertype);
    copy_slice(&mut response, 14, 14+2, htype);
    copy_slice(&mut response, 14+2, 14+4, ptype);
    response[14+4] = hlen;
    response[14+5] = plen;
    response[14+7] = 2; // operation: reply
    copy_slice(&mut response, 14+8, 14+14, &target_mac); // sender hardware address
    copy_slice(&mut response, 14+14, 14+18, &target_proto); // sender protocol address
    copy_slice(&mut response, 14+18, 14+24, sender_hard); // target hardware address
    copy_slice(&mut response, 14+24, 14+28, sender_proto); // target protocol address
    Ok(response)
}
