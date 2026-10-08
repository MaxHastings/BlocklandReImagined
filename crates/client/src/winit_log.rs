//! winit says why its event loop gave up (a Wayland protocol error, a lost X
//! connection) only through `tracing`, then returns a bare exit code. This
//! forwards its warnings and errors to the session log.

use std::fmt::{self, Write as _};
use tracing_core::field::{Field, Visit};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Event, Level, LevelFilter, Metadata, Subscriber};

/// Installs the forwarder unless something else already receives `tracing`.
pub fn install() {
    let _ = tracing_core::dispatcher::set_global_default(tracing_core::Dispatch::new(ConsoleLog));
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
