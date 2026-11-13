//! Whole-screen capture through `org.freedesktop.portal.Screenshot`.
//!
//! The desktop takes the picture itself and hands back a file URI, so this rung needs no
//! Wayland protocol at all. It is the last resort: it cannot choose an output, and
//! `interactive: false` is a request, not a promise — a portal may still ask the user. The
//! round trip can only be waited in, so it answers [`Started::Blocking`].

use std::collections::HashMap;
use std::time::Duration;

use crabsnap_core::capture::{Backend, Frame, PixelFormat, Started};
use ui_core::colour::ContentColour;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

/// Bus name owning every portal object.
const DESKTOP_DEST: &str = "org.freedesktop.portal.Desktop";
/// Object path of the portal's desktop root.
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
/// Interface that takes screenshots.
const SCREENSHOT_IFACE: &str = "org.freedesktop.portal.Screenshot";
/// Interface of a portal request, which answers with `Response`.
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";

/// The bound rung.
pub(crate) struct Portal {
    connection: zbus::blocking::Connection,
}

impl Portal {
    /// Bind the portal, or `None` when this session has none with `Screenshot`.
    pub(crate) fn new() -> Result<Option<Self>, String> {
        let Ok(connection) = zbus::blocking::Connection::session() else {
            return Ok(None);
        };
        // A machine without the portal, or one too old to have `Screenshot`, is the same
        // answer to us: this rung is unavailable.
        let Ok(portal) = screenshot_proxy(&connection) else {
            return Ok(None);
        };
        match portal.get_property::<u32>("version") {
            Ok(version) if version >= 1 => Ok(Some(Self { connection })),
            _ => Ok(None),
        }
    }

    fn take_picture(&self, timeout: Duration) -> Result<Frame, String> {
        // The token names the request object path; one grab per process id is enough.
        let token = format!("crabsnap{}", std::process::id());
        let path = request_path(&self.connection, &token);
        // Subscribe before the call: the answer may arrive before the call returns.
        let request = zbus::blocking::Proxy::new_owned(
            self.connection.clone(),
            DESKTOP_DEST,
            path.clone(),
            REQUEST_IFACE,
        )
        .map_err(|error| format!("portal request: {error}"))?;
        let signals = request
            .receive_signal("Response")
            .map_err(|error| format!("portal response stream: {error}"))?;

        let mut options: HashMap<String, OwnedValue> = HashMap::new();
        options.insert("handle_token".into(), owned_str(&token)?);
        // Our overlay owns the selection; a portal picker would be a second one.
        options.insert(
            "interactive".into(),
            Value::from(false)
                .try_to_owned()
                .map_err(|error| format!("portal options: {error}"))?,
        );
        // No parent: the capture happens before this process has a window.
        let handle: OwnedObjectPath = screenshot_proxy(&self.connection)?
            .call("Screenshot", &("", options))
            .map_err(|error| format!("portal Screenshot: {error}"))?;
        // The response is emitted on the returned handle. A portal that chose another one
        // (older than 0.9) will never answer the path we are listening on.
        if handle.as_str() != path {
            return Err(format!(
                "the portal answered on {} instead of {path}",
                handle.as_str()
            ));
        }

        let answer = match wait_response(signals, timeout) {
            Ok(answer) => answer,
            Err(error) => {
                // Nobody is waiting for this request any more; leaving it open would leave
                // the desktop's prompt on screen after we gave up.
                let _ = request.call_noreply("Close", &());
                return Err(error);
            }
        };
        let (code, results) = answer;
        if code == 1 {
            return Err("the user cancelled the portal screenshot".into());
        }
        if code != 0 {
            return Err(format!("the portal failed the screenshot: code {code}"));
        }
        let uri = results
            .get("uri")
            .and_then(|value| <&str>::try_from(value).ok())
            .ok_or_else(|| "the portal returned no screenshot uri".to_string())?;
        let path = file_uri_to_path(uri)?;
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        decode_png(&bytes)
    }
}

impl Backend for Portal {
    fn name(&self) -> &'static str {
        "portal"
    }

    fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        self.take_picture(timeout)
    }

    fn begin(self: Box<Self>, _timeout: Duration) -> Result<Started, String> {
        Ok(Started::Blocking(self))
    }
}

/// A proxy on the desktop root for the `Screenshot` interface.
fn screenshot_proxy(
    connection: &zbus::blocking::Connection,
) -> Result<zbus::blocking::Proxy<'static>, String> {
    zbus::blocking::Proxy::new(connection, DESKTOP_DEST, DESKTOP_PATH, SCREENSHOT_IFACE)
        .map_err(|error| format!("portal: {error}"))
}

