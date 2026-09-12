//! The TCL RPC framing, over a real TCP socket.
//!
//! A stub server stands in for OpenOCD, so the framing code — sending a
//! command terminated by `0x1a`, reading until the next one, surviving replies
//! that contain newlines — is exercised for real rather than through a
//! substituted transport. What it cannot check is whether OpenOCD's commands
//! mean what this backend assumes; only a real OpenOCD answers that.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use flash_backend_openocd::tcl::{TclLink, TcpTcl, TERMINATOR};
use flash_core::error::FlashError;

/// Starts a server that answers each command from `replies`, in order.
///
/// Returns the port and a handle that yields the commands it received.
fn stub_server(replies: Vec<String>) -> (u16, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();

    let handle = thread::spawn(move || {
        let mut received = Vec::new();
        let (mut stream, _) = listener.accept().expect("one client");
        let mut pending = Vec::new();
        let mut byte = [0u8; 1];
        let mut answered = 0usize;

        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) if byte[0] == TERMINATOR => {
                    received.push(String::from_utf8_lossy(&pending).to_string());
                    pending.clear();

                    match replies.get(answered) {
                        Some(reply) => {
                            let mut out = reply.as_bytes().to_vec();
                            out.push(TERMINATOR);
                            if stream.write_all(&out).is_err() {
                                break;
                            }
                        }
                        // Out of scripted replies: hang up, which is what a
                        // dying OpenOCD looks like from this end.
                        None => break,
                    }
                    answered += 1;
                }
                Ok(_) => pending.push(byte[0]),
                Err(_) => break,
            }
        }
        received
    });

    (port, handle)
}

#[test]
fn a_command_and_its_reply_are_framed_by_the_terminator() {
    let (port, server) = stub_server(vec!["stm32f4x.cpu".to_string()]);
    let mut link = TcpTcl::connect(&format!("127.0.0.1:{port}")).expect("the stub answers");

    assert_eq!(link.command("targets").unwrap(), "stm32f4x.cpu");
    drop(link);

    let received = server.join().unwrap();
    assert_eq!(received, vec!["targets".to_string()]);
}

#[test]
fn a_reply_containing_newlines_arrives_whole() {
    // The terminator is 0x1a, not a newline, and OpenOCD's output is full of
    // newlines: reading a line at a time would cut every reply short.
    let reply = "#0 : stm32f4x at 0x08000000, size 0x00080000\n\t# 0: 0x00000000 (0x4000 16kB)";
    let (port, server) = stub_server(vec![reply.to_string()]);
    let mut link = TcpTcl::connect(&format!("127.0.0.1:{port}")).unwrap();

    let answer = link.command("capture {flash info 0}").unwrap();
    assert_eq!(answer.lines().count(), 2, "got {answer:?}");
    assert!(answer.contains("16kB"));

    drop(link);
    let received = server.join().unwrap();
    assert_eq!(received[0], "capture {flash info 0}");
}

#[test]
fn several_commands_go_down_one_connection_in_order() {
    let (port, server) = stub_server(vec![
        "first".to_string(),
        "second".to_string(),
        "third".to_string(),
    ]);
    let mut link = TcpTcl::connect(&format!("127.0.0.1:{port}")).unwrap();

    assert_eq!(link.command("a").unwrap(), "first");
    assert_eq!(link.command("b").unwrap(), "second");
    assert_eq!(link.command("c").unwrap(), "third");

    drop(link);
    assert_eq!(server.join().unwrap(), vec!["a", "b", "c"]);
}

#[test]
fn an_openocd_that_hangs_up_is_reported_as_a_lost_connection() {
    // Not as a timeout and not as an empty reply: an empty reply is a
    // perfectly ordinary answer from OpenOCD, and treating a dead socket as
    // one would make every later command look like it quietly succeeded.
    let (port, server) = stub_server(vec!["only one".to_string()]);
    let mut link = TcpTcl::connect(&format!("127.0.0.1:{port}")).unwrap();
    assert_eq!(link.command("a").unwrap(), "only one");

    let err = link.command("b").unwrap_err();
    assert!(matches!(err, FlashError::ConnectionLost(_)), "got {err}");
    let _ = server.join();
}

#[test]
fn a_port_nothing_listens_on_names_the_address_and_what_to_start() {
    // Bind a port and drop it, so the address is almost certainly free.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    let err = TcpTcl::connect(&format!("127.0.0.1:{port}")).unwrap_err();
    let message = err.to_string();
    assert!(message.contains(&port.to_string()), "got {message}");
    assert!(
        message.contains("OpenOCD"),
        "the message has to say what is missing, got {message}"
    );
}

#[test]
fn a_loopback_endpoint_is_recognised_as_local() {
    // Which operations are allowed depends on this: programming goes through a
    // file OpenOCD opens itself.
    let (port, server) = stub_server(vec!["ok".to_string()]);
    let link = TcpTcl::connect(&format!("127.0.0.1:{port}")).unwrap();
    assert!(link.is_local());
    assert!(link.endpoint().contains(&port.to_string()));
    drop(link);
    let _ = server.join();
}

#[test]
fn a_stream_can_be_declared_non_local_for_a_remote_openocd() {
    let (port, server) = stub_server(vec!["ok".to_string()]);
    let stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let link = TcpTcl::from_stream(stream, "openocd.example:6666", false);
    assert!(!link.is_local());
    assert_eq!(link.endpoint(), "openocd.example:6666");
    drop(link);
    let _ = server.join();
}
