//! The platform-bound half of the app: the local channel commands use to reach the daemon.
//!
//! The daemon serves it through the UI backend, which polls the channel with every other
//! platform source; this crate carries the client and the address both sides agree on. Which
//! transport carries it is the target tables' decision, and the re-exports below are the only
//! place that choice appears.

// The ffmpeg shim loads the libraries itself and has a loader per platform inside, so the
// one crate serves both targets and needs no row in the tables below.
pub use crabsnap_platform_ffmpeg as shim;

#[cfg(unix)]
pub use crabsnap_platform_unix::{
    CAPTURE_BACKENDS, Client, ClientError, address, alert, app_user_model_id, attach_terminal,
    autostart, capture, paths, set_app_user_model_id, system_languages,
};

#[cfg(windows)]
pub use crabsnap_platform_win32::{
    CAPTURE_BACKENDS, Client, ClientError, address, alert, app_user_model_id, attach_terminal,
    autostart, capture, paths, set_app_user_model_id, system_languages,
};

#[cfg(not(any(unix, windows)))]
compile_error!("crabsnap-platform has no local channel for this target");
