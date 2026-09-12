//! The seam between this backend's logic and OpenOCD's TCL RPC port.
//!
//! Everything above [`TclLink`] — geometry parsing, address checks, progress,
//! verification — is testable without an OpenOCD process. Below it sits a TCP
//! client whose only job is framing.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use flash_core::error::FlashError;

/// OpenOCD's TCL RPC terminator: `0x1a`, ASCII SUB.
///
/// Both directions use it. It is not a newline, and that matters: command
/// output routinely contains newlines, so reading a line at a time would cut a
/// reply in half.
pub const TERMINATOR: u8 = 0x1a;

/// Port OpenOCD serves the TCL RPC on unless its configuration says otherwise.
pub const DEFAULT_PORT: u16 = 6666;

/// How long to wait for a reply before giving up.
///
/// Generous, because a mass erase on a large part happens inside one command
/// and OpenOCD says nothing until it finishes.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(120);

/// One open connection to an OpenOCD TCL RPC port.
pub trait TclLink: Send {
    /// Runs `command` and returns everything OpenOCD replied with, trimmed.
    ///
    /// OpenOCD reports most failures as ordinary reply text rather than by any
    /// separate channel, so callers must read the reply rather than trusting
    /// that a call returning `Ok` means the target did the thing.
    fn command(&mut self, command: &str) -> Result<String, FlashError>;

    /// Whether the OpenOCD process is on this machine.
    ///
    /// Programming goes through `flash write_image`, which makes **OpenOCD**
    /// open the file. A path written here is meaningless to a remote process,
    /// so those operations are refused rather than attempted against an
    /// endpoint that is not local.
    fn is_local(&self) -> bool;

    /// How the connection is addressed, for messages and logs.
    fn endpoint(&self) -> String;
}

/// A TCL RPC connection over TCP.
#[derive(Debug)]
pub struct TcpTcl {
    stream: TcpStream,
    endpoint: String,
    local: bool,
}

impl TcpTcl {
    /// Connects to `endpoint`, which may be `host:port`, a bare host, or a
    /// bare port. A missing port means [`DEFAULT_PORT`].
    pub fn connect(endpoint: &str) -> Result<Self, FlashError> {
        let (host, port) = split_endpoint(endpoint);
        let address = format!("{host}:{port}");

        let addresses: Vec<_> = address
            .to_socket_addrs()
            .map_err(|e| {
                FlashError::ConnectError(format!(
                    "'{address}' is not an address this machine can resolve: {e}"
                ))
            })?
            .collect();

        let first = addresses.first().ok_or_else(|| {
            FlashError::ConnectError(format!("'{address}' resolved to no addresses"))
        })?;

        let stream = TcpStream::connect_timeout(first, Duration::from_secs(5)).map_err(|e| {
            FlashError::ConnectError(format!(
                "no OpenOCD answered on {address}: {e}. Start OpenOCD with its \
                 TCL port enabled (it listens on {DEFAULT_PORT} by default), or \
                 point --openocd at the port in its configuration."
            ))
        })?;
        stream
            .set_read_timeout(Some(REPLY_TIMEOUT))
            .map_err(|e| FlashError::Io(e.to_string()))?;
        stream
            .set_nodelay(true)
            .map_err(|e| FlashError::Io(e.to_string()))?;

        let local = first.ip().is_loopback();
        Ok(Self {
            stream,
            endpoint: address,
            local,
        })
    }

    /// A connection over an already-open stream, for tests against a stub
    /// server. `local` says whether file paths are shared with the peer.
    pub fn from_stream(stream: TcpStream, endpoint: impl Into<String>, local: bool) -> Self {
        let _ = stream.set_read_timeout(Some(REPLY_TIMEOUT));
        Self {
            stream,
            endpoint: endpoint.into(),
            local,
        }
    }
}

impl TclLink for TcpTcl {
    fn command(&mut self, command: &str) -> Result<String, FlashError> {
        let mut request = command.as_bytes().to_vec();
        request.push(TERMINATOR);
        self.stream.write_all(&request).map_err(|e| {
            FlashError::ConnectionLost(format!("could not send '{command}' to OpenOCD: {e}"))
        })?;
        self.stream
            .flush()
            .map_err(|e| FlashError::ConnectionLost(e.to_string()))?;

        let mut reply = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match self.stream.read(&mut byte) {
                Ok(0) => {
                    return Err(FlashError::ConnectionLost(format!(
                        "OpenOCD closed the connection while answering '{command}'"
                    )))
                }
                Ok(_) if byte[0] == TERMINATOR => break,
                Ok(_) => reply.push(byte[0]),
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(FlashError::Timeout(format!(
                        "OpenOCD did not answer '{command}' within {}s",
                        REPLY_TIMEOUT.as_secs()
                    )))
                }
                Err(e) => {
                    return Err(FlashError::ConnectionLost(format!(
                        "reading OpenOCD's answer to '{command}' failed: {e}"
                    )))
                }
            }
        }

        Ok(String::from_utf8_lossy(&reply).trim().to_string())
    }

    fn is_local(&self) -> bool {
        self.local
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }
}

