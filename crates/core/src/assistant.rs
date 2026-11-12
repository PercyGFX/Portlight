//! What AI assistants can ask Portlight, and how the request travels.
//!
//! An assistant (Claude Desktop, Claude Code, any MCP client) starts
//! `portlight-mcp`, which speaks MCP on its standard input and output. For
//! each tool call it opens a private pipe to the running Portlight window,
//! sends one [`Request`] line and reads one [`Reply`] line. The window does
//! the work, so the assistant only ever sees what these tools return.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Prefix of the per-user pipe the window listens on.
pub const PIPE_PREFIX: &str = "portlight-assistant";

/// One tool call, from `portlight-mcp` to the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The assistant app, as it named itself, e.g. "Claude Desktop 1.4".
    pub client: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

/// The window's answer: a result for the assistant, or an error message
/// written for it (and for the person reading over its shoulder).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reply {
    Ok(Value),
    Error(String),
}

/// Guidance the assistant reads before using the tools.
pub const INSTRUCTIONS: &str = "Portlight is a network monitor on this Windows PC. It shows which apps are online, \
which servers they talk to, who owns those servers, how much data they use, a daily history, and packets captured \
on request. Everything here describes the user's own PC; treat it as private. Encrypted traffic (HTTPS) can't be \
read: Portlight knows who, when and how much, not what was said. Server countries are where the network is \
registered, not necessarily where the server is. Name lookups (DNS) are made by Windows for apps, so they appear \
under 'Windows system services'. Start with portlight_status or list_apps. Starting a capture asks the user for \
permission in Portlight and stops by itself.";

