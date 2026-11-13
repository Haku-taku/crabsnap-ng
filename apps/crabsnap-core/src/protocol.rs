//! What a command asks the daemon for, and what the daemon answers.
//!
//! One message is a fixed header and a bincode payload: the header carries the magic, the
//! protocol version, the operation and the payload length, so a stream transport can frame
//! messages and an older client is told its version is wrong instead of misparsing bytes.

use serde::{Deserialize, Serialize};

use crate::media::Kind;

/// Bumped when a variant or a field changes; a client with another version is refused.
pub const VERSION: u16 = 11;

/// Marks this app's messages, so a stray datagram on the channel is dropped, not parsed.
pub const MAGIC: [u8; 4] = *b"CSNP";

/// `magic`, `version`, `op`, payload length.
pub const HEADER_LEN: usize = 12;

/// What a request does, without its payload: one number in the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Op {
    Ping = 1,
    Quit = 2,
    Reload = 3,
    Status = 4,
    PinList = 5,
    PinAdd = 6,
    PinClose = 7,
    PinVisible = 8,
    PinExport = 9,
    CopyImage = 10,
    PinEdit = 11,
}

impl Op {
    fn from_u16(raw: u16) -> Result<Self, String> {
        match raw {
            1 => Ok(Self::Ping),
            2 => Ok(Self::Quit),
            3 => Ok(Self::Reload),
            4 => Ok(Self::Status),
            5 => Ok(Self::PinList),
            6 => Ok(Self::PinAdd),
            7 => Ok(Self::PinClose),
            8 => Ok(Self::PinVisible),
            9 => Ok(Self::PinExport),
            10 => Ok(Self::CopyImage),
            11 => Ok(Self::PinEdit),
            other => Err(format!("unknown operation {other}")),
        }
    }
}

/// Where a pin's material comes from; only sources cross the socket, never pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PinSource {
    /// Files the daemon reads itself, one pin each.
    Paths(Vec<String>),
    /// One CSS colour code, as typed on the command line.
    Colour(String),
    /// The process's selection: the daemon reads it on the loop, one pin per file or one
    /// for a blob.
    Clipboard,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Ping,
    Quit,
    Reload,
    Status,
    PinList,
    /// Put materials on the screen: the daemon reads the files, so no pixels cross the socket.
    PinAdd {
        source: PinSource,
        /// `--as`: the kind to trust instead of detection.
        as_kind: Option<Kind>,
        output: Option<usize>,
    },
    PinClose {
        id: Option<u64>,
    },
    /// Open the annotation editor over one pin, as its menu's Edit row does.
    PinEdit {
        id: u64,
    },
    /// Hide, show or toggle one held pin; `shown: None` toggles.
    PinVisible {
        id: Option<u64>,
        shown: Option<bool>,
    },
    PinExport {
        id: u64,
        path: String,
        /// `png`, `jpeg` or `webp`; `None` lets the path and the config decide.
        format: Option<String>,
    },
    /// Hold a PNG on the clipboard: the daemon reads the file, so no pixels cross the socket,
    /// and keeps serving it after the command that asked exits.
    CopyImage {
        path: String,
    },
}

impl Request {
    pub fn op(&self) -> Op {
        match self {
            Self::Ping => Op::Ping,
            Self::Quit => Op::Quit,
            Self::Reload => Op::Reload,
            Self::Status => Op::Status,
            Self::PinList => Op::PinList,
            Self::PinAdd { .. } => Op::PinAdd,
            Self::PinClose { .. } => Op::PinClose,
            Self::PinVisible { .. } => Op::PinVisible,
            Self::PinExport { .. } => Op::PinExport,
            Self::PinEdit { .. } => Op::PinEdit,
            Self::CopyImage { .. } => Op::CopyImage,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Pong,
    Ok,
    /// The command failed; the text is what the user gets to read.
    Error(String),
    Status(Status),
    Pins(Vec<PinInfo>),
}

/// What the daemon knows about itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    /// The chosen capture rung, or `none`.
    pub capture: String,
    /// Output names, in the compositor's order.
    pub outputs: Vec<String>,
    /// Pins currently held.
    pub pins: usize,
    /// The hotkeys the daemon has bound, one `action = chord` line each.
    pub hotkeys: Vec<String>,
}

/// Where a pin's material came from: the pin's own provenance, not the request that made it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinOrigin {
    /// A capture this app took, named by the output it came from.
    Capture { output: Option<String> },
    /// A file the daemon opened, as the command or the file list named it.
    File { path: String },
    /// A selection read from the clipboard.
    Selection { mime: String },
    /// A colour code the user typed.
    Colour { code: String },
}

impl PinOrigin {
    /// One word for `pin list`: the provenance, without the pin's placement.
    pub fn describe(&self) -> String {
        match self {
            Self::Capture { output } => output.clone().unwrap_or_else(|| "capture".into()),
            Self::File { path } => format!("file:{path}"),
            Self::Selection { mime } => format!("clipboard:{mime}"),
            Self::Colour { code } => format!("colour:{code}"),
        }
    }
}

