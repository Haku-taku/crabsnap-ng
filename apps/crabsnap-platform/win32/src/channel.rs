//! The command's side of the daemon channel on Windows.
//!
//! A request is one message and one answer over a message-mode named pipe. The daemon serves
//! it from the UI backend's channel, which reads one message and writes one answer; a daemon
//! that accepts and then says nothing is an error, never "no daemon" — starting a second one
//! would split the pins and the clipboard between two processes that cannot see each other.

use crabsnap_core::protocol::{self, Request, Response};
use windows::Win32::Foundation::{ERROR_SEM_TIMEOUT, GetLastError};
use windows::Win32::System::Pipes::{CallNamedPipeW, WaitNamedPipeW};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::HSTRING;

/// Largest message accepted, the same bound the daemon's channel holds.
const MAX_MESSAGE: usize = 64 * 1024;

/// How long a request may stay unanswered before the command fails.
const REPLY_TIMEOUT: u32 = 2_000;

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

/// The channel's address: a pipe name scoped to the user's logon session.
///
/// Every named pipe on the machine shares one namespace, so the session is part of the name;
/// without it two logged-in users would reach for the same pipe, and the default access rules
/// would then refuse each of them the other's daemon.
pub fn address() -> String {
    match session_id() {
        Some(session) => format!("crabsnap-daemon-{session}"),
        None => String::from("crabsnap-daemon"),
    }
}

fn session_id() -> Option<u32> {
    let mut session = 0u32;
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }.ok()?;
    Some(session)
}

/// The full path a pipe name names.
fn pipe_path() -> HSTRING {
    HSTRING::from(format!(r"\\.\pipe\{}", address()))
}

/// One command's reach on the daemon.
pub struct Client {
    pipe: HSTRING,
}

impl Client {
    pub fn connect() -> Result<Self, ClientError> {
        let pipe = pipe_path();
        // A zero wait asks only whether the name exists. A pipe whose instances are all busy
        // still has a daemon behind it, and the request below waits for one.
        if !unsafe { WaitNamedPipeW(&pipe, 0) }.as_bool() {
            let error = unsafe { GetLastError() };
            if error != ERROR_SEM_TIMEOUT {
                return Err(ClientError::NoDaemon);
            }
        }
        Ok(Self { pipe })
    }

    pub fn request(&mut self, request: &Request) -> Result<Response, ClientError> {
        let bytes = protocol::encode_request(request)
            .map_err(|error| ClientError::Failed(format!("encode: {error}")))?;
        let mut answer = vec![0u8; MAX_MESSAGE];
        let mut read = 0u32;
        // Connect, write the request and read the answer as one call: the pipe is the daemon's
        // per request, and a half-open connection would leave it serving nobody.
        let called = unsafe {
            CallNamedPipeW(
                &self.pipe,
                Some(bytes.as_ptr().cast()),
                bytes.len() as u32,
                Some(answer.as_mut_ptr().cast()),
                answer.len() as u32,
                &mut read,
                REPLY_TIMEOUT,
            )
        };
        if !called.as_bool() {
            let error = unsafe { GetLastError() };
            return Err(ClientError::Failed(format!(
                "the daemon did not answer: {error:?}"
            )));
        }
        protocol::decode_response(&answer[..read as usize]).map_err(ClientError::Failed)
    }
}
