//! What the app asks of a Unix session: the daemon channel, the user's directories and clock,
//! and the names of the capture backends this build can be told to use.
//!
//! The crate is empty off unix: a socket in the runtime directory, a Wayland grab and a C shim
//! cannot exist without those calls, so a Windows workspace builds it empty.

#![cfg(unix)]

pub mod capture;
pub mod channel;
pub mod colour;
pub mod paths;
pub mod shim;

pub use channel::{Client, ClientError, address};

/// The capture backends a Unix build offers, `auto` first.
///
/// Each is a Wayland rung: `ext` and `screencopy` copy through a protocol, `portal` asks the
/// desktop over D-Bus.
pub const CAPTURE_BACKENDS: &[&str] = &["auto", "ext", "screencopy", "portal"];
