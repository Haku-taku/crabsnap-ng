//! The platform-bound half of the app: the local channel commands use to reach the daemon.
//!
//! The daemon serves it through the UI backend, which polls the channel with every other
//! platform source; this crate carries the client and the address both sides agree on. Which
//! transport carries it is the target tables' decision, and the re-exports below are the only
//! place that choice appears.

#[cfg(unix)]
pub use crabsnap_platform_unix::{Client, ClientError, address};

#[cfg(windows)]
pub use crabsnap_platform_win32::{Client, ClientError, address};

#[cfg(not(any(unix, windows)))]
compile_error!("crabsnap-platform has no local channel for this target");
