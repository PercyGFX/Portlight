//! Names for the numbers that appear in packets: ports, protocols, types.

/// The service usually found on a well-known port.
pub(crate) fn port(port: u16) -> Option<&'static str> {
    Some(match port {
        20 | 21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP mail",
        53 => "DNS",
        67 => "DHCP server",
        68 => "DHCP client",
        69 => "TFTP",
        80 => "HTTP",
        88 => "Kerberos",
        110 => "POP3 mail",
        123 => "NTP time",
        135 => "Windows RPC",
        137 => "NetBIOS names",
        138 => "NetBIOS datagrams",
        139 => "NetBIOS sessions",
        143 => "IMAP mail",
        161 => "SNMP",
        162 => "SNMP traps",
        389 => "LDAP",
        443 => "HTTPS",
        445 => "Windows file sharing",
        465 => "SMTP mail over TLS",
        500 => "IPsec key exchange",
        546 => "DHCPv6 client",
        547 => "DHCPv6 server",
        587 => "mail submission",
        636 => "LDAP over TLS",
        853 => "DNS over TLS",
        993 => "IMAP mail over TLS",
        995 => "POP3 mail over TLS",
        1194 => "OpenVPN",
        1433 => "SQL Server",
        1900 => "SSDP",
        3074 => "Xbox Live",
        3306 => "MySQL",
        3389 => "Remote Desktop",
        3478 => "STUN",
        3702 => "WS-Discovery",
        4500 => "IPsec NAT traversal",
        5060 => "SIP",
        5222 => "XMPP chat",
        5228 => "Google services",
        5353 => "mDNS",
        5355 => "LLMNR",
        5432 => "PostgreSQL",
        5900 => "VNC",
        6379 => "Redis",
        7680 => "Windows Update delivery",
        8080 => "HTTP alternate",
        8443 => "HTTPS alternate",
        27017 => "MongoDB",
        51820 => "WireGuard",
        _ => return None,
    })
}

/// "443 (HTTPS)" or "51234".
pub(crate) fn port_value(p: u16) -> String {
    match port(p) {
        Some(name) => format!("{p} ({name})"),
        None => p.to_string(),
    }
}

pub(crate) fn ip_proto(p: u8) -> Option<&'static str> {
    Some(match p {
        0 => "IPv6 hop-by-hop options",
        1 => "ICMP",
        2 => "IGMP",
        4 => "IPv4 in IP",
        6 => "TCP",
        17 => "UDP",
        41 => "IPv6 in IP",
        43 => "IPv6 routing header",
        44 => "IPv6 fragment header",
        47 => "GRE",
        50 => "ESP (IPsec)",
        51 => "AH (IPsec)",
        58 => "ICMPv6",
        59 => "No next header",
        60 => "IPv6 destination options",
        89 => "OSPF",
        103 => "PIM",
        112 => "VRRP",
        132 => "SCTP",
        _ => return None,
    })
}

/// "6 (TCP)".
pub(crate) fn ip_proto_value(p: u8) -> String {
    match ip_proto(p) {
        Some(name) => format!("{p} ({name})"),
        None => p.to_string(),
    }
}

/// A short name for an EtherType, also used as the protocol column.
pub(crate) fn ethertype(t: u16) -> Option<&'static str> {
    Some(match t {
        0x0800 => "IPv4",
        0x0806 => "ARP",
        0x86dd => "IPv6",
        0x8100 => "802.1Q VLAN",
        0x88a8 => "802.1ad VLAN",
        0x888e => "EAPOL",
        0x88cc => "LLDP",
        0x8863 => "PPPoE discovery",
        0x8864 => "PPPoE",
        0x893a => "IEEE 1905.1",
        0x88e1 => "HomePlug AV",
        0x8899 => "Realtek",
        0x8847 | 0x8848 => "MPLS",
        0x9000 => "Loopback",
        _ => return None,
    })
}

