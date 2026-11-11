//! The full breakdown of a frame for the packet detail view.

use crate::packet::{App, Headers, L4, Net, Payload};
use crate::summary::{Who, describe};
use crate::text::{byte_count, hex, lossy, mostly_text, utc_time};
use crate::{
    Context, Field, FrameInfo, Layer, LinkType, arp, dhcp, dns, http, icmp, ip, link, misc, quic, tcp, tls, udp,
};

/// Every layer of a frame, outermost first: a "frame" layer from `info`, then
/// link, network, transport and application layers. Bytes nothing decoded
/// become a final "data" layer.
pub fn dissect(link_type: LinkType, frame: &[u8], info: &FrameInfo, ctx: &dyn Context) -> Vec<Layer> {
    let h = Headers::parse(link_type, frame);
    let who = Who::new(&h, ctx);
    let mut layers = Vec::new();

    if let Some(layer) = link::layer(frame, &h.link) {
        layers.push(layer);
    }
    // Where the bytes nothing has decoded yet start.
    let mut undecoded = if matches!(h.link.kind, link::Kind::Unreadable) { 0 } else { h.link.net_at.min(frame.len()) };
    // Where the network packet ends; anything after it is link-layer padding.
    let mut packet_end = frame.len();

    match &h.net {
        Net::Arp(a) => {
            let layer = arp::layer(frame, a);
            packet_end = (layer.start + layer.length).min(frame.len());
            undecoded = packet_end;
            layers.push(layer);
        }
        Net::Ip(ipv) => {
            let mut layer = ip::layer(frame, ipv);
            packet_end = ipv.end;
            undecoded = layer.start + layer.length;
            if !ipv.first_fragment() {
                layer.push(Field::note("Fragment", ip::fragment_info(ipv)).explain(
                    "This is a later piece of a packet that was split up on the way. The receiver puts the pieces back together.",
                ));
            }
            layers.push(layer);
            transport_layers(&h, frame, &mut layers, &mut undecoded);
        }
        Net::None => {}
    }
    if undecoded < packet_end {
        layers.push(data_layer(frame, undecoded, packet_end));
    }
    if let Some(eth) = layers.iter_mut().find(|l| l.id == "eth")
        && let Some(pad) = link::padding_field(frame, packet_end)
    {
        eth.push(pad);
    }

    let desc = describe(&h, frame, &who);
    let protocols: Vec<&str> = layers.iter().map(|l| l.id.as_str()).collect();
    let mut all = vec![frame_layer(frame, info, &desc.summary, &protocols.join(":"))];
    all.extend(layers);
    for layer in &mut all {
        layer.clamp(frame.len());
    }
    all
}

fn transport_layers(h: &Headers, frame: &[u8], layers: &mut Vec<Layer>, undecoded: &mut usize) {
    match &h.l4 {
        L4::Tcp(t) => {
            layers.push(tcp::layer(frame, t));
            *undecoded = t.payload_at;
        }
        L4::Udp(u) => {
            layers.push(udp::layer(frame, u));
            *undecoded = u.payload_at;
        }
        L4::Icmp(i) => {
            layers.push(icmp::layer(frame, i));
            *undecoded = i.end;
        }
        L4::Igmp { at, end } => {
            layers.push(icmp::igmp_layer(frame, *at, *end));
            *undecoded = *end;
        }
        L4::None => return,
    }
    let Some(p) = h.payload_info() else { return };
    if p.len() == 0 {
        return;
    }
    if let Some(layer) = app_layer(h.app, frame, &p) {
        *undecoded = layer.start + layer.length;
        layers.push(layer);
    }
}

fn app_layer(app: App, frame: &[u8], p: &Payload) -> Option<Layer> {
    match app {
        App::None => None,
        App::Dns(kind) => dns::layer(frame, p, kind),
        App::Tls => Some(tls::layer(frame, p)),
        App::Http => Some(http::layer(frame, p)),
        App::Quic => quic::layer(frame, p),
        App::Dhcp => dhcp::layer(frame, p),
        App::Ntp => misc::ntp_layer(frame, p),
        App::Ssdp => Some(misc::ssdp_layer(frame, p)),
        App::Nbns | App::Nbds => Some(misc::netbios_layer(app, p)),
    }
}

fn data_layer(frame: &[u8], start: usize, end: usize) -> Layer {
    let len = end - start;
    let mut layer = Layer::new(
        "data",
        "Data",
        byte_count(len),
        "Bytes Portlight does not decode. They belong to the program that sent them, and are often encrypted.",
        start,
        end,
    );
    layer.push(Field::new("Length", byte_count(len), start, len));
    let bytes = crate::bytes::range(frame, start, end);
    let preview = &bytes[..bytes.len().min(48)];
    if mostly_text(preview) {
        layer.push(
            Field::new("Text", lossy(preview, 48), start, preview.len())
                .explain("The start of the data, shown as text."),
        );
    } else if !preview.is_empty() {
        layer.push(Field::new("Bytes", hex(preview, 24), start, preview.len().min(24)));
    }
    layer
}

fn frame_layer(frame: &[u8], info: &FrameInfo, summary: &str, protocols: &str) -> Layer {
    let wire = if info.original_len == 0 { frame.len() } else { info.original_len as usize };
    let mut layer = Layer::new(
        "frame",
        &format!("Frame {}", info.number),
        summary.to_string(),
        "Facts about this packet as it was captured: when it arrived, how big it was and which network card it came through.",
        0,
        frame.len(),
    );
    layer.push(Field::note("Arrival time", utc_time(info.timestamp_ms)));
    let since =
        if info.time.is_finite() { format!("{:.6} seconds", info.time.max(0.0)) } else { "unknown".to_string() };
    layer.push(Field::note("Time since the capture started", since));
    layer.push(Field::note("Frame number", info.number.to_string()));
    layer.push(Field::note("Length on the wire", byte_count(wire)));
    let mut captured = Field::note("Captured length", byte_count(frame.len()));
    if frame.len() < wire {
        captured = captured.explain("Only the first part of this packet was kept, so later layers may be cut short.");
    }
    layer.push(captured);
    layer.push(Field::note("Interface", info.interface_name));
    if let Some(app) = info.app {
        layer.push(Field::note("App", app).explain("The program on this PC that sent or received this packet."));
    }
    if !protocols.is_empty() {
        layer.push(
            Field::note("Protocols in frame", protocols).explain(
                "The layers inside this packet, outermost first: each one is carried inside the one before it.",
            ),
        );
    }
    layer
}
