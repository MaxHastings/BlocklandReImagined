//! The in-game console (v20's `~` ConsoleDlg) without a script VM.
//!
//! - [`log`]: the process-wide console log. Any subsystem echoes, warns or
//!   reports errors into it; the console window and stderr show it.
//! - [`registry`]: typed commands and cvars that subsystems register into,
//!   with a small command-line parser, help and tab completion.
//!
//! The console is an input surface, not an authority: commands that change
//! shared game state go through the same requests and host trust checks as
//! the menus that already expose them.
pub mod log;
pub mod registry;

pub use log::{Level, Line, echo, error, warn};
pub use registry::{Command, CommandInfo, Completion, Cvar, Kind, Output, Registry, Store};
