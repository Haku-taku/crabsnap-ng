//! The records an ffmpeg library answers with, and the picture handed to it.
//!
//! This is the data the app reads and writes; loading a library and the calls into it are a
//! platform answer, and a build whose platform has none says so where it answers.

/// One muxer as libavformat describes it, in its own words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Muxer {
    name: String,
    long_name: String,
    extensions: String,
    mime: String,
}

impl Muxer {
    pub fn new(name: String, long_name: String, extensions: String, mime: String) -> Self {
        Self {
            name,
            long_name,
            extensions,
            mime,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn long_name(&self) -> &str {
        &self.long_name
    }

    /// The extensions the muxer writes, comma-separated, as libavformat states them.
    pub fn extensions(&self) -> &str {
        &self.extensions
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }
}

/// One encoder as libavcodec describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoder {
    name: String,
    long_name: String,
    id: i32,
}

impl Encoder {
    pub fn new(name: String, long_name: String, id: i32) -> Self {
        Self {
            name,
            long_name,
            id,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn long_name(&self) -> &str {
        &self.long_name
    }

    pub fn id(&self) -> i32 {
        self.id
    }
}

/// One option an encoder takes, with the help and default the library gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncoderOption {
    name: String,
    help: String,
    default_text: String,
}

impl EncoderOption {
    pub fn new(name: String, help: String, default_text: String) -> Self {
        Self {
            name,
            help,
            default_text,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn help(&self) -> &str {
        &self.help
    }

    pub fn default_text(&self) -> &str {
        &self.default_text
    }
}

/// The colour tags a picture carries: libavutil's own enum values for each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Colour {
    pub primaries: i32,
    pub transfer: i32,
    pub space: i32,
    pub range: i32,
}

/// One picture, encoded and muxed into `extension`'s container.
pub struct Picture<'a> {
    pub width: i32,
    pub height: i32,
    /// Top-down RGBA with no padding between rows.
    pub rgba: &'a [u8],
    /// The pixel format to encode in, as a name ("bgra"); `None` lets the library choose.
    pub pixel_format: Option<&'a str>,
    pub colour: Colour,
    /// An old-style qscale for encoders that only take that; `None` for the encoder default.
    pub global_quality: Option<i32>,
    pub options: &'a [(&'a str, &'a str)],
}

/// One picture read out of a file, or out of bytes already in memory.
pub struct Decoded {
    pub width: i32,
    pub height: i32,
    /// `true` when the pixels are 2-10-10-10 words rather than 8-bit RGBA.
    pub ten_bit: bool,
    pub colour: Colour,
    pub data: Vec<u8>,
}