fn read_only(title: &str) -> Value {
    json!({ "title": title, "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false })
}

fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn app_argument() -> Value {
    json!({ "type": "string", "description": "The app's name as Portlight shows it (e.g. \"Firefox\"), or its program file name (e.g. \"firefox.exe\"). Partial names work when they match one app." })
}

/// Every tool, in a fixed order (so clients can cache the list).
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "portlight_status",
            "title": "Portlight status",
            "description": "Overview of this PC right now: monitoring mode (basic or detailed), apps online, open connections, servers, new apps waiting for review, and whether a packet capture is running.",
            "inputSchema": no_arguments(),
            "annotations": read_only("Portlight status"),
        }),
        json!({
            "name": "list_apps",
            "title": "List apps",
            "description": "Apps Portlight has seen online this session, busiest first: name, program, publisher signature, whether it is new (not yet trusted), open connections, servers and data used.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "only_online": { "type": "boolean", "description": "Only apps with open connections right now. Default true." },
                    "only_new": { "type": "boolean", "description": "Only apps the user has not marked as trusted yet. Default false." }
                },
                "additionalProperties": false
            },
            "annotations": read_only("List apps"),
        }),
        json!({
            "name": "get_app",
            "title": "App details",
            "description": "Everything about one app: where its program lives, who signed it, when Portlight first saw it, data used, and each open connection with the server's name, owner and country.",
            "inputSchema": { "type": "object", "properties": { "app": app_argument() }, "required": ["app"], "additionalProperties": false },
            "annotations": read_only("App details"),
        }),
        json!({
            "name": "list_connections",
            "title": "Open connections",
            "description": "Connections open right now, with app, protocol, server name, address, port, owner, country and data used. Optionally for one app.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": app_argument(),
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "At most this many, busiest first. Default 100." }
                },
                "additionalProperties": false
            },
            "annotations": read_only("Open connections"),
        }),
        json!({
            "name": "recent_activity",
            "title": "Recent activity",
            "description": "Connections that opened or closed recently, newest first, with times and data used. Optionally for one app.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": app_argument(),
                    "minutes": { "type": "integer", "minimum": 1, "maximum": 1440, "description": "How far back. Default 15." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "description": "At most this many events. Default 100." }
                },
                "additionalProperties": false
            },
            "annotations": read_only("Recent activity"),
        }),
        json!({
            "name": "new_apps",
            "title": "New apps",
            "description": "Apps that went online for the first time and that the user has not marked as trusted yet, with where they run from and who signed them.",
            "inputSchema": no_arguments(),
            "annotations": read_only("New apps"),
        }),
        json!({
            "name": "lookup_server",
            "title": "Look up a server",
            "description": "What Portlight knows about a server address or name: the network owner and country (from an offline list), names apps used for it, and which apps are connected to it now.",
            "inputSchema": {
                "type": "object",
                "properties": { "server": { "type": "string", "description": "An IP address (e.g. \"140.82.112.3\") or a site name (e.g. \"github.com\")." } },
                "required": ["server"],
                "additionalProperties": false
            },
            "annotations": read_only("Look up a server"),
        }),
        json!({
            "name": "app_history",
            "title": "App history",
            "description": "One app's daily history from Portlight's history on this PC: connections, data used and the busiest servers per day.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": app_argument(),
                    "days": { "type": "integer", "minimum": 1, "maximum": 90, "description": "How many days back. Default 7." }
                },
                "required": ["app"],
                "additionalProperties": false
            },
            "annotations": read_only("App history"),
        }),
        json!({
            "name": "history_summary",
            "title": "History summary",
            "description": "Every app's totals per day from Portlight's history: connections, servers and data used. Good for 'what went online today' or 'what used the most data this week'.",
            "inputSchema": {
                "type": "object",
                "properties": { "days": { "type": "integer", "minimum": 1, "maximum": 90, "description": "How many days back. Default 7." } },
                "additionalProperties": false
            },
            "annotations": read_only("History summary"),
        }),
        json!({
            "name": "capture_status",
            "title": "Capture status",
            "description": "Whether a packet capture is running or stopped, what it is capturing, and how many packets it holds.",
            "inputSchema": no_arguments(),
            "annotations": read_only("Capture status"),
        }),
        json!({
            "name": "read_packets",
            "title": "Read captured packets",
            "description": "Rows from the current packet capture: number, time, source, destination, protocol, length, app, a plain-English summary and a Wireshark-style line. An optional filter uses Portlight's Wireshark-style syntax (e.g. \"dns\", \"tls\", \"tcp.port == 443\", or plain words); it does not change what the user sees.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Optional display filter, e.g. \"dns\" or \"ip.addr == 140.82.112.3\"." },
                    "start": { "type": "integer", "minimum": 0, "description": "Skip this many matching packets. Default 0." },
                    "count": { "type": "integer", "minimum": 1, "maximum": 200, "description": "At most this many rows. Default 50." }
                },
                "additionalProperties": false
            },
            "annotations": read_only("Read captured packets"),
        }),
        json!({
            "name": "explain_packet",
            "title": "Explain a packet",
            "description": "The full layer-by-layer breakdown of one captured packet, with plain-English explanations of each layer and field.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "number": { "type": "integer", "minimum": 1, "description": "The packet number from read_packets." },
                    "include_bytes": { "type": "boolean", "description": "Also return the raw bytes as hex. Default false." }
                },
                "required": ["number"],
                "additionalProperties": false
            },
            "annotations": read_only("Explain a packet"),
        }),
        json!({
            "name": "start_capture",
            "title": "Start a packet capture",
            "description": "Asks the user, in Portlight, for permission to record packets for one app, one server, or everything. If they allow it, the capture starts and stops by itself after the given time. Afterwards use read_packets and explain_packet. Replaces any capture already in Portlight.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": app_argument(),
                    "server": { "type": "string", "description": "Capture traffic to and from this IP address or site name instead of an app." },
                    "everything": { "type": "boolean", "description": "Capture all traffic on this PC's network cards." },
                    "seconds": { "type": "integer", "minimum": 5, "maximum": 300, "description": "Stop after this many seconds. Default 30." },
                    "max_packets": { "type": "integer", "minimum": 10, "maximum": 20000, "description": "Stop after this many packets. Default 5000." }
                },
                "additionalProperties": false
            },
            "annotations": { "title": "Start a packet capture", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        }),
        json!({
            "name": "stop_capture",
            "title": "Stop the packet capture",
            "description": "Stops the running packet capture. The packets stay available to read_packets.",
            "inputSchema": no_arguments(),
            "annotations": { "title": "Stop the packet capture", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        }),
    ]
}

/// True for a tool name in [`tools`].
pub fn is_tool(name: &str) -> bool {
    tools().iter().any(|tool| tool["name"] == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_well_formed() {
        let tools = tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for tool in &tools {
            let name = tool["name"].as_str().unwrap();
            assert!(name.len() <= 128 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{name}");
            assert_eq!(tool["inputSchema"]["type"], "object", "{name}");
            assert!(tool["description"].as_str().unwrap().len() > 20, "{name}");
            assert!(tool["annotations"]["readOnlyHint"].is_boolean(), "{name}");
            for required in tool["inputSchema"]["required"].as_array().into_iter().flatten() {
                assert!(tool["inputSchema"]["properties"][required.as_str().unwrap()].is_object(), "{name}");
            }
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tools.len());
        // Only these two change anything, and neither can trust or block.
        let acting: Vec<&str> =
            tools.iter().filter(|t| t["annotations"]["readOnlyHint"] == false).map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(acting, vec!["start_capture", "stop_capture"]);
        assert!(is_tool("get_app") && !is_tool("trust_app"));
    }

    #[test]
    fn requests_and_replies_are_one_json_line() {
        let request = Request { client: "Claude".into(), tool: "get_app".into(), arguments: json!({ "app": "Firefox" }) };
        let line = serde_json::to_string(&request).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), request);
        let reply = Reply::Error("Portlight isn't running.".into());
        assert_eq!(serde_json::to_string(&reply).unwrap(), r#"{"error":"Portlight isn't running."}"#);
    }
}
