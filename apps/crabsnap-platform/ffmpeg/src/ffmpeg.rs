//! The C shim, called as plain C: fixed-layout records and functions that return counts.
//!
//! The buffer sizes below are the header's (`CS_NAME` and friends); both sides say the same
//! numbers and the compiler does not check that. Change them together.

use std::ffi::{CString, c_char, c_int, c_long};
use std::path::Path;

pub use crabsnap_core::ffmpeg::{Colour, Decoded, Encoder, EncoderOption, Muxer, Picture};

const NAME: usize = 32;
const TEXT: usize = 96;
const DEFAULT: usize = 72;

#[repr(C)]
#[derive(Clone, Copy)]
struct RawMuxer {
    name: [c_char; NAME],
    long_name: [c_char; TEXT],
    extensions: [c_char; TEXT],
    mime: [c_char; TEXT],
    video_codec: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawEncoder {
    name: [c_char; NAME],
    long_name: [c_char; TEXT],
    id: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RawEncoderOption {
    name: [c_char; NAME],
    help: [c_char; TEXT],
    default_text: [c_char; DEFAULT],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawColour {
    primaries: c_int,
    transfer: c_int,
    space: c_int,
    range: c_int,
}

impl RawMuxer {
    const EMPTY: Self = Self {
        name: [0; NAME],
        long_name: [0; TEXT],
        extensions: [0; TEXT],
        mime: [0; TEXT],
        video_codec: 0,
    };

    fn empty(count: usize) -> Vec<Self> {
        vec![Self::EMPTY; count]
    }
}

impl RawEncoder {
    const EMPTY: Self = Self {
        name: [0; NAME],
        long_name: [0; TEXT],
        id: 0,
    };

    fn empty(count: usize) -> Vec<Self> {
        vec![Self::EMPTY; count]
    }
}

impl RawEncoderOption {
    const EMPTY: Self = Self {
        name: [0; NAME],
        help: [0; TEXT],
        default_text: [0; DEFAULT],
    };

    fn empty(count: usize) -> Vec<Self> {
        vec![Self::EMPTY; count]
    }
}

impl RawColour {
    fn plain(self) -> Colour {
        Colour {
            primaries: self.primaries,
            transfer: self.transfer,
            space: self.space,
            range: self.range,
        }
    }

    fn of(colour: Colour) -> Self {
        Self {
            primaries: colour.primaries,
            transfer: colour.transfer,
            space: colour.space,
            range: colour.range,
        }
    }
}

/// How the shim packed a decoded picture.
pub const STORAGE_RGBA: c_int = 0;
pub const STORAGE_PACKED10: c_int = 1;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawDecoded {
    width: c_int,
    height: c_int,
    storage: c_int,
    colour: RawColour,
}

#[repr(C)]
struct RawPicture {
    width: c_int,
    height: c_int,
    rgba: *const u8,
    pitch: c_long,
    pix_fmt: c_int,
    colour: RawColour,
    global_quality: c_int,
    option_names: *const *const c_char,
    option_values: *const *const c_char,
    option_count: c_int,
}

unsafe extern "C" {
    fn cs_open() -> c_int;
    fn cs_versions() -> *const c_char;
    fn cs_error() -> *const c_char;
    fn cs_reopen(directory: *const c_char);
    fn cs_muxers(out: *mut RawMuxer, cap: c_int) -> c_int;
    fn cs_encoders(out: *mut RawEncoder, cap: c_int) -> c_int;
    fn cs_options(encoder: *const c_char, out: *mut RawEncoderOption, cap: c_int) -> c_int;
    fn cs_has_option(encoder: *const c_char, name: *const c_char) -> c_int;
    fn cs_pix_fmts(encoder: *const c_char, out: *mut c_int, cap: c_int) -> c_int;
    fn cs_pix_fmt(name: *const c_char) -> c_int;
    fn cs_pix_fmt_name(format: c_int) -> *const c_char;
    fn cs_codec_name(codec: c_int) -> *const c_char;
    fn cs_guess_codec(extension: *const c_char) -> c_int;
    fn cs_srgb(colour: *mut RawColour);
    fn cs_encode(
        encoder: *const c_char,
        extension: *const c_char,
        picture: *const RawPicture,
        out: *mut *mut u8,
        size: *mut c_long,
        error: *mut c_char,
        error_cap: c_int,
    ) -> c_int;
    fn cs_decode(
        path: *const c_char,
        data: *const u8,
        size: c_long,
        out: *mut *mut u8,
        out_size: *mut c_long,
        info: *mut RawDecoded,
        error: *mut c_char,
        error_cap: c_int,
    ) -> c_int;
    fn cs_free(buffer: *mut u8);
}

/// Forget what a previous probe loaded and take `directory` for the next `open`.
///
/// Probing is the moment of truth: a reload must be able to pick up a directory the user has
/// just fixed, or a library installed while the process was running.
pub fn reopen(directory: Option<&Path>) {
    let text = directory.and_then(|path| CString::new(path.to_string_lossy().as_bytes()).ok());
    let pointer = text.as_ref().map_or(std::ptr::null(), |text| text.as_ptr());
    // SAFETY: the shim copies the text into its own buffer and closes only its own handles.
    unsafe { cs_reopen(pointer) };
}

/// The libraries' versions ("avcodec avformat avutil"), or why they cannot be used.
pub fn open() -> Result<String, String> {
    // SAFETY: cs_open touches only the shim's own state and the dynamic loader.
    if unsafe { cs_open() } == 1 {
        return Ok(c_string(unsafe { cs_versions() }).unwrap_or_default());
    }
    Err(c_string(unsafe { cs_error() })
        .unwrap_or_else(|| "the ffmpeg libraries did not load".into()))
}

pub fn muxers() -> Vec<Muxer> {
    collect(|out, cap| unsafe { cs_muxers(out, cap) }, RawMuxer::empty)
        .into_iter()
        .map(|raw| {
            Muxer::new(
                text(&raw.name),
                text(&raw.long_name),
                text(&raw.extensions),
                text(&raw.mime),
            )
        })
        .collect()
}

pub fn encoders() -> Vec<Encoder> {
    collect(|out, cap| unsafe { cs_encoders(out, cap) }, RawEncoder::empty)
        .into_iter()
        .map(|raw| Encoder::new(text(&raw.name), text(&raw.long_name), raw.id))
        .collect()
}

/// The encoder's own options; `None` when there is no such encoder.
pub fn options(encoder: &str) -> Option<Vec<EncoderOption>> {
    let name = CString::new(encoder).ok()?;
    // SAFETY: `name` outlives the call, and a null `out` with cap 0 only counts.
    let total = unsafe { cs_options(name.as_ptr(), std::ptr::null_mut(), 0) };
    if total < 0 {
        return None;
    }
    let mut list = RawEncoderOption::empty(total as usize);
    // SAFETY: the buffer holds `total` records and the shim fills at most that many.
    unsafe { cs_options(name.as_ptr(), list.as_mut_ptr(), total) };
    Some(
        list.into_iter()
            .map(|raw| EncoderOption::new(text(&raw.name), text(&raw.help), text(&raw.default_text)))
            .collect(),
    )
}

/// Whether this encoder takes an option of that name; `None` when there is no such encoder.
pub fn has_option(encoder: &str, name: &str) -> Option<bool> {
    let encoder = CString::new(encoder).ok()?;
    let name = CString::new(name).ok()?;
    // SAFETY: both texts outlive the call, which only reads them.
    match unsafe { cs_has_option(encoder.as_ptr(), name.as_ptr()) } {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// The pixel formats the encoder accepts; `None` when there is no such encoder.
pub fn pix_fmts(encoder: &str) -> Option<Vec<c_int>> {
    let name = CString::new(encoder).ok()?;
    // SAFETY: as in `options`.
    let total = unsafe { cs_pix_fmts(name.as_ptr(), std::ptr::null_mut(), 0) };
    if total < 0 {
        return None;
    }
    let mut list = vec![0; total as usize];
    // SAFETY: the buffer holds `total` entries and the shim fills at most that many.
    unsafe { cs_pix_fmts(name.as_ptr(), list.as_mut_ptr(), total) };
    Some(list)
}

/// The codec libavformat itself would put in `out.<extension>`; 0 for none.
pub fn guess_codec(extension: &str) -> c_int {
    let Ok(extension) = CString::new(extension) else {
        return 0;
    };
    // SAFETY: the text is a C string for the duration of the call.
    unsafe { cs_guess_codec(extension.as_ptr()) }
}

/// The pixel format a name such as "bgra" means; -1 when there is none.
pub fn pix_fmt(name: &str) -> c_int {
    let Ok(name) = CString::new(name) else {
        return -1;
    };
    // SAFETY: the text is a C string for the duration of the call.
    unsafe { cs_pix_fmt(name.as_ptr()) }
}

/// The name of a codec id, such as "mjpeg"; `None` when the number means nothing.
pub fn codec_name(codec: c_int) -> Option<String> {
    // SAFETY: the returned pointer is libavcodec's static text.
    c_string(unsafe { cs_codec_name(codec) })
}

/// The name of a pixel format; `None` when the number means nothing.
pub fn pix_fmt_name(format: c_int) -> Option<String> {
    // SAFETY: the returned pointer is libavutil's static text.
    c_string(unsafe { cs_pix_fmt_name(format) })
}

/// The tags for 8-bit sRGB: BT.709 primaries and matrix, sRGB transfer, full range.
pub fn srgb() -> Colour {
    let mut colour = RawColour::default();
    // SAFETY: `colour` is a live record of the layout the shim fills.
    unsafe { cs_srgb(&mut colour) };
    colour.plain()
}

/// One picture, encoded and muxed into `extension`'s container.
pub fn encode(encoder: &str, extension: &str, picture: &Picture<'_>) -> Result<Vec<u8>, String> {
    let encoder = CString::new(encoder).map_err(|error| error.to_string())?;
    let extension = CString::new(extension).map_err(|error| error.to_string())?;
    let names: Vec<CString> = picture
        .options
        .iter()
        .map(|(name, _)| CString::new(*name))
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    let values: Vec<CString> = picture
        .options
        .iter()
        .map(|(_, value)| CString::new(*value))
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    let name_pointers: Vec<*const c_char> = names.iter().map(|text| text.as_ptr()).collect();
    let value_pointers: Vec<*const c_char> = values.iter().map(|text| text.as_ptr()).collect();

    let record = RawPicture {
        width: picture.width,
        height: picture.height,
        rgba: picture.rgba.as_ptr(),
        // The picture is top-down RGBA with no padding between rows.
        pitch: c_long::from(picture.width) * 4,
        pix_fmt: picture.pixel_format.map_or(-1, pix_fmt),
        colour: RawColour::of(picture.colour),
        global_quality: picture.global_quality.unwrap_or(-1),
        option_names: name_pointers.as_ptr(),
        option_values: value_pointers.as_ptr(),
        option_count: name_pointers.len() as c_int,
    };

    let mut output: *mut u8 = std::ptr::null_mut();
    let mut size: c_long = 0;
    let mut error = [0 as c_char; 512];
    // SAFETY: every pointer in the record is live for the call, the source holds
    // `height * pitch` bytes, and the shim writes the error text within its capacity.
    let code = unsafe {
        cs_encode(
            encoder.as_ptr(),
            extension.as_ptr(),
            &record,
            &mut output,
            &mut size,
            error.as_mut_ptr(),
            error.len() as c_int,
        )
    };
    if code != 0 {
        let reason = text(&error);
        return Err(if reason.is_empty() {
            format!("the ffmpeg libraries could not encode ({code})")
        } else {
            reason
        });
    }
    if output.is_null() || size <= 0 {
        return Err("the encoder produced nothing".into());
    }
    // SAFETY: the shim wrote `size` bytes at `output` with malloc and hands them over.
    let bytes = unsafe { std::slice::from_raw_parts(output, size as usize) }.to_vec();
    // SAFETY: the buffer came from the shim and is released once, here.
    unsafe { cs_free(output) };
    Ok(bytes)
}

/// Decode the first picture of `bytes`.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let mut output: *mut u8 = std::ptr::null_mut();
    let mut size: c_long = 0;
    let mut info = RawDecoded::default();
    let mut error = [0 as c_char; 512];
    // SAFETY: the bytes outlive the call, `info` is a live record of the layout the shim
    // fills, and the shim writes the error text within its capacity.
    let code = unsafe {
        cs_decode(
            std::ptr::null(),
            bytes.as_ptr(),
            bytes.len() as c_long,
            &mut output,
            &mut size,
            &mut info,
            error.as_mut_ptr(),
            error.len() as c_int,
        )
    };
    if code != 0 {
        let reason = text(&error);
        return Err(if reason.is_empty() {
            format!("the ffmpeg libraries could not decode ({code})")
        } else {
            reason
        });
    }
    let ten_bit = match info.storage {
        STORAGE_RGBA => false,
        STORAGE_PACKED10 => true,
        other => return Err(format!("the decoder returned an unknown layout ({other})")),
    };
    let expected = info.width as usize * info.height as usize * 4;
    if output.is_null() || size <= 0 || size as usize != expected {
        if !output.is_null() {
            // SAFETY: the shim handed this buffer over; it is released once, here.
            unsafe { cs_free(output) };
        }
        return Err("the decoder returned an unexpected amount of pixels".into());
    }
    // SAFETY: the shim wrote `size` bytes at `output` with malloc and hands them over.
    let pixels = unsafe { std::slice::from_raw_parts(output, size as usize) }.to_vec();
    // SAFETY: the buffer came from the shim and is released once, here.
    unsafe { cs_free(output) };
    Ok(Decoded {
        width: info.width,
        height: info.height,
        ten_bit,
        colour: info.colour.plain(),
        data: pixels,
    })
}

fn collect<T>(
    mut fill: impl FnMut(*mut T, c_int) -> c_int,
    allocate: fn(usize) -> Vec<T>,
) -> Vec<T> {
    let total = fill(std::ptr::null_mut(), 0);
    if total <= 0 {
        return Vec::new();
    }
    let mut list = allocate(total as usize);
    fill(list.as_mut_ptr(), total);
    list
}

fn c_string(text: *const c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: the shim and libavutil return NUL-terminated strings that outlive the call.
    Some(
        unsafe { std::ffi::CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// A NUL-terminated C character array as text.
fn text(bytes: &[c_char]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let bytes: Vec<u8> = bytes[..end].iter().map(|byte| *byte as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

