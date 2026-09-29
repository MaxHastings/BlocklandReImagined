//! The console log: a bounded, process-wide list of lines with v20's three
//! levels (GuiConsole draws them in the profile's normal, HL and NA colours).

use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub level: Level,
    pub text: String,
}

/// Lines kept for scrollback; older lines are dropped first.
pub const CAPACITY: usize = 2000;

/// A bounded log. [`revision`](Log::revision) changes whenever the lines do,
/// so readers copy only after a change.
#[derive(Debug, Default)]
pub struct Log {
    lines: VecDeque<Line>,
    revision: u64,
}

impl Log {
    pub const fn new() -> Log {
        Log {
            lines: VecDeque::new(),
            revision: 0,
        }
    }
    /// Append `text`, one line per `\n` (Torque splits console output the
    /// same way). Tabs become spaces; other control characters are dropped.
    pub fn push(&mut self, level: Level, text: &str) {
        for part in text.split('\n') {
            let text: String = part
                .trim_end_matches('\r')
                .chars()
                .map(|c| if c == '\t' { ' ' } else { c })
                .filter(|c| !c.is_control())
                .collect();
            if self.lines.len() == CAPACITY {
                self.lines.pop_front();
            }
            self.lines.push_back(Line { level, text });
        }
        self.revision += 1;
    }
    pub fn clear(&mut self) {
        self.lines.clear();
        self.revision += 1;
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn lines(&self) -> impl ExactSizeIterator<Item = &Line> {
        self.lines.iter()
    }
}

static LOG: Mutex<Log> = Mutex::new(Log::new());

fn with<R>(f: impl FnOnce(&mut Log) -> R) -> R {
    // A panic while holding the lock leaves the log itself consistent.
    let mut log = LOG.lock().unwrap_or_else(|poison| poison.into_inner());
    f(&mut log)
}

/// Write to the console log and mirror it to stderr.
pub fn print(level: Level, text: &str) {
    match level {
        Level::Normal => eprintln!("{text}"),
        Level::Warning => eprintln!("Warning: {text}"),
        Level::Error => eprintln!("Error: {text}"),
    }
    with(|log| log.push(level, text));
}

/// Torque `echo`.
pub fn echo(text: impl AsRef<str>) {
    print(Level::Normal, text.as_ref());
}

/// Torque `warn`.
pub fn warn(text: impl AsRef<str>) {
    print(Level::Warning, text.as_ref());
}

/// Torque `error`.
pub fn error(text: impl AsRef<str>) {
    print(Level::Error, text.as_ref());
}

/// Empty the log (the console's `cls`).
pub fn clear() {
    with(Log::clear);
}

/// Changes whenever the global log does.
pub fn revision() -> u64 {
    with(|log| log.revision())
}

/// A copy of the global log's lines, oldest first.
pub fn lines() -> Vec<Line> {
    with(|log| log.lines().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines_and_stays_bounded() {
        let mut log = Log::new();
        log.push(Level::Warning, "a\r\nb\tc\u{7}");
        let got: Vec<_> = log.lines().map(|l| (l.level, l.text.as_str())).collect();
        assert_eq!(got, [(Level::Warning, "a"), (Level::Warning, "b c")]);
        let before = log.revision();
        for i in 0..CAPACITY + 5 {
            log.push(Level::Normal, &i.to_string());
        }
        assert!(log.revision() > before);
        assert_eq!(log.lines().len(), CAPACITY);
        assert_eq!(log.lines().next().unwrap().text, "5");
        log.clear();
        assert_eq!(log.lines().len(), 0);
    }
}