/// The object path a portal request will live at for `token`.
fn request_path(connection: &zbus::blocking::Connection, token: &str) -> String {
    // The portal derives the path from our own bus name: the leading `:` goes and every `.`
    // becomes `_`.
    let unique = connection
        .unique_name()
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| "unknown".into());
    format!(
        "{DESKTOP_PATH}/request/{}/{}",
        unique.trim_start_matches(':').replace('.', "_"),
        token
    )
}

/// An `OwnedValue` holding a string, for the vardicts the portal methods take.
fn owned_str(text: &str) -> Result<OwnedValue, String> {
    Value::from(text)
        .try_to_owned()
        .map_err(|error| format!("portal options: {error}"))
}

/// Read the request's `Response` signal, or give up at `timeout`.
///
/// The blocking iterator has no deadline of its own, so it waits on a thread while the caller
/// waits on the channel: a portal that never answers parks one thread instead of hanging us.
fn wait_response(
    mut signals: zbus::blocking::proxy::SignalIterator<'static>,
    timeout: Duration,
) -> Result<(u32, HashMap<String, OwnedValue>), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("portal-screenshot".into())
        .spawn(move || {
            let _ = sender.send(signals.next());
        })
        .map_err(|error| format!("portal reader: {error}"))?;
    let message = receiver
        .recv_timeout(timeout)
        .map_err(|_| "the portal did not answer in time".to_string())?
        .ok_or_else(|| "the portal closed the request".to_string())?;
    message
        .body()
        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
        .map_err(|error| format!("malformed portal response: {error}"))
}

/// The portal's PNG as the BGRA rows the protocol rungs produce.
fn decode_png(bytes: &[u8]) -> Result<Frame, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| format!("decode the portal screenshot: {error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut pixels = image.into_raw();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Frame {
        width,
        height,
        stride: width * 4,
        pixels,
        format: PixelFormat::Bgra8,
        // A PNG is sRGB by definition, whatever the desktop it photographed was.
        colour: ContentColour::SDR,
        // The desktop names no output: the portal captures whatever it shows.
        output: None,
        // Nor does it say what a point was worth; a PNG pixel is taken as a point.
        scale: 1.0,
    })
}

/// The local path behind the portal's `file://` URI.
fn file_uri_to_path(uri: &str) -> Result<std::path::PathBuf, String> {
    let rest = uri
        .strip_prefix("file://")
        .ok_or_else(|| format!("the portal returned {uri:?}, not a file"))?;
    let (host, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => return Err(format!("the portal returned {uri:?}, not a file path")),
    };
    if !host.is_empty() && host != "localhost" {
        return Err(format!("{uri:?} names host {host:?}, not this machine"));
    }
    let bytes = percent_decode(path)?;
    use std::os::unix::ffi::OsStringExt;
    Ok(std::path::PathBuf::from(std::ffi::OsString::from_vec(
        bytes,
    )))
}

/// Undo the `%XX` escapes a file URI puts on bytes outside the URI alphabet.
fn percent_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let high = bytes.next().and_then(hex_digit);
        let low = bytes.next().and_then(hex_digit);
        match (high, low) {
            (Some(high), Some(low)) => out.push(high << 4 | low),
            _ => return Err(format!("{text:?} has a truncated percent escape")),
        }
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The portal names its file as a URI, and the path behind it may carry percent escapes.
    #[test]
    fn a_file_uri_decodes_to_its_path() {
        assert_eq!(
            file_uri_to_path("file:///run/user/1000/Screenshot%20from%202026-11-09.png").unwrap(),
            std::path::PathBuf::from("/run/user/1000/Screenshot from 2026-11-09.png")
        );
        assert_eq!(
            file_uri_to_path("file://localhost/a%2Bb.png").unwrap(),
            std::path::PathBuf::from("/a+b.png")
        );
        assert!(
            file_uri_to_path("file://otherhost/a.png").is_err(),
            "another host's path is not ours"
        );
        assert!(
            percent_decode("%zz").is_err(),
            "a truncated escape is not a byte we can guess"
        );
    }

    /// The portal answers with a PNG; the frame it becomes must have the same BGRA layout as
    /// the protocol rungs, alpha included.
    #[test]
    fn a_portal_png_decodes_to_bgra_rows() {
        let source =
            image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
        let mut png = Vec::new();
        source
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let frame = decode_png(&png).unwrap();
        assert_eq!((frame.width, frame.height, frame.stride), (2, 1, 8));
        assert_eq!(frame.format, PixelFormat::Bgra8);
        assert_eq!(frame.pixels, vec![0, 0, 255, 255, 255, 0, 0, 128]);
    }
}
