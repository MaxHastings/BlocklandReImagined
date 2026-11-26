//! winit says why its event loop gave up (a Wayland protocol error, a lost X
//! connection) only through `tracing`, then returns a bare exit code; the
//! lower Wayland stack reports the same failures through the `log` crate.
//! This forwards both to the console log and this run's session log, so a
//! report names the error instead of "Exit Failure: 1".

use std::fmt::{self, Write as _};
#[cfg(target_os = "linux")]
use std::io::Write as _;
use tracing_core::field::{Field, Visit};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Event, Level, LevelFilter, Metadata, Subscriber};

/// Installs the forwarders unless something else already receives `tracing`
/// (or, on Linux, the `log` facade).
pub fn install() {
    let _ = tracing_core::dispatcher::set_global_default(tracing_core::Dispatch::new(ConsoleLog));
    #[cfg(target_os = "linux")]
    install_display_log();
}

struct ConsoleLog;

impl Subscriber for ConsoleLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // Lower levels sort above WARN.
        metadata.is_event()
            && *metadata.level() <= Level::WARN
            && metadata.target().starts_with("winit")
    }
    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::WARN)
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        // Never called: `enabled` admits only events.
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut line = format!("{}:", event.metadata().target());
        event.record(&mut Fields(&mut line));
        if *event.metadata().level() == Level::ERROR {
            bri_console::error(line);
        } else {
            bri_console::warn(line);
        }
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

struct Fields<'a>(&'a mut String);

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let _ = if field.name() == "message" {
            write!(self.0, " {value:?}")
        } else {
            write!(self.0, " {}={value:?}", field.name())
        };
    }
}

// The `log` facade half (Linux only, where the Wayland stack lives): when a
// display connection dies, wayland-backend logs the reason — and with its
// `log` feature on (bri-client enables it) libwayland's C layer reports
// protocol errors through this same logger. Only warnings and errors from the
// display crates are forwarded; everything else stays out of the player's
// console.

/// Crates whose warnings and errors name a display failure: the Wayland stack
/// (wayland-backend, smithay-client-toolkit) and winit itself.
#[cfg(target_os = "linux")]
const DISPLAY_TARGETS: [&str; 3] = ["wayland", "winit", "smithay"];

#[cfg(target_os = "linux")]
struct DisplayLog;

#[cfg(target_os = "linux")]
impl log::Log for DisplayLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        is_display_record(metadata.level(), metadata.target())
    }
    fn log(&self, record: &log::Record) {
        let metadata = record.metadata();
        if !is_display_record(metadata.level(), metadata.target()) {
            return;
        }
        report_display_line(
            metadata.level() == log::Level::Error,
            format!("{}: {}", metadata.target(), record.args()),
        );
    }
    fn flush(&self) {
        // Each line is written to the console and the session file at once.
    }
}

#[cfg(target_os = "linux")]
fn is_display_record(level: log::Level, target: &str) -> bool {
    level <= log::Level::Warn
        && DISPLAY_TARGETS
            .iter()
            .any(|prefix| target.starts_with(prefix))
}

/// Mirror the line to the console (stderr and scrollback) and append it to
/// this run's session log: by the time a player looks for the reason, the
/// window is gone, and stderr was lost when the game started from a file
/// manager. The prefixed form matches what `bri_console` writes to stderr.
#[cfg(target_os = "linux")]
fn report_display_line(is_error: bool, line: String) {
    if is_error {
        bri_console::error(&line);
    } else {
        bri_console::warn(&line);
    }
    let prefixed = if is_error {
        format!("Error: {line}")
    } else {
        format!("Warning: {line}")
    };
    if let Some(path) = bri_crash::session_log_path() {
        let _ = append_to_file(&path, &prefixed);
    }
}

/// Append one line; a missing or unwritable file only loses this diagnostic.
#[cfg(target_os = "linux")]
fn append_to_file(path: &std::path::Path, line: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

#[cfg(target_os = "linux")]
fn install_display_log() {
    // Keep a logger another crate installed first (there is none today); the
    // max level keeps `log`'s per-record checks cheap on hot paths.
    if log::set_boxed_logger(Box::new(DisplayLog)).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn a_display_record_is_a_warning_or_error_from_the_display_stack() {
        assert!(is_display_record(
            log::Level::Error,
            "wayland_backend::rs::client_impl"
        ));
        assert!(is_display_record(log::Level::Warn, "winit"));
        assert!(is_display_record(
            log::Level::Warn,
            "smithay_client_toolkit"
        ));
        assert!(!is_display_record(log::Level::Info, "wayland_backend"));
        assert!(!is_display_record(log::Level::Error, "rustls"));
    }

    #[test]
    fn a_reported_line_is_appended_to_the_session_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session-test.log");
        append_to_file(&path, "Error: wayland_backend: Connection reset by peer").unwrap();
        append_to_file(&path, "Warning: winit: something").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "Error: wayland_backend: Connection reset by peer\nWarning: winit: something\n"
        );
    }
}
