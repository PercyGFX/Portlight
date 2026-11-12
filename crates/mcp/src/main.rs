//! portlight-mcp: an MCP server that lets AI assistants ask a running
//! Portlight what is happening on this PC.
//!
//! The assistant starts this program and talks MCP on its standard input and
//! output: one JSON message per line. Every tool call is passed to the
//! Portlight window over a private pipe that only this Windows user can
//! open; the window decides what to share and asks the person before
//! anything changes. This program never goes on the network.
//!
//! Set it up in Portlight: Settings › AI assistants.

mod protocol;

use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use portlight_core::assistant::{PIPE_PREFIX, Reply, Request};
use serde_json::Value;

use crate::protocol::{Action, Session, handle, tool_response};

fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("portlight-mcp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let out = Arc::new(Mutex::new(io::stdout()));
    let mut session = Session::default();

    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                send(&out, &serde_json::json!({
                    "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {error}") }
                }));
                continue;
            }
        };
        match handle(&message, &mut session) {
            Action::Respond(response) => send(&out, &response),
            Action::Ignore => {}
            // Tool calls can wait on the person (permission prompts), so
            // each runs on its own thread and pings keep being answered.
            Action::CallTool { id, request } => {
                let out = Arc::clone(&out);
                thread::spawn(move || {
                    let reply = ask_portlight(&request);
                    send(&out, &tool_response(&id, reply));
                });
            }
        }
    }
}

/// Writes one message as one line. Standard output carries only MCP
/// messages; anything else goes to standard error.
fn send(out: &Mutex<io::Stdout>, message: &Value) {
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    let mut out = out.lock().unwrap_or_else(|p| p.into_inner());
    if out.write_all(line.as_bytes()).and_then(|_| out.flush()).is_err() {
        eprintln!("portlight-mcp: could not write to the assistant");
    }
}

/// Sends one request to the Portlight window and waits for its answer.
fn ask_portlight(request: &Request) -> Reply {
    let pipe = match portlight_platform::user_pipe::connect(PIPE_PREFIX) {
        Ok(pipe) => pipe,
        Err(_) => {
            return Reply::Error(
                "Portlight isn't running, so there's nothing to read. Ask the user to open Portlight (and to turn on \
                 Settings › AI assistants if it is off), then try again."
                    .into(),
            );
        }
    };
    let mut line = serde_json::to_string(request).unwrap_or_default();
    line.push('\n');
    if let Err(error) = (&pipe).write_all(line.as_bytes()) {
        return Reply::Error(format!("Could not reach Portlight: {error}"));
    }
    let mut answer = String::new();
    match BufReader::new(&pipe).read_line(&mut answer) {
        Ok(0) => Reply::Error("Portlight closed the connection without answering.".into()),
        Ok(_) => serde_json::from_str(&answer)
            .unwrap_or_else(|error| Reply::Error(format!("Portlight sent something unexpected: {error}"))),
        Err(error) => Reply::Error(format!("Could not read Portlight's answer: {error}")),
    }
}