/// One on-screen pin, as `pin list` prints it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinInfo {
    pub id: u64,
    /// What the pin holds, as `Kind::name` spells it in `pin list`.
    pub kind: Kind,
    /// The content's own name: a text pin's file name, a picture has none.
    pub title: Option<String>,
    pub width: u32,
    pub height: u32,
    pub x: f32,
    pub y: f32,
    /// What the pin was made from.
    pub origin: PinOrigin,
    /// The output the pin sits on, when its owner knows the name.
    pub output: Option<String>,
    pub shown: bool,
}

/// A request as bytes.
pub fn encode_request(request: &Request) -> Result<Vec<u8>, String> {
    encode(request.op(), request)
}

/// A response to `op` as bytes.
pub fn encode_response(op: Op, response: &Response) -> Result<Vec<u8>, String> {
    encode(op, response)
}

pub fn decode_request(bytes: &[u8]) -> Result<Request, String> {
    let (op, payload) = split(bytes)?;
    let request: Request = bincode::deserialize(payload).map_err(|e| format!("request: {e}"))?;
    if request.op() != op {
        return Err(format!(
            "header says {op:?}, payload says {:?}",
            request.op()
        ));
    }
    Ok(request)
}

pub fn decode_response(bytes: &[u8]) -> Result<Response, String> {
    let (op, payload) = split(bytes)?;
    let response: Response = bincode::deserialize(payload).map_err(|e| format!("response: {e}"))?;
    let expected = matches!(
        (&response, op),
        (Response::Status(_), Op::Status)
            | (Response::Pins(_), Op::PinList)
            | (Response::Pong, Op::Ping)
            | (
                Response::Ok,
                Op::Quit
                    | Op::Reload
                    | Op::PinAdd
                    | Op::PinClose
                    | Op::PinVisible
                    | Op::PinExport
                    | Op::PinEdit
                    | Op::CopyImage,
            )
            | (Response::Error(_), _)
    );
    if !expected {
        return Err(format!("{response:?} cannot answer {op:?}"));
    }
    Ok(response)
}

fn encode<T: Serialize>(op: Op, value: &T) -> Result<Vec<u8>, String> {
    // The payload is written straight after the header, into the message's own buffer: the
    // length is the only byte the encoder cannot know first, so it is patched afterwards.
    let mut bytes = Vec::with_capacity(HEADER_LEN + 64);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&(op as u16).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bincode::serialize_into(&mut bytes, value).map_err(|e| format!("encode: {e}"))?;
    let length = u32::try_from(bytes.len() - HEADER_LEN)
        .map_err(|_| "message too large".to_string())?;
    bytes[HEADER_LEN - 4..HEADER_LEN].copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

fn split(bytes: &[u8]) -> Result<(Op, &[u8]), String> {
    if bytes.len() < HEADER_LEN {
        return Err(format!("{} bytes is shorter than a header", bytes.len()));
    }
    if bytes[..4] != MAGIC {
        return Err("not a crabsnap message".into());
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION {
        return Err(format!(
            "protocol version {version}, this build speaks {VERSION}"
        ));
    }
    let op = Op::from_u16(u16::from_le_bytes([bytes[6], bytes[7]]))?;
    let length = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let payload = &bytes[HEADER_LEN..];
    if payload.len() != length {
        return Err(format!(
            "header promises {length} payload bytes, the message has {}",
            payload.len()
        ));
    }
    Ok((op, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pin command with a value survives the wire, and its answer passes the gate that
    /// pairs a response with its operation — the line that is easiest to forget.
    #[test]
    fn a_pin_edit_survives_the_wire_and_its_ok_is_an_answer() {
        let request = Request::PinEdit { id: 7 };
        let bytes = encode_request(&request).expect("encode");
        assert_eq!(decode_request(&bytes).expect("decode"), request);

        let answer = encode_response(Op::PinEdit, &Response::Ok).expect("encode");
        assert!(matches!(decode_response(&answer), Ok(Response::Ok)));
    }

    #[test]
    fn a_request_survives_the_wire_and_a_broken_header_is_refused() {
        let bytes = encode_request(&Request::Status).expect("encode");
        assert_eq!(decode_request(&bytes).expect("decode"), Request::Status);

        let mut wrong_version = bytes.clone();
        wrong_version[4] = VERSION.wrapping_add(1) as u8;
        assert!(
            decode_request(&wrong_version).is_err(),
            "versions must match"
        );

        let mut short_length = bytes.clone();
        let length = (bytes.len() - HEADER_LEN) as u32 - 1;
        short_length[8..12].copy_from_slice(&length.to_le_bytes());
        assert!(decode_request(&short_length).is_err(), "length must match");

        let mut wrong_magic = bytes.clone();
        wrong_magic[0] = b'X';
        assert!(decode_request(&wrong_magic).is_err(), "magic must match");
    }

    #[test]
    fn a_response_must_answer_the_operation_it_names() {
        let bytes = encode_response(Op::Status, &Response::Pong).expect("encode");
        assert!(decode_response(&bytes).is_err(), "Pong is no status answer");

        let bytes = encode_response(Op::Ping, &Response::Pong).expect("encode");
        assert_eq!(decode_response(&bytes).expect("decode"), Response::Pong);
    }
}
