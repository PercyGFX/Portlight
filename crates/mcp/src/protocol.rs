//! MCP messages in, MCP messages out. No input/output here, so every case
//! can be tested with a pretend Portlight.
//!
//! Speaks both MCP eras:
//! - **Modern** (2026-07-28): every request carries its protocol version in
//!   `_meta`; there is no handshake. `server/discover` describes the server.
//! - **Legacy** (2025-11-25 and earlier): the client sends `initialize` first,
//!   then plain requests.

use portlight_core::assistant::{INSTRUCTIONS, Reply, Request, tools};
use serde_json::{Value, json};

pub const MODERN_VERSIONS: &[&str] = &["2026-07-28"];
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PROTOCOL_VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CLIENT_INFO_KEY: &str = "io.modelcontextprotocol/clientInfo";
const SERVER_INFO_KEY: &str = "io.modelcontextprotocol/serverInfo";

// JSON-RPC error codes.
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INVALID_REQUEST: i64 = -32600;
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// What the server remembers about the connection. Only legacy clients
/// need any: their `initialize` sets the version and the client's name.
#[derive(Debug, Default, Clone)]
pub struct Session {
    pub legacy_version: Option<String>,
    pub client_name: Option<String>,
}

/// How a message should be handled.
pub enum Action {
    /// Send this response now.
    Respond(Value),
    /// A tool call: ask Portlight, then finish with [`tool_response`].
    CallTool { id: Value, request: Request },
    /// A notification or a reply to us: nothing to send.
    Ignore,
}

fn server_info() -> Value {
    json!({ "name": "portlight", "title": "Portlight", "version": env!("CARGO_PKG_VERSION") })
}

fn error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

fn result(id: &Value, mut result: Value, modern: bool) -> Value {
    result["resultType"] = json!("complete");
    if modern {
        result["_meta"] = json!({ SERVER_INFO_KEY: server_info() });
    }
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn client_name(info: Option<&Value>) -> Option<String> {
    let info = info?;
    let name = info.get("title").or_else(|| info.get("name"))?.as_str()?;
    Some(match info.get("version").and_then(Value::as_str) {
        Some(version) => format!("{name} {version}"),
        None => name.to_string(),
    })
}

/// Decides what to do with one incoming message.
pub fn handle(message: &Value, session: &mut Session) -> Action {
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to something we sent (we send nothing), or junk.
        return Action::Ignore;
    };
    let Some(id) = message.get("id").cloned() else {
        // Notifications (initialized, cancelled, ...) need no answer.
        return Action::Ignore;
    };
    if id.is_null() {
        return Action::Respond(error(&Value::Null, INVALID_REQUEST, "The request id must not be null.", None));
    }
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    if method == "initialize" {
        let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
        let version = if LEGACY_VERSIONS.contains(&requested) { requested } else { LEGACY_VERSIONS[0] };
        session.legacy_version = Some(version.to_string());
        session.client_name = client_name(params.get("clientInfo"));
        return Action::Respond(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": server_info(),
                "instructions": INSTRUCTIONS,
            }
        }));
    }

    // Modern requests name their version in `_meta`; legacy ones rely on
    // the earlier `initialize`.
    let meta = params.get("_meta");
    let modern_version = meta.and_then(|m| m.get(PROTOCOL_VERSION_KEY)).and_then(Value::as_str);
    let modern = match (modern_version, &session.legacy_version) {
        (Some(version), _) if MODERN_VERSIONS.contains(&version) => true,
        (Some(version), _) => {
            let supported: Vec<&str> = MODERN_VERSIONS.iter().chain(LEGACY_VERSIONS).copied().collect();
            return Action::Respond(error(
                &id,
                UNSUPPORTED_PROTOCOL_VERSION,
                "Unsupported protocol version",
                Some(json!({ "supported": supported, "requested": version })),
            ));
        }
        (None, Some(_)) => false,
        (None, None) if method == "ping" => false,
        (None, None) => {
            return Action::Respond(error(
                &id,
                INVALID_PARAMS,
                "Missing _meta[\"io.modelcontextprotocol/protocolVersion\"]. Send initialize first, or use protocol version 2026-07-28.",
                None,
            ));
        }
    };
    let client = client_name(meta.and_then(|m| m.get(CLIENT_INFO_KEY)))
        .or_else(|| session.client_name.clone())
        .unwrap_or_else(|| "An AI assistant".to_string());

    match method {
        "ping" => Action::Respond(result(&id, json!({}), modern)),
        "server/discover" => Action::Respond(result(
            &id,
            json!({
                "supportedVersions": MODERN_VERSIONS,
                "capabilities": { "tools": { "listChanged": false } },
                "instructions": INSTRUCTIONS,
            }),
            true,
        )),
        "tools/list" => Action::Respond(result(&id, json!({ "tools": tools() }), modern)),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Action::Respond(error(&id, INVALID_PARAMS, "tools/call needs a tool name.", None));
            };
            if !portlight_core::assistant::is_tool(name) {
                return Action::Respond(error(&id, INVALID_PARAMS, &format!("Unknown tool: {name}"), None));
            }
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if !arguments.is_object() {
                return Action::Respond(error(&id, INVALID_PARAMS, "Tool arguments must be an object.", None));
            }
            Action::CallTool { id, request: Request { client, tool: name.to_string(), arguments } }
        }
        _ => Action::Respond(error(&id, METHOD_NOT_FOUND, &format!("Method not found: {method}"), None)),
    }
}

