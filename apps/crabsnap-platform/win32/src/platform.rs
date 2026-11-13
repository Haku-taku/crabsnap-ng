//! What the app asks of a Windows session: the daemon channel, the user's directories and
//! clock, and the names of the capture backends this build can be told to use.

pub mod channel;
pub mod paths;

pub use channel::{Client, ClientError, address};

/// The capture backends a Windows build offers, `auto` first.
///
/// One rung so far: Desktop Duplication. Windows Graphics Capture joins this list when the
/// app can choose between them.
pub const CAPTURE_BACKENDS: &[&str] = &["auto"];
