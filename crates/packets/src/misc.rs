//! Small protocols read only a little: NTP (time), SSDP (device discovery), NetBIOS.

use crate::bytes::{u8_at, u32_at};
use crate::http;
use crate::packet::{App, Payload};
use crate::summary::{Desc, Who};
use crate::text::{byte_count, shorten, utc_time};
use crate::{Field, Layer};

// NTP

fn ntp_mode(mode: u8) -> &'static str {
    match mode {
        1 => "symmetric active",
        2 => "symmetric passive",
        3 => "client",
        4 => "server",
        5 => "broadcast",
        6 => "control",
        7 => "private",
        _ => "reserved",
    }
}

pub(crate) fn ntp_describe(frame: &[u8], p: &Payload, who: &Who) -> Option<Desc> {
    let b0 = u8_at(frame, p.at)?;
    let (version, mode) = ((b0 >> 3) & 7, b0 & 7);
    let (src, dst) = (&who.src, &who.dst);
    let summary = match mode {
        3 => format!("{src} asks {dst} for the current time"),
        4 => format!("{src} tells {dst} the current time"),
        5 => format!("{src} announces the current time to the network"),
        _ => format!("{src} sends a time message to {dst}"),
    };
    Some(Desc::new("NTP", format!("NTP Version {version}, {}", ntp_mode(mode)), summary))
}

/// NTP time (seconds since 1900 plus a binary fraction) as text.
fn ntp_time(frame: &[u8], at: usize) -> Option<String> {
    let secs = u32_at(frame, at)?;
    let frac = u32_at(frame, at + 4)?;
    if secs == 0 && frac == 0 {
        return Some("not set".to_string());
    }
    let unix = f64::from(secs) - 2_208_988_800.0 + f64::from(frac) / 4_294_967_296.0;
    Some(utc_time(unix * 1000.0))
}

pub(crate) fn ntp_layer(frame: &[u8], p: &Payload) -> Option<Layer> {
    let at = p.at;
    let b0 = u8_at(frame, at)?;
    let (leap, version, mode) = (b0 >> 6, (b0 >> 3) & 7, b0 & 7);
    let mut layer = Layer::new(
        "ntp",
        "Network Time Protocol (NTP)",
        format!("NTP version {version}, {}", ntp_mode(mode)),
        "NTP keeps the PC's clock right by asking a time server for the current time.",
        p.at,
        p.end,
    );
    let leap_text = match leap {
        0 => "0 (no warning)",
        1 => "1 (last minute of the day has 61 seconds)",
        2 => "2 (last minute of the day has 59 seconds)",
        _ => "3 (clock not synchronized)",
    };
    layer.push(Field::new("Leap indicator", leap_text, at, 1));
    layer.push(Field::new("Version", version.to_string(), at, 1));
    layer.push(
        Field::new("Mode", format!("{mode} ({})", ntp_mode(mode)), at, 1)
            .explain("3 = a question from a client, 4 = the server's answer."),
    );
    if let Some(stratum) = u8_at(frame, at + 1) {
        layer.push(Field::new("Stratum", stratum.to_string(), at + 1, 1).explain(
            "How far the server is from a reference clock: 1 = directly connected to one, 2 = one step away, and so on.",
        ));
    }
    if let Some(t) = ntp_time(frame, at + 40) {
        layer.push(
            Field::new("Transmit time", t, at + 40, 8)
                .explain("The time on the sender's clock when it sent this packet."),
        );
    }
    Some(layer)
}

// SSDP

pub(crate) fn ssdp_describe(frame: &[u8], p: &Payload, who: &Who) -> Desc {
    let (lines, _) = http::lines(frame, p.at, p.end, 1);
    let first = lines.first().map_or(String::new(), |l| l.text.clone());
    let src = &who.src;
    let summary = if first.starts_with("M-SEARCH") {
        format!("{src} looks for devices on the local network (TVs, speakers, printers, routers…)")
    } else if first.starts_with("NOTIFY") {
        format!("{src} announces itself to devices on the local network")
    } else if first.starts_with("HTTP/") {
        format!("{src} answers a device search")
    } else {
        format!("{src} sends a device discovery message")
    };
    Desc::new("SSDP", shorten(&first, 200), summary)
}

pub(crate) fn ssdp_layer(frame: &[u8], p: &Payload) -> Layer {
    let (lines, _) = http::lines(frame, p.at, p.end, 32);
    let first = lines.first().map_or(String::new(), |l| shorten(&l.text, 200));
    let mut layer = Layer::new(
        "ssdp",
        "Simple Service Discovery Protocol (SSDP)",
        first,
        "SSDP lets devices on the local network (TVs, speakers, printers, routers) find each other. It is part of \
         Universal Plug and Play (UPnP).",
        p.at,
        p.end,
    );
    for (i, line) in lines.iter().enumerate() {
        let field = if i == 0 {
            Field::new("Request line", shorten(&line.text, 200), line.at, line.len)
        } else {
            let (name, value) = line.text.split_once(':').unwrap_or((line.text.as_str(), ""));
            let mut f = Field::new(name.trim(), shorten(value.trim(), 200), line.at, line.len);
            match name.trim().to_ascii_uppercase().as_str() {
                "ST" | "NT" => f = f.explain("The kind of device or service being looked for or announced."),
                "LOCATION" => f = f.explain("Where to fetch the device's description."),
                "USN" => f = f.explain("A unique name for this device."),
                _ => {}
            }
            f
        };
        layer.push(field);
    }
    layer
}

// NetBIOS

pub(crate) fn netbios_describe(app: App, p: &Payload, who: &Who) -> Desc {
    if app == App::Nbns {
        Desc::new(
            "NBNS",
            "NetBIOS name service".to_string(),
            format!("{} looks up or announces a computer name the old Windows way (NetBIOS)", who.src),
        )
    } else {
        Desc::new(
            "NBDS",
            format!("NetBIOS datagram, {}", byte_count(p.len())),
            format!("{} sends a NetBIOS message (old Windows network browsing)", who.src),
        )
    }
}

pub(crate) fn netbios_layer(app: App, p: &Payload) -> Layer {
    let title = if app == App::Nbns { "NetBIOS Name Service" } else { "NetBIOS Datagram Service" };
    let mut layer = Layer::new(
        "netbios",
        title,
        byte_count(p.len()),
        "NetBIOS is an old Windows way of finding computers and shared folders on the local network by name.",
        p.at,
        p.end,
    );
    layer.push(Field::new("Data", byte_count(p.len()), p.at, p.len()));
    layer
}
