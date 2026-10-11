//! Where each frame's time went on the main thread, so a slow frame in a
//! player's session log names the work that took it instead of only its
//! length.
//!
//! The frame loop and the app open named [`span`]s around their work and
//! [`note`] one-off events (a renderer rebuild, a world upload, a focus
//! change). The platform closes each frame with [`finish`], which hands back
//! a [`FrameRecord`]: the frame's length, whether the window was in the
//! background, each span's time (nested spans keep their parent) and the
//! notes. Spans cost two clock reads, and nothing is kept between frames, so
//! tracing is always on. It is per thread: only the main thread's frame
//! loop calls `finish`, and spans opened on other threads are never read.
use std::cell::RefCell;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// Notes kept per frame; one-off events, so more means a burst of the same.
const MAX_NOTES: usize = 8;

/// One named stretch of the frame, summed over every time it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanTime {
    pub name: &'static str,
    /// The span this one ran inside, if any.
    pub parent: Option<&'static str>,
    pub time: Duration,
    pub count: u32,
}

/// One finished frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameRecord {
    /// From the start of the previous frame's update to this one's.
    pub total: Duration,
    /// The window was unfocused or hidden at some point in the frame, so it
    /// was paced by the background timer, not drawn as fast as it could be.
    pub background: bool,
    /// In the order each span first closed.
    pub spans: Vec<SpanTime>,
    pub notes: Vec<String>,
}

#[derive(Default)]
struct Frame {
    spans: Vec<SpanTime>,
    stack: Vec<&'static str>,
    notes: Vec<String>,
    dropped_notes: u32,
}

thread_local! {
    static FRAME: RefCell<Frame> = RefCell::new(Frame::default());
}

/// A running span; its time is added when it drops.
#[must_use = "a span measures until it is dropped"]
pub struct Span {
    name: &'static str,
    parent: Option<&'static str>,
    start: Instant,
}

/// Time `name` until the returned guard drops. Spans opened inside it are
/// its children.
pub fn span(name: &'static str) -> Span {
    let parent = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let parent = f.stack.last().copied();
        f.stack.push(name);
        parent
    });
    Span {
        name,
        parent,
        start: Instant::now(),
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let time = self.start.elapsed();
        FRAME.with(|f| {
            let mut f = f.borrow_mut();
            if let Some(i) = f.stack.iter().rposition(|n| *n == self.name) {
                f.stack.remove(i);
            }
            add_to(&mut f, self.name, self.parent, time);
        });
    }
}

/// Add time measured some other way (waiting in the event loop) as a
/// top-level span.
pub fn add(name: &'static str, time: Duration) {
    FRAME.with(|f| add_to(&mut f.borrow_mut(), name, None, time));
}

fn add_to(f: &mut Frame, name: &'static str, parent: Option<&'static str>, time: Duration) {
    match f
        .spans
        .iter_mut()
        .find(|s| s.name == name && s.parent == parent)
    {
        Some(s) => {
            s.time += time;
            s.count += 1;
        }
        None => f.spans.push(SpanTime {
            name,
            parent,
            time,
            count: 1,
        }),
    }
}

/// Something that happened this frame and may explain its length. Repeats
/// of the same text are kept once.
pub fn note(text: impl Into<String>) {
    let text = text.into();
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        if f.notes.contains(&text) {
            return;
        }
        if f.notes.len() < MAX_NOTES {
            f.notes.push(text);
        } else {
            f.dropped_notes += 1;
        }
    });
}

/// Close the frame and start the next.
pub fn finish(total: Duration, background: bool) -> FrameRecord {
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let mut notes = std::mem::take(&mut f.notes);
        let dropped = std::mem::take(&mut f.dropped_notes);
        if dropped > 0 {
            notes.push(format!("{dropped} more events"));
        }
        FrameRecord {
            total,
            background,
            spans: std::mem::take(&mut f.spans),
            notes,
        }
    })
}

/// Throw away anything traced so far (a test starting clean).
pub fn reset() {
    FRAME.with(|f| *f.borrow_mut() = Frame::default());
}

