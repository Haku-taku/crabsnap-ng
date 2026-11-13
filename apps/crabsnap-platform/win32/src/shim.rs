//! The ffmpeg library on Windows: no loader yet, so every question answers "absent".
//!
//! The shim stays C, and its Windows loader is `LoadLibraryExW` with `AddDllDirectory`; until
//! that exists a Windows build encodes through the ffmpeg program, or writes PNG only. The
//! answers below are the same ones the Unix shim's stub gives when no library loads.

use std::path::Path;

pub use crabsnap_core::ffmpeg::{Colour, Decoded, Encoder, EncoderOption, Muxer, Picture};

/// Nothing was loaded, so there is nothing to forget.
pub fn reopen(_directory: Option<&Path>) {}

/// No loader in this build, whichever directory the settings name.
pub fn open() -> Result<String, String> {
    Err("this build has no ffmpeg library loader".into())
}

pub fn muxers() -> Vec<Muxer> {
    Vec::new()
}

pub fn encoders() -> Vec<Encoder> {
    Vec::new()
}

pub fn options(_encoder: &str) -> Option<Vec<EncoderOption>> {
    None
}

pub fn has_option(_encoder: &str, _name: &str) -> Option<bool> {
    None
}

pub fn pix_fmts(_encoder: &str) -> Option<Vec<i32>> {
    None
}

pub fn guess_codec(_extension: &str) -> i32 {
    0
}

pub fn pix_fmt(_name: &str) -> i32 {
    -1
}

pub fn codec_name(_codec: i32) -> Option<String> {
    None
}

pub fn pix_fmt_name(_format: i32) -> Option<String> {
    None
}

/// Only asked after [`open`] succeeded, which it never does here; zeroes say "untagged".
pub fn srgb() -> Colour {
    Colour::default()
}

pub fn encode(_encoder: &str, _extension: &str, _picture: &Picture<'_>) -> Result<Vec<u8>, String> {
    Err("this build has no ffmpeg library to encode with".into())
}

pub fn decode(_bytes: &[u8]) -> Result<Decoded, String> {
    Err("this build has no ffmpeg library to decode with".into())
}
