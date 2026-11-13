//! What the app asks of a Windows session: the daemon channel, the user's directories and
//! clock, and the names of the capture backends this build can be told to use.

pub mod capture;
pub mod channel;
pub mod paths;

pub use channel::{Client, ClientError, address};

/// The capture backends a Windows build offers, `auto` first.
///
/// A Windows Graphics Capture session holds nothing exclusively and answers on a still
/// desktop, so it leads; Desktop Duplication carries a float frame's highlights; a GDI blit
/// always answers but is 8-bit.
pub const CAPTURE_BACKENDS: &[&str] = &["auto", "wgc", "duplication", "gdi"];