/// "0x0800 (IPv4)".
pub(crate) fn ethertype_value(t: u16) -> String {
    match ethertype(t) {
        Some(name) => format!("0x{t:04x} ({name})"),
        None => format!("0x{t:04x}"),
    }
}

pub(crate) fn dns_type(t: u16) -> Option<&'static str> {
    Some(match t {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        6 => "SOA",
        12 => "PTR",
        13 => "HINFO",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        33 => "SRV",
        35 => "NAPTR",
        41 => "OPT",
        43 => "DS",
        46 => "RRSIG",
        47 => "NSEC",
        48 => "DNSKEY",
        50 => "NSEC3",
        64 => "SVCB",
        65 => "HTTPS",
        255 => "ANY",
        257 => "CAA",
        _ => return None,
    })
}

/// "A", or "TYPE99" for types without a name.
pub(crate) fn dns_type_label(t: u16) -> String {
    match dns_type(t) {
        Some(name) => name.to_string(),
        None => format!("TYPE{t}"),
    }
}

/// "A (IPv4 address)".
pub(crate) fn dns_type_value(t: u16) -> String {
    let meaning = match t {
        1 => "IPv4 address",
        2 => "name server",
        5 => "another name for",
        6 => "zone information",
        12 => "name for an address",
        15 => "mail server",
        16 => "text",
        28 => "IPv6 address",
        33 => "service location",
        41 => "extended DNS options",
        64 => "service settings",
        65 => "connection settings for websites",
        255 => "everything",
        _ => return dns_type_label(t),
    };
    format!("{} ({meaning})", dns_type_label(t))
}

pub(crate) fn dns_rcode(code: u16) -> &'static str {
    match code {
        0 => "No error",
        1 => "Format error",
        2 => "Server failure",
        3 => "No such name",
        4 => "Not implemented",
        5 => "Refused",
        6 => "Name exists",
        9 => "Not authoritative",
        _ => "Unknown error",
    }
}

pub(crate) fn dns_opcode(code: u16) -> &'static str {
    match code {
        0 => "Standard query",
        1 => "Inverse query",
        2 => "Server status request",
        4 => "Zone change notification",
        5 => "Dynamic update",
        _ => "Unknown operation",
    }
}

/// "TLS 1.3", or the number in hex.
pub(crate) fn tls_version(v: u16) -> String {
    match v {
        0x0300 => "SSL 3.0".to_string(),
        0x0301 => "TLS 1.0".to_string(),
        0x0302 => "TLS 1.1".to_string(),
        0x0303 => "TLS 1.2".to_string(),
        0x0304 => "TLS 1.3".to_string(),
        v if is_grease(v) => "reserved (GREASE)".to_string(),
        v if v >> 8 == 0x7f => format!("TLS 1.3 draft {}", v & 0xff),
        v => format!("0x{v:04x}"),
    }
}

/// GREASE values are random placeholders apps send to keep servers tolerant of new values.
pub(crate) fn is_grease(v: u16) -> bool {
    v & 0x0f0f == 0x0a0a && v >> 8 == v & 0xff
}

pub(crate) fn tls_content_type(t: u8) -> &'static str {
    match t {
        20 => "Change Cipher Spec",
        21 => "Alert",
        22 => "Handshake",
        23 => "Application Data",
        24 => "Heartbeat",
        _ => "Unknown",
    }
}

pub(crate) fn tls_handshake(t: u8) -> Option<&'static str> {
    Some(match t {
        0 => "Hello Request",
        1 => "Client Hello",
        2 => "Server Hello",
        4 => "New Session Ticket",
        5 => "End of Early Data",
        8 => "Encrypted Extensions",
        11 => "Certificate",
        12 => "Server Key Exchange",
        13 => "Certificate Request",
        14 => "Server Hello Done",
        15 => "Certificate Verify",
        16 => "Client Key Exchange",
        20 => "Finished",
        24 => "Key Update",
        _ => return None,
    })
}

