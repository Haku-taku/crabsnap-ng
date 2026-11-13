//! The platform-bound half of the app: the local channel commands use to reach the daemon.
//!
//! The daemon's side of the socket lives in the UI backend, which polls it with every other
//! platform descriptor; this crate carries the client and the address both sides agree on.

pub mod channel;

pub use channel::{Client, ClientError, address};