/// How long to wait when checking whether anything is listening.
///
/// Short: this runs during a probe scan, and a scan that stalls makes the
/// application look hung. A local OpenOCD answers a connect immediately.
pub const LISTEN_CHECK_TIMEOUT: Duration = Duration::from_millis(150);

/// Whether something accepts a connection on `endpoint` right now.
///
/// Used to decide whether to list the endpoint as available. It says only
/// that a socket answered -- not that it is OpenOCD, and not that an adapter
/// is attached to it -- which is the same standard the other transports list
/// by: a serial port is listed because it exists, not because a board is on
/// the other end.
pub fn is_listening(endpoint: &str) -> bool {
    let (host, port) = split_endpoint(endpoint);
    let Ok(addresses) = format!("{host}:{port}").to_socket_addrs() else {
        return false;
    };
    addresses
        .take(2)
        .any(|address| TcpStream::connect_timeout(&address, LISTEN_CHECK_TIMEOUT).is_ok())
}

/// Splits `host:port`, a bare host, or a bare port into both halves.
///
/// A bare number is a port on localhost, because that is what someone writing
/// `--openocd 4444` means. An IPv6 literal in brackets keeps its colons.
pub fn split_endpoint(endpoint: &str) -> (String, u16) {
    let text = endpoint.trim();
    if text.is_empty() {
        return ("127.0.0.1".to_string(), DEFAULT_PORT);
    }

    // [::1]:6666
    if let Some(rest) = text.strip_prefix('[') {
        if let Some((host, tail)) = rest.split_once(']') {
            let port = tail
                .strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            return (format!("[{host}]"), port);
        }
    }

    if let Ok(port) = text.parse::<u16>() {
        return ("127.0.0.1".to_string(), port);
    }

    match text.rsplit_once(':') {
        Some((host, port)) => {
            let host = if host.is_empty() { "127.0.0.1" } else { host };
            (host.to_string(), port.parse().unwrap_or(DEFAULT_PORT))
        }
        None => (text.to_string(), DEFAULT_PORT),
    }
}

/// Whether an OpenOCD reply describes a failure.
///
/// OpenOCD answers a failed command with prose on the same channel as a
/// successful one, so this is the only way to tell them apart. It errs towards
/// reporting a failure: a write that silently did nothing is worse than a
/// spurious error naming the reply that caused it.
pub fn reply_is_error(reply: &str) -> bool {
    let lower = reply.to_lowercase();
    [
        "error:",
        "invalid command name",
        "wrong # args",
        "target not halted",
        "not halted",
        "no working area",
        "timed out",
        "failed",
        "unable to",
        "couldn't",
        "could not",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_endpoint_may_be_a_host_a_port_or_both() {
        assert_eq!(
            split_endpoint("localhost:4444"),
            ("localhost".to_string(), 4444)
        );
        assert_eq!(split_endpoint("6666"), ("127.0.0.1".to_string(), 6666));
        assert_eq!(
            split_endpoint("192.168.1.5"),
            ("192.168.1.5".to_string(), DEFAULT_PORT)
        );
        assert_eq!(split_endpoint(""), ("127.0.0.1".to_string(), DEFAULT_PORT));
        assert_eq!(split_endpoint("[::1]:7000"), ("[::1]".to_string(), 7000));
        assert_eq!(split_endpoint("[::1]"), ("[::1]".to_string(), DEFAULT_PORT));
    }

    #[test]
    fn a_port_that_is_not_a_number_falls_back_to_the_default() {
        // Better than refusing: the host half is usable, and a connection
        // failure names the address it tried.
        assert_eq!(
            split_endpoint("localhost:not-a-port"),
            ("localhost".to_string(), DEFAULT_PORT)
        );
    }

    #[test]
    fn a_listening_socket_is_seen_and_a_free_port_is_not() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let taken = listener.local_addr().unwrap().port();
        assert!(is_listening(&format!("127.0.0.1:{taken}")));

        let free = {
            let temporary = TcpListener::bind("127.0.0.1:0").unwrap();
            temporary.local_addr().unwrap().port()
        };
        assert!(!is_listening(&format!("127.0.0.1:{free}")));
    }

    #[test]
    fn an_unresolvable_host_is_not_listening_rather_than_an_error() {
        // A probe scan must not fail because one endpoint cannot be resolved.
        assert!(!is_listening("no-such-host.invalid:6666"));
    }

    #[test]
    fn openocd_failures_are_recognised_in_the_reply_text() {
        assert!(reply_is_error("Error: target not halted"));
        assert!(reply_is_error("invalid command name \"flash\""));
        assert!(reply_is_error("Error: failed erasing sectors 0 to 3"));
        assert!(reply_is_error("couldn't open /tmp/x.bin"));
    }

    #[test]
    fn ordinary_replies_are_not_mistaken_for_failures() {
        assert!(!reply_is_error(""));
        assert!(!reply_is_error("stm32f4x.cpu: hardware has 6 breakpoints"));
        assert!(!reply_is_error(
            "wrote 1024 bytes from file /tmp/seg.bin in 0.123s"
        ));
        assert!(!reply_is_error("verified 1024 bytes in 0.05s"));
    }
}