pub(crate) fn tls_extension(t: u16) -> String {
    let name = match t {
        0 => "server_name",
        1 => "max_fragment_length",
        5 => "status_request",
        10 => "supported_groups",
        11 => "ec_point_formats",
        13 => "signature_algorithms",
        16 => "application_layer_protocol_negotiation",
        17 => "status_request_v2",
        18 => "signed_certificate_timestamp",
        21 => "padding",
        22 => "encrypt_then_mac",
        23 => "extended_master_secret",
        27 => "compress_certificate",
        28 => "record_size_limit",
        35 => "session_ticket",
        41 => "pre_shared_key",
        42 => "early_data",
        43 => "supported_versions",
        44 => "cookie",
        45 => "psk_key_exchange_modes",
        49 => "post_handshake_auth",
        50 => "signature_algorithms_cert",
        51 => "key_share",
        57 => "quic_transport_parameters",
        17513 | 17613 => "application_settings",
        0xfe0d => "encrypted_client_hello",
        0xff01 => "renegotiation_info",
        t if is_grease(t) => "reserved (GREASE)",
        _ => return format!("unknown ({t})"),
    };
    name.to_string()
}

pub(crate) fn tls_alert(d: u8) -> &'static str {
    match d {
        0 => "Close Notify",
        10 => "Unexpected Message",
        20 => "Bad Record MAC",
        22 => "Record Overflow",
        40 => "Handshake Failure",
        42 => "Bad Certificate",
        43 => "Unsupported Certificate",
        44 => "Certificate Revoked",
        45 => "Certificate Expired",
        46 => "Certificate Unknown",
        47 => "Illegal Parameter",
        48 => "Unknown CA",
        49 => "Access Denied",
        50 => "Decode Error",
        51 => "Decrypt Error",
        70 => "Protocol Version",
        71 => "Insufficient Security",
        80 => "Internal Error",
        86 => "Inappropriate Fallback",
        90 => "User Canceled",
        100 => "No Renegotiation",
        109 => "Missing Extension",
        110 => "Unsupported Extension",
        112 => "Unrecognized Name",
        116 => "Certificate Required",
        120 => "No Application Protocol",
        _ => "Unknown",
    }
}

pub(crate) fn tls_cipher(c: u16) -> Option<&'static str> {
    Some(match c {
        0x1301 => "TLS_AES_128_GCM_SHA256",
        0x1302 => "TLS_AES_256_GCM_SHA384",
        0x1303 => "TLS_CHACHA20_POLY1305_SHA256",
        0xc02b => "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
        0xc02c => "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
        0xc02f => "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
        0xc030 => "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
        0xcca8 => "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
        0xcca9 => "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
        0xc013 => "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
        0xc014 => "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
        0xc009 => "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
        0xc00a => "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
        0x009c => "TLS_RSA_WITH_AES_128_GCM_SHA256",
        0x009d => "TLS_RSA_WITH_AES_256_GCM_SHA384",
        0x002f => "TLS_RSA_WITH_AES_128_CBC_SHA",
        0x0035 => "TLS_RSA_WITH_AES_256_CBC_SHA",
        0x000a => "TLS_RSA_WITH_3DES_EDE_CBC_SHA",
        0x00ff => "TLS_EMPTY_RENEGOTIATION_INFO_SCSV",
        _ => return None,
    })
}

/// What an ALPN protocol ID means.
pub(crate) fn alpn(id: &str) -> Option<&'static str> {
    Some(match id {
        "h2" => "HTTP/2",
        "h3" => "HTTP/3",
        "http/1.1" => "HTTP/1.1",
        "http/1.0" => "HTTP/1.0",
        "dot" => "DNS over TLS",
        "doq" => "DNS over QUIC",
        "acme-tls/1" => "certificate check",
        _ => return None,
    })
}
