//! The one place the engine writes to stdout.
//!
//! Every response goes out as a single line and is flushed immediately, because
//! Neovim reads them from a pipe: a buffered `ready` that never flushed would
//! leave the editor waiting on an engine that had already started.

use std::io::{self, Write};

use crate::ipc::IpcResponse;

pub fn emit_response(response: &IpcResponse) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(response.to_json_line().as_bytes());
    let _ = lock.flush();
}

pub fn emit_error(code: &str, message: impl Into<String>) {
    emit_response(&IpcResponse::Error {
        code: code.to_string(),
        message: message.into(),
    });
}

pub fn emit_warning(code: &str, message: impl Into<String>) {
    emit_response(&IpcResponse::Warning {
        code: code.to_string(),
        message: message.into(),
    });
}