/// The response to a tool call, from Portlight's reply. Errors go back as
/// tool results with `isError`, so the assistant can read them and explain.
pub fn tool_response(id: &Value, reply: Reply) -> Value {
    let body = match reply {
        Reply::Ok(value) => {
            let text = serde_json::to_string_pretty(&value).unwrap_or_default();
            json!({ "content": [{ "type": "text", "text": text }], "structuredContent": value, "isError": false })
        }
        Reply::Error(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
    };
    // resultType is harmless for legacy clients, which ignore unknown fields.
    result(id, body, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn respond(message: Value, session: &mut Session) -> Value {
        match handle(&message, session) {
            Action::Respond(value) => value,
            Action::CallTool { .. } => panic!("expected a response, got a tool call"),
            Action::Ignore => panic!("expected a response, got nothing"),
        }
    }

    fn modern(method: &str, params: Value) -> Value {
        let mut params = params;
        params["_meta"] = json!({
            PROTOCOL_VERSION_KEY: "2026-07-28",
            CLIENT_INFO_KEY: { "name": "claude-ai", "title": "Claude", "version": "1.4" },
            "io.modelcontextprotocol/clientCapabilities": {}
        });
        json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params })
    }

    #[test]
    fn a_legacy_client_initializes_then_lists_tools() {
        let mut session = Session::default();
        let init = respond(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "Claude Desktop", "version": "1.2" }
            }}),
            &mut session,
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "portlight");
        assert!(init["result"]["capabilities"]["tools"].is_object());

        assert!(matches!(
            handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), &mut session),
            Action::Ignore
        ));
        let list = respond(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }), &mut session);
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), tools().len());

        match handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "list_apps" } }), &mut session) {
            Action::CallTool { id, request } => {
                assert_eq!(id, json!(3));
                assert_eq!(request.client, "Claude Desktop 1.2");
                assert_eq!(request.arguments, json!({}));
            }
            _ => panic!("expected a tool call"),
        }
    }

    #[test]
    fn an_unknown_legacy_version_gets_the_newest_legacy_one() {
        let mut session = Session::default();
        let init = respond(
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2099-01-01" } }),
            &mut session,
        );
        assert_eq!(init["result"]["protocolVersion"], LEGACY_VERSIONS[0]);
    }

    #[test]
    fn a_modern_client_needs_no_handshake() {
        let mut session = Session::default();
        let discover = respond(modern("server/discover", json!({})), &mut session);
        assert_eq!(discover["result"]["resultType"], "complete");
        assert_eq!(discover["result"]["supportedVersions"], json!(["2026-07-28"]));
        assert_eq!(discover["result"]["_meta"][SERVER_INFO_KEY]["name"], "portlight");

        let list = respond(modern("tools/list", json!({})), &mut session);
        assert_eq!(list["result"]["resultType"], "complete");

        match handle(&modern("tools/call", json!({ "name": "get_app", "arguments": { "app": "Firefox" } })), &mut session) {
            Action::CallTool { request, .. } => {
                assert_eq!(request.client, "Claude 1.4");
                assert_eq!(request.arguments["app"], "Firefox");
            }
            _ => panic!("expected a tool call"),
        }
        assert!(session.legacy_version.is_none());
    }

    #[test]
    fn bad_requests_get_clear_errors() {
        let mut session = Session::default();
        let mut wrong = modern("tools/list", json!({}));
        wrong["params"]["_meta"][PROTOCOL_VERSION_KEY] = json!("1900-01-01");
        let reply = respond(wrong, &mut session);
        assert_eq!(reply["error"]["code"], UNSUPPORTED_PROTOCOL_VERSION);
        assert_eq!(reply["error"]["data"]["requested"], "1900-01-01");
        assert!(reply["error"]["data"]["supported"].as_array().unwrap().contains(&json!("2026-07-28")));

        let no_version = respond(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }), &mut session);
        assert_eq!(no_version["error"]["code"], INVALID_PARAMS);

        let unknown_tool = respond(modern("tools/call", json!({ "name": "trust_app" })), &mut session);
        assert_eq!(unknown_tool["error"]["code"], INVALID_PARAMS);

        let unknown_method = respond(modern("resources/list", json!({})), &mut session);
        assert_eq!(unknown_method["error"]["code"], METHOD_NOT_FOUND);

        // A ping works before anything else.
        assert!(respond(json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" }), &mut session)["result"].is_object());
    }

    #[test]
    fn tool_results_carry_text_and_data_and_errors_stay_readable() {
        let ok = tool_response(&json!(5), Reply::Ok(json!({ "apps": 3 })));
        assert_eq!(ok["result"]["structuredContent"]["apps"], 3);
        assert!(ok["result"]["content"][0]["text"].as_str().unwrap().contains("\"apps\": 3"));
        assert_eq!(ok["result"]["isError"], false);

        let failed = tool_response(&json!(6), Reply::Error("Portlight isn't running.".into()));
        assert_eq!(failed["result"]["isError"], true);
        assert_eq!(failed["result"]["content"][0]["text"], "Portlight isn't running.");
    }
}
