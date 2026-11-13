//! The portable half of the app: what a command and the daemon say to each other. Nothing
//! here names a platform, a socket or a compositor.

pub mod config;
pub mod hotkeys;
pub mod media;
pub mod paths;
pub mod protocol;
pub mod transfer;
pub mod uri_list;
