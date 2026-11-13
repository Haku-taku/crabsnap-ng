//! The command's side of the daemon channel.
//!
//! A request is one message and one answer; a daemon that accepts and then says nothing is an
//! error, never "no daemon" — starting a second one would split the pins and the clipboard
//! between two processes that cannot see each other.

use std::os::fd::{AsFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

use crabsnap_core::protocol::{self, Request, Response};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketAddrUnix, SocketFlags, SocketType, connect, recv,
    send, socket_with,
};

/// Largest message accepted. Pixels never travel here: they are a file or a shared buffer.
const MAX_MESSAGE: usize = 64 * 1024;

/// How long a request may stay unanswered before the command fails.
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// Why a command could not reach the daemon.
#[derive(Debug)]
pub enum ClientError {
    /// Nothing is listening: the command may run the service itself.
    NoDaemon,
    /// Something answered badly, or did not answer.
    Failed(String),
}

impl ClientError {
    pub fn message(&self) -> String {
        match self {
            Self::NoDaemon => "no daemon is running".into(),
            Self::Failed(error) => error.clone(),
        }
    }
}

/// The channel's address: per display, short enough for a socket path, cleaned at logout.
///
/// A daemon is bound to the compositor it connected to, so two sessions of one user must not
/// share a channel; the display's own name is what tells them apart.
pub fn address() -> String {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    let name = if display.is_empty() {
        "daemon.sock".to_string()
    } else {
        format!("daemon-{}.sock", sanitized(&display))
    };
    dir.join("crabsnap")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// Whatever the compositor calls its socket is reduced to characters a path can hold.
fn sanitized(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '-',
        })
        .collect()
}

/// One command's connection to the daemon.
pub struct Client {
    fd: OwnedFd,
}

impl Client {
    pub fn connect() -> Result<Self, ClientError> {
        let address = address();
        if !std::path::Path::new(&address).exists() {
            return Err(ClientError::NoDaemon);
        }
        let fd = socket_with(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        )
        .map_err(|error| ClientError::Failed(format!("socket: {error}")))?;
        let socket = SocketAddrUnix::new(address.as_str())
            .map_err(|error| ClientError::Failed(format!("address: {error}")))?;
        match connect(&fd, &socket) {
            Ok(()) => Ok(Self { fd }),
            // A socket file with no listener behind it is a daemon that died; the command
            // treats it as absent and the next daemon start replaces the file.
            Err(Errno::CONNREFUSED | Errno::NOENT) => Err(ClientError::NoDaemon),
            Err(error) => Err(ClientError::Failed(format!("connect: {error}"))),
        }
    }

    pub fn request(&mut self, request: &Request) -> Result<Response, ClientError> {
        let bytes = protocol::encode_request(request)
            .map_err(|error| ClientError::Failed(format!("encode: {error}")))?;
        send(&self.fd, &bytes, SendFlags::NOSIGNAL)
            .map_err(|error| ClientError::Failed(format!("send: {error}")))?;
        self.receive()
    }

    fn receive(&mut self) -> Result<Response, ClientError> {
        let timeout = Timespec {
            tv_sec: REPLY_TIMEOUT.as_secs() as i64,
            tv_nsec: REPLY_TIMEOUT.subsec_nanos() as i64,
        };
        let mut fds = [PollFd::from_borrowed_fd(self.fd.as_fd(), PollFlags::IN)];
        let ready = poll(&mut fds, Some(&timeout))
            .map_err(|error| ClientError::Failed(format!("wait for an answer: {error}")))?;
        if ready == 0 {
            return Err(ClientError::Failed(
                "the daemon accepted the request but did not answer".into(),
            ));
        }
        let mut buffer = vec![0u8; MAX_MESSAGE];
        match recv(&self.fd, &mut buffer[..], RecvFlags::empty()) {
            Ok((0, _)) => Err(ClientError::Failed(
                "the daemon closed the channel without answering".into(),
            )),
            Ok((received, _)) => {
                protocol::decode_response(&buffer[..received]).map_err(ClientError::Failed)
            }
            Err(error) => Err(ClientError::Failed(format!("receive: {error}"))),
        }
    }
}
