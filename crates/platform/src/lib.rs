//! Operating-system data sources for Portlight.
//!
//! Each supported system gets one module that implements
//! [`portlight_core::Platform`]. Only Windows exists today; adding Linux or
//! macOS means adding a module here, with no changes to the core or the UI.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::WindowsPlatform as SystemPlatform;
#[cfg(windows)]
pub use windows::{adapters, clock, etw, icon, npcap, pipe, process, service, signature, startup, udp_ports};

#[cfg(not(windows))]
compile_error!("Portlight supports Windows only for now. See docs/PLAN.md for the roadmap.");

/// The data source for the system this program was built for.
pub fn system() -> SystemPlatform {
    SystemPlatform
}
