//! What the app asks of a Unix session: the daemon channel, the user's directories and clock,
//! and the names of the capture backends this build can be told to use.
//!
//! The crate is empty off unix: a socket in the runtime directory and a Wayland grab cannot
//! exist without those calls, so a Windows workspace builds it empty.

#![cfg(unix)]

pub mod capture;
pub mod channel;
pub mod colour;
pub mod paths;

pub use channel::{Client, ClientError, address};

/// The capture backends a Unix build offers, `auto` first.
///
/// Each is a Wayland rung: `ext` and `screencopy` copy through a protocol, `portal` asks the
/// desktop over D-Bus.
pub const CAPTURE_BACKENDS: &[&str] = &["auto", "ext", "screencopy", "portal"];

/// 说一句终端之外也得让人看见的话：桌面启动的进程没有终端，这句话只能进日志。
pub fn alert(message: &str) {
    eprintln!("{message}");
}
