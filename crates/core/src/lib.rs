//! Portlight's core: the rules, with no operating-system or UI code.
//!
//! The pieces:
//! - [`Platform`] is what the core needs from the operating system. The
//!   `portlight-platform` crate implements it; tests use a fake.
//! - [`Tracker`] turns readings ([`Observation`]) into apps, live connections
//!   and a stream of "opened" / "closed" events, and decides which apps are new.
//! - [`FlowTable`] turns low-level network events into readings. The
//!   Portlight Collector service uses it.
//! - [`wire`] is what the collector sends the window.
//! - [`KnownApps`] remembers every app that has gone online, across restarts.
//! - The types in [`model`] are what front-ends (window, console) display.

mod flows;
mod known_apps;
pub mod model;
mod tracker;
pub mod wire;

pub use flows::{FlowTable, NetEvent, NetEventKind, UDP_IDLE_MS, parse_dns_results};
pub use known_apps::{KnownApp, KnownApps, LEARNING_MS};
pub use model::{
    AppView, ConnView, Connection, EventKind, FlowEvent, NewApp, Observation, Observed, Platform, ProcessInfo,
    ProcessRecord, Protocol, Snapshot, Source,
};
pub use tracker::{SYSTEM_APP_ID, Tracker, Update, describe_process, leaves_this_machine};