impl FrameRecord {
    /// Time in top-level spans.
    pub fn traced(&self) -> Duration {
        self.spans
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| s.time)
            .sum()
    }
    /// A top-level span's time.
    pub fn top(&self, name: &str) -> Duration {
        self.spans
            .iter()
            .filter(|s| s.parent.is_none() && s.name == name)
            .map(|s| s.time)
            .sum()
    }
    /// The frame's spans, longest first with their longest children, then
    /// what no span covered and the notes: "update 3.1 ms (network 0.2,
    /// local game 2.6), draw 380.2 ms (world upload 351.0, chunk uploads
    /// 20.3), present 1.0 ms, untraced 0.4 ms; events: graphics rebuilt".
    /// Spans under 0.1 ms are left out.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let mut tops: Vec<&SpanTime> = self.spans.iter().filter(|s| s.parent.is_none()).collect();
        tops.sort_by(|a, b| b.time.cmp(&a.time));
        for top in tops.iter().filter(|s| s.time >= SHOWN) {
            if !out.is_empty() {
                out.push_str(", ");
            }
            let _ = write!(out, "{} {}", top.name, ms(top.time));
            let mut children: Vec<&SpanTime> = self
                .spans
                .iter()
                .filter(|s| s.parent == Some(top.name) && s.time >= SHOWN)
                .collect();
            children.sort_by(|a, b| b.time.cmp(&a.time));
            if !children.is_empty() {
                out.push_str(" (");
                for (i, child) in children.iter().take(6).enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(out, "{} {:.1}", child.name, millis(child.time));
                    if child.count > 1 {
                        let _ = write!(out, " x{}", child.count);
                    }
                }
                out.push(')');
            }
        }
        let untraced = self.total.saturating_sub(self.traced());
        if untraced >= SHOWN {
            if !out.is_empty() {
                out.push_str(", ");
            }
            let _ = write!(out, "untraced {}", ms(untraced));
        }
        if !self.notes.is_empty() {
            let _ = write!(out, "; events: {}", self.notes.join(", "));
        }
        out
    }
}

const SHOWN: Duration = Duration::from_micros(100);

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", millis(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(ms: u64) {
        let until = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < until {
            std::hint::spin_loop();
        }
    }

    #[test]
    fn spans_nest_and_sum_per_name() {
        reset();
        {
            let _update = span("update");
            for _ in 0..3 {
                let _n = span("network");
                wait(1);
            }
        }
        {
            let _draw = span("draw");
            wait(2);
        }
        add("idle", Duration::from_millis(4));
        let frame = finish(Duration::from_millis(20), false);
        let network = frame.spans.iter().find(|s| s.name == "network").unwrap();
        assert_eq!(network.parent, Some("update"));
        assert_eq!(network.count, 3);
        assert!(network.time >= Duration::from_millis(3));
        assert!(
            frame.top("update") >= network.time,
            "a parent covers its children"
        );
        assert_eq!(
            frame.top("network"),
            Duration::ZERO,
            "children are not top level"
        );
        assert_eq!(frame.top("idle"), Duration::from_millis(4));
        assert!(frame.traced() >= Duration::from_millis(9));
        assert!(
            finish(Duration::ZERO, false).spans.is_empty(),
            "each frame starts empty"
        );
    }

    #[test]
    fn notes_are_kept_once_and_bounded() {
        reset();
        for _ in 0..3 {
            note("graphics rebuilt");
        }
        for i in 0..20 {
            note(format!("event {i}"));
        }
        let frame = finish(Duration::ZERO, true);
        assert!(frame.background);
        assert_eq!(frame.notes.len(), MAX_NOTES + 1);
        assert_eq!(frame.notes[0], "graphics rebuilt");
        assert_eq!(frame.notes.last().unwrap(), "13 more events");
    }

    #[test]
    fn a_description_puts_the_longest_work_first() {
        let frame = FrameRecord {
            total: Duration::from_millis(400),
            background: false,
            spans: vec![
                SpanTime {
                    name: "update",
                    parent: None,
                    time: Duration::from_millis(3),
                    count: 1,
                },
                SpanTime {
                    name: "world upload",
                    parent: Some("draw"),
                    time: Duration::from_millis(350),
                    count: 1,
                },
                SpanTime {
                    name: "chunk uploads",
                    parent: Some("draw"),
                    time: Duration::from_millis(20),
                    count: 1,
                },
                SpanTime {
                    name: "draw",
                    parent: None,
                    time: Duration::from_millis(380),
                    count: 1,
                },
                SpanTime {
                    name: "tiny",
                    parent: None,
                    time: Duration::from_micros(10),
                    count: 1,
                },
            ],
            notes: vec!["graphics rebuilt".into()],
        };
        assert_eq!(
            frame.describe(),
            "draw 380.0 ms (world upload 350.0, chunk uploads 20.0), update 3.0 ms, \
             untraced 17.0 ms; events: graphics rebuilt"
        );
    }
}
