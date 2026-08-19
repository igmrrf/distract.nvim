//! A malformed argument must be reported *and* exit non-zero.
//!
//! The `error` response alone was not enough: `jobstart`'s `on_exit` treats
//! code 0 as a clean shutdown, so an engine that refused its own arguments and
//! exited 0 looked to Neovim exactly like one the user had stopped.
//!
//! This runs the real binary, with `DISPLAY` and `WAYLAND_DISPLAY` removed from
//! its environment so the invariant holds on a developer's desktop as well as on
//! a headless runner.
//!
//! Argument parsing must happen *before* the winit event loop is built. Building
//! one needs a window system, so with the two the other way round the engine
//! died on a display-less Linux session before it could report the bad argument
//! — passing on macOS, where the loop builds headless, and failing only in CI.

use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn a_malformed_argument_exits_non_zero_and_says_why() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_distract-engine"))
        .args(["--overlay-monitor", "left"])
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
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
