//! Recorded input. A recording is what the platform layer delivered, frame
//! by frame: the real elapsed time of each tick and every input event that
//! arrived before it. Replaying one drives the real [`PlatformApp`] exactly
//! as the window did, without a window, GPU or OS input, so menus, binds and
//! gameplay input are tested the way they are played.
//!
//! Record a session with `BRI_RECORD_INPUT=<file.jsonl> bri-client --run`.
//! Tests can also author frames directly ([`Script`]) from control names.
use crate::platform::PlatformApp;
use anyhow::{Context, Result, ensure};
use bri_ui::input::{InputEvent, MouseButton};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::Path,
    time::Duration,
};

/// One platform tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Elapsed time the platform passed to this tick, in microseconds.
    pub dt_us: u32,
    /// Input delivered since the previous tick, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<InputEvent>,
}
impl Frame {
    pub fn wait(dt: Duration) -> Self {
        Self {
            dt_us: dt.as_micros().min(u32::MAX as u128) as u32,
            events: Vec::new(),
        }
    }
}

/// Recordings larger than this are refused on load.
const MAX_RECORDING: u64 = 256 * 1024 * 1024;

/// Appends frames to a JSON-lines file as the game runs.
pub struct Recorder {
    out: BufWriter<File>,
    pending: Vec<InputEvent>,
}
impl Recorder {
    pub fn create(path: &Path) -> Result<Self> {
        let file = File::create(path)
            .with_context(|| format!("Could not create input recording {}", path.display()))?;
        Ok(Self {
            out: BufWriter::new(file),
            pending: Vec::new(),
        })
    }
    pub fn event(&mut self, event: InputEvent) {
        self.pending.push(event);
    }
    /// Close the current frame with the tick's elapsed time.
    pub fn frame(&mut self, elapsed: Duration) -> Result<()> {
        let frame = Frame {
            events: std::mem::take(&mut self.pending),
            ..Frame::wait(elapsed)
        };
        serde_json::to_writer(&mut self.out, &frame)?;
        self.out.write_all(b"\n")?;
        // A crash must not lose the input that led to it.
        self.out.flush()?;
        Ok(())
    }
}

pub fn load(path: &Path) -> Result<Vec<Frame>> {
    let file = File::open(path)
        .with_context(|| format!("Could not open input recording {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= MAX_RECORDING,
        "Input recording is too large"
    );
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter(|(_, line)| line.as_ref().map_or(true, |l| !l.trim().is_empty()))
        .map(|(n, line)| {
            serde_json::from_str(&line?).with_context(|| format!("Input recording line {}", n + 1))
        })
        .collect()
}

pub fn save(path: &Path, frames: &[Frame]) -> Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    for frame in frames {
        serde_json::to_writer(&mut out, frame)?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}

/// Replay `frames` into `app` exactly as the platform delivers them: input,
/// then the UI clock, the game tick and pending window commands. With
/// `realtime`, each frame also waits its recorded time so background work
/// (loading, the network worker) sees the same wall clock it did live.
/// `after` runs after every frame and may stop the replay early.
pub fn replay<A: PlatformApp + ?Sized>(
    app: &mut A,
    frames: &[Frame],
    realtime: bool,
    mut after: impl FnMut(&mut A, usize) -> Result<bool>,
) -> Result<()> {
    for (index, frame) in frames.iter().enumerate() {
        for event in &frame.events {
            app.ui_mut().handle_input(*event);
        }
        let elapsed = Duration::from_micros(u64::from(frame.dt_us));
        if realtime {
            std::thread::sleep(elapsed);
        }
        app.ui_mut().update(elapsed.as_millis().min(250) as u64);
        app.tick(elapsed)?;
        app.pump()?;
        if !after(app, index)? {
            break;
        }
    }
    Ok(())
}

/// Builds input the way a player makes it, addressed by control names so a
/// script survives layout changes.
#[derive(Default)]
pub struct Script {
    pub frames: Vec<Frame>,
}
impl Script {
    pub const TICK: Duration = Duration::from_micros(16_667);
    fn push(&mut self, events: Vec<InputEvent>) {
        self.frames.push(Frame {
            events,
            ..Frame::wait(Self::TICK)
        });
    }
    /// Let `duration` of ticks pass with no input.
    pub fn wait(&mut self, duration: Duration) -> &mut Self {
        let ticks = (duration.as_secs_f64() / Self::TICK.as_secs_f64()).ceil() as usize;
        for _ in 0..ticks.max(1) {
            self.push(Vec::new());
        }
        self
    }
    /// Move to `(x, y)` and click the left button (press and release on
    /// separate ticks, as a hand does).
    pub fn click(&mut self, (x, y): (f32, f32)) -> &mut Self {
        self.push(vec![
            InputEvent::MouseMove { x, y },
            InputEvent::MouseDown {
                button: MouseButton::Left,
                x,
                y,
            },
        ]);
        self.push(vec![InputEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        }]);
        self
    }
    /// Hold `key` for `duration`.
    pub fn hold(&mut self, key: bri_ui::input::Key, duration: Duration) -> &mut Self {
        let mods = bri_ui::input::Modifiers::NONE;
        self.push(vec![InputEvent::KeyDown {
            key,
            mods,
            repeat: false,
        }]);
        self.wait(duration);
        self.push(vec![InputEvent::KeyUp { key, mods }]);
        self
    }
    pub fn press(&mut self, key: bri_ui::input::Key) -> &mut Self {
        self.hold(key, Self::TICK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_ui::input::Key;

    #[test]
    fn recordings_round_trip_through_json_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.jsonl");
        let mut recorder = Recorder::create(&path).unwrap();
        recorder.event(InputEvent::MouseMove { x: 3.0, y: 4.0 });
        recorder.frame(Duration::from_millis(16)).unwrap();
        recorder.frame(Duration::from_millis(17)).unwrap();
        recorder.event(InputEvent::Char('é'));
        recorder.frame(Duration::from_millis(15)).unwrap();
        drop(recorder);
        let frames = load(&path).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].events, vec![InputEvent::MouseMove { x: 3.0, y: 4.0 }]);
        assert!(frames[1].events.is_empty());
        assert_eq!(frames[2].dt_us, 15_000);

        let mut script = Script::default();
        script
            .click((10.0, 20.0))
            .hold(Key::Letter('w'), Duration::from_millis(100));
        save(&path, &script.frames).unwrap();
        assert_eq!(load(&path).unwrap(), script.frames);
    }
}
