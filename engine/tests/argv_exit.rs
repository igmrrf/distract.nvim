//! A malformed argument must be reported *and* exit non-zero.
//!
//! The `error` response alone was not enough: `jobstart`'s `on_exit` treats
//! code 0 as a clean shutdown, so an engine that refused its own arguments and
//! exited 0 looked to Neovim exactly like one the user had stopped.
//!
//! This runs the real binary. It never reaches window creation, so it is safe
//! headless.

use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn a_malformed_argument_exits_non_zero_and_says_why() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_distract-engine"))
        .args(["--overlay-monitor", "left"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("engine binary should be runnable");

    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("stdout was piped")
        .read_to_string(&mut stdout)
        .expect("engine stdout should be readable");

    let status = child.wait().expect("engine should terminate");

    assert!(
        stdout.contains(r#""code":"INVALID_ARGUMENT""#),
        "expected an INVALID_ARGUMENT response, got: {stdout}"
    );
    assert_eq!(
        status.code(),
        Some(1),
        "a refused argument must exit non-zero"
    );
}
