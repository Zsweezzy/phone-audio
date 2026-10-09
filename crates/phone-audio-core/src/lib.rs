//! phone-audio-core — routes a Bluetooth phone's audio to this PC via PipeWire.
//!
//! Everything shells out through [`CmdRunner`]; nothing talks to PipeWire directly.

mod app;
mod cmd;
mod pw;

pub use app::{App, Phone, Status};
pub use cmd::{CmdOut, CmdRunner, RealRunner};
pub use pw::{parse_profiles, parse_volume, pick_receive_profile};

/// Errors produced by the app.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("{0}")]
    NotFound(String),
    #[error("command failed: `{cmd}`: {err}")]
    Command { cmd: String, err: String },
    #[error("{0}")]
    Config(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AudioError>;
