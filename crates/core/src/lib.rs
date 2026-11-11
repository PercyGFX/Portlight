//! Portlight's core: the rules, with no operating-system or UI code.
//!
//! The pieces:
//! - [`Platform`] is what the core needs from the operating system. The
//!   `portlight-platform` crate implements it; tests use a fake.
//! - [`Tracker`] turns raw connection snapshots into apps, live connections
//!   and a stream of "opened" / "closed" events, and decides which apps are new.
//! - [`KnownApps`] remembers every app that has gone online, across restarts.
//! - The types in [`model`] are what front-ends (window, console) display.

mod known_apps;
pub mod model;
mod tracker;

pub use known_apps::{KnownApp, KnownApps, LEARNING_MS};
pub use model::{
    AppView, ConnView, Connection, EventKind, FlowEvent, NewApp, Platform, ProcessInfo, Snapshot,
};
pub use tracker::{SYSTEM_APP_ID, Tracker, Update};
