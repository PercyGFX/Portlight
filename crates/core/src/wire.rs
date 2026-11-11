//! What the Portlight Collector service sends the window: one JSON message
//! per line.
//!
//! The first message is a [`Message::Hello`] with every site name the
//! collector knows; after that a [`Message::Reading`] arrives twice a second.

use std::io;
use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use crate::model::Observation;

/// Goes up when the messages change in a way older windows cannot read.
pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Message {
    Hello { version: String, protocol: u32, names: Vec<(IpAddr, String)> },
    Reading { observation: Observation },
}

impl Message {
    /// One line of JSON, ending in a newline.
    pub fn encode(&self) -> String {
        let mut line = serde_json::to_string(self).unwrap_or_default();
        line.push('\n');
        line
    }

    pub fn decode(line: &str) -> io::Result<Message> {
        serde_json::from_str(line.trim()).map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::*;
    use crate::model::{Connection, Observed, ProcessRecord, Protocol, Source};

    #[test]
    fn messages_survive_the_trip() {
        let conn = Connection {
            pid: 7,
            protocol: Protocol::Udp,
            local: SocketAddr::from(([192, 168, 1, 10], 0)),
            remote: "[2606:50c0::1]:443".parse().unwrap(),
        };
        let reading = Message::Reading {
            observation: Observation {
                source: Source::Detailed,
                open: vec![Observed { conn, sent: 1, received: 2 }],
                closed: vec![],
                names: vec![("140.82.112.3".parse().unwrap(), "github.com".into())],
                processes: vec![ProcessRecord { pid: 7, path: "C:\\a.exe".into(), name: None }],
            },
        };
        let hello = Message::Hello { version: "0.3.0".into(), protocol: PROTOCOL, names: vec![] };

        for message in [reading, hello] {
            let line = message.encode();
            assert!(line.ends_with('\n') && !line.trim_end().contains('\n'));
            assert_eq!(Message::decode(&line).unwrap(), message);
        }
    }
}
