//! The v20 Tutorial played through the real App, the way a new player does
//! it: the first-run offer's Play Tutorial button, then each lesson in order
//! with only the player's own key binds, mouse movement and clicks on real
//! screens. No window or OS input is created; the local host runs as it does
//! in a game. A lesson that cannot be completed fails the test with where
//! the player was and what the Tutorial last said.
//!
//! Covered: Look, Move, Jump, Duck, Bricks, Build, Break, Jet, Light, Ride,
//! Dismount, Wrench, Print, Diving and Shooting (the target practice, which
//! must hit most of its targets). Still to add: Drive, Spray, the wand room,
//! the finish and the optional Secrets.
//!
//! Run: cargo test -p bri-client --test tutorial_walkthrough -- --ignored --nocapture
//! `BRI_TUTORIAL_UNTIL=<goal>` stops after that goal while working on a
//! lesson; `BRI_TUTORIAL_SHOT=1` also saves an offscreen frame of the target
//! range (needs a GPU adapter).
use anyhow::{Context, Result, bail, ensure};
use bri_client::{
    app::App,
    platform::PlatformApp,
    playback::{self, Frame},
};
use bri_console::Clamp;
use bri_ui::{
    api::{BindInput, ConnectionState},
    input::{InputEvent, Key, Modifiers, MouseButton},
    screens::ScreenId,
};
use glam::{Vec2, Vec3};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const SIZE: (u32, u32) = (960, 720);
/// Real time between frames: the host runs on its own clock.
const FRAME: Duration = Duration::from_millis(8);
const TICKS: f32 = 120.0;

/// One player at the keyboard and mouse of a real App.
struct Player {
    app: Box<App>,
    last: Instant,
    /// Input for the next frame.
    input: Vec<InputEvent>,
    /// Everything delivered, frame by frame, kept as a replayable recording.
    recorded: Vec<Frame>,
    /// Yaw and pitch per unit of mouse motion, measured.
    gain: (f32, f32),
    /// Goals the Tutorial announced, in order.
    goals: Vec<String>,
    bottom: Option<String>,
    center: Option<String>,
    /// What the Tutorial last said, newest last.
    said: VecDeque<String>,
    artifact: PathBuf,
}

impl Player {
    fn frame(&mut self) -> Result<()> {
        let events = std::mem::take(&mut self.input);
        for event in &events {
            self.app.ui_mut().handle_input(*event);
        }
        std::thread::sleep(FRAME);
        let now = Instant::now();
        let dt = now - self.last;
        self.last = now;
        self.app.ui_mut().update(dt.as_millis().min(250) as u64);
        self.app.tick(dt)?;
        self.app.pump()?;
        self.recorded.push(Frame {
            dt_us: dt.as_micros().min(u128::from(u32::MAX)) as u32,
            events,
        });
        if let ConnectionState::Failed { reason } = &self.app.ui.core.conn {
            bail!("Connection failed: {reason}");
        }
        self.listen();
        Ok(())
    }

    /// Read the Tutorial's center and bottom prints as a player does.
    fn listen(&mut self) {
        let center = self
            .app
            .ui
            .core
            .center_print
            .as_ref()
            .map(|(t, _)| t.clone());
        if center != self.center {
            if let Some(text) = center.as_ref().filter(|t| !t.is_empty()) {
                self.note(format!("center: {}", plain(text)));
            }
            self.center = center;
        }
        let bottom = self
            .app
            .ui
            .core
            .bottom_print
            .as_ref()
            .map(|(t, _, _)| t.clone());
        if bottom != self.bottom {
            if let Some(text) = &bottom {
                let text = plain(text);
                if let Some(goal) = text
                    .split("Goal Completed! - ")
                    .nth(1)
                    .and_then(|rest| rest.split(" - ").next())
                {
                    self.goals.push(goal.trim().to_string());
                }
                self.note(format!("bottom: {text}"));
            }
            self.bottom = bottom;
        }
    }

    fn note(&mut self, line: String) {
        let at = self.app.network_view().map_or(0, |v| v.tick);
        eprintln!("[{:7.2}] {line}", at as f32 / TICKS);
        if self.said.len() == 12 {
            self.said.pop_front();
        }
        self.said.push_back(line);
    }

    /// Where the player is and what they were last told, for a failure.
    fn situation(&self) -> String {
        let me = self.app.presented_local();
        let ghost = self
            .app
            .building()
            .and_then(|b| b.ghost())
            .map(|g| g.position);
        format!(
            "feet {:?}, view {:?}; ghost {ghost:?}; screens {:?}; goals {:?}; last said {:?}",
            me.map(|p| p.feet),
            self.angles(),
            self.app.ui.stack(),
            self.goals,
            self.said
        )
    }

    fn feet(&self) -> Result<Vec3> {
        Ok(Vec3::from(
            self.app.presented_local().context("No local player")?.feet,
        ))
    }

    fn server_seconds(&self) -> f32 {
        self.app
            .network_view()
            .map_or(0.0, |v| v.tick as f32 / TICKS)
    }

    /// Play frames until `done`, failing after `seconds` of game time.
    fn until(&mut self, what: &str, seconds: f32, done: impl Fn(&Player) -> bool) -> Result<()> {
        let start = self.server_seconds();
        let wall = Instant::now();
        while !done(self) {
            ensure!(
                self.server_seconds() - start < seconds
                    && wall.elapsed() < Duration::from_secs_f32(seconds * 4.0 + 30.0),
                "Timed out waiting for {what}: {}",
                self.situation()
            );
            self.frame()?;
        }
        Ok(())
    }

    fn wait(&mut self, seconds: f32) -> Result<()> {
        let start = self.server_seconds();
        let wall = Instant::now();
        while self.server_seconds() - start < seconds
            && wall.elapsed() < Duration::from_secs_f32(seconds * 4.0 + 5.0)
        {
            self.frame()?;
        }
        Ok(())
    }

    fn goal(&mut self, goal: &str, seconds: f32) -> Result<()> {
        self.until(&format!("the {goal} goal"), seconds, |p| {
            p.goals.iter().any(|g| g == goal)
        })?;
        if std::env::var("BRI_TUTORIAL_UNTIL").is_ok_and(|g| g.eq_ignore_ascii_case(goal)) {
            self.save()?;
            bail!("Stopped after {goal} (BRI_TUTORIAL_UNTIL)");
        }
        Ok(())
    }

    // ---------------------------------------------------------------- input

    fn binding(&self, command: &str) -> Result<BindInput> {
        self.app
            .ui
            .core
            .binds
            .binding_of(command)
            .with_context(|| format!("Nothing is bound to {command}"))
    }

    /// Press or release whatever the player's binds put `command` on.
    fn set(&mut self, command: &str, down: bool) -> Result<()> {
        let (x, y) = (SIZE.0 as f32 / 2.0, SIZE.1 as f32 / 2.0);
        let event = match self.binding(command)? {
            BindInput::Key(chord) if down => InputEvent::KeyDown {
                key: chord.key,
                mods: chord.mods,
                repeat: false,
            },
            BindInput::Key(chord) => InputEvent::KeyUp {
                key: chord.key,
                mods: chord.mods,
            },
            BindInput::Mouse(button) if down => InputEvent::MouseDown { button, x, y },
            BindInput::Mouse(button) => InputEvent::MouseUp { button, x, y },
            other => bail!("{command} is bound to {other:?}"),
        };
        self.input.push(event);
        Ok(())
    }

    /// A press as long as a finger makes one.
    fn press(&mut self, command: &str) -> Result<()> {
        self.set(command, true)?;
        self.wait(0.08)?;
        self.set(command, false)?;
        self.frame()
    }

    fn key(&mut self, key: Key) -> Result<()> {
        let mods = Modifiers::NONE;
        self.input.push(InputEvent::KeyDown {
            key,
            mods,
            repeat: false,
        });
        self.frame()?;
        self.input.push(InputEvent::KeyUp { key, mods });
        self.frame()
    }

    fn click(&mut self, screen: ScreenId, control: &str) -> Result<()> {
        let (x, y) = self
            .app
            .ui
            .control_center(screen, control)
            .with_context(|| format!("{screen:?} shows no {control}: {}", self.situation()))?;
        self.input.push(InputEvent::MouseMove { x, y });
        self.input.push(InputEvent::MouseDown {
            button: MouseButton::Left,
            x,
            y,
        });
        self.frame()?;
        self.input.push(InputEvent::MouseUp {
            button: MouseButton::Left,
            x,
            y,
        });
        self.frame()
    }

    /// The brick selector's index of a brick by its name.
    fn brick_index(&self, ui_name: &str) -> Result<usize> {
        self.app
            .ui
            .core
            .bricks
            .iter()
            .position(|b| b.ui_name.eq_ignore_ascii_case(ui_name))
            .with_context(|| format!("No {ui_name} brick"))
    }

    fn type_text(&mut self, text: &str) -> Result<()> {
        for c in text.chars() {
            self.input.push(InputEvent::Char(c));
            self.frame()?;
        }
        Ok(())
    }

    // ----------------------------------------------------------------- look

    fn angles(&self) -> (f32, f32) {
        self.app.controls.view_angles()
    }

    /// Learn how far the view turns per unit of mouse motion.
    fn calibrate(&mut self) -> Result<()> {
        let (yaw, pitch) = self.angles();
        self.input
            .push(InputEvent::MouseDelta { dx: 40.0, dy: 20.0 });
        self.frame()?;
        let (yaw2, pitch2) = self.angles();
        let gain = (wrap(yaw2 - yaw) / 40.0, (pitch2 - pitch) / 20.0);
        ensure!(
            gain.0.abs() > 1e-5 && gain.1.abs() > 1e-5,
            "The mouse does not turn the view: {gain:?}"
        );
        self.gain = gain;
        Ok(())
    }

    /// Move the mouse toward a view, at most `max` radians this frame.
    fn steer(&mut self, yaw: f32, pitch: f32, max: f32) {
        let (y, p) = self.angles();
        let dy = wrap(yaw - y).clamped(-max, max);
        let dp = (pitch - p).clamped(-max, max);
        if dy.abs() > 1e-4 || dp.abs() > 1e-4 {
            self.input.push(InputEvent::MouseDelta {
                dx: dy / self.gain.0,
                dy: dp / self.gain.1,
            });
        }
    }

    /// Turn to a view and settle there: the player's body, which aims
    /// clicks and shots, follows the view a frame or two later.
    fn face(&mut self, yaw: f32, pitch: f32) -> Result<()> {
        for _ in 0..240 {
            let (y, p) = self.angles();
            let body = self
                .app
                .presented_local()
                .is_some_and(|me| wrap(me.yaw - y).abs() < 0.005 && (me.pitch - p).abs() < 0.005);
            if wrap(yaw - y).abs() < 0.01 && (pitch - p).abs() < 0.01 && body {
                return Ok(());
            }
            self.steer(yaw, pitch, 0.6);
            self.frame()?;
        }
        bail!("Could not turn to {yaw}, {pitch}: {}", self.situation())
    }

    /// The view that looks from the eye at `point`.
    fn aim(&self, point: Vec3) -> Result<(f32, f32)> {
        let me = self.app.presented_local().context("No local player")?;
        let eye = Vec3::from(me.feet) + Vec3::Y * if me.crouched { 0.63 } else { 2.16 };
        Ok(view_toward(point - eye))
    }

    /// Face a heading, level.
    fn settle_view(&mut self, yaw: f32) -> Result<()> {
        self.face(yaw, 0.0)
    }

    fn look_at(&mut self, point: Vec3) -> Result<()> {
        let (yaw, pitch) = self.aim(point)?;
        self.face(yaw, pitch)
    }

    // --------------------------------------------------------------- moving

    /// Walk to `to` (x, z) by steering the view with the mouse and holding
    /// forward. Fails if the player stops making progress.
    fn walk_to(&mut self, to: Vec2, within: f32) -> Result<()> {
        self.walk(&[to], within, |_| Ok(()))
    }

    /// Walk a route of (x, z) points; `each` runs every frame (to jump,
    /// crouch or look around on the way).
    fn walk(
        &mut self,
        route: &[Vec2],
        within: f32,
        mut each: impl FnMut(&mut Player) -> Result<()>,
    ) -> Result<()> {
        let pitch = self.angles().1;
        for (i, to) in route.iter().enumerate() {
            let last = i + 1 == route.len();
            let reach = if last { within } else { within.min(0.35) };
            let mut best = f32::INFINITY;
            let mut since = self.server_seconds();
            let mut forward = false;
            loop {
                let feet = self.feet()?;
                let d = *to - Vec2::new(feet.x, feet.z);
                let distance = d.length();
                if distance < reach {
                    break;
                }
                if distance < best - 0.2 {
                    best = distance;
                    since = self.server_seconds();
                }
                ensure!(
                    self.server_seconds() - since < 3.0,
                    "Stuck walking to {to} ({distance:.2} away): {}",
                    self.situation()
                );
                let yaw = d.x.atan2(-d.y);
                self.steer(yaw, pitch.clamped(-0.3, 0.0), 0.35);
                let aligned = wrap(yaw - self.angles().0).abs() < 0.6;
                if aligned != forward {
                    self.set("moveforward", aligned)?;
                    forward = aligned;
                }
                each(self)?;
                self.frame()?;
            }
            if forward && last {
                self.set("moveforward", false)?;
                self.frame()?;
                self.settle()?;
            } else if forward {
                // Keep running into the next leg.
                self.set("moveforward", false)?;
            }
        }
        Ok(())
    }

    /// Let the player come to a stop.
    fn settle(&mut self) -> Result<()> {
        self.until("the player to stop", 3.0, |p| {
            p.app
                .presented_local()
                .is_some_and(|me| Vec2::new(me.velocity[0], me.velocity[2]).length() < 0.05)
        })
    }

    fn save(&self) -> Result<()> {
        playback::save(&self.artifact.join("tutorial-input.jsonl"), &self.recorded)
    }
}

/// Yaw 0 faces -z and grows toward +x; pitch grows upward.
fn view_toward(d: Vec3) -> (f32, f32) {
    let flat = Vec2::new(d.x, d.z).length();
    (d.x.atan2(-d.z), d.y.atan2(flat))
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Print text without colour escapes (`\u{E000}`-`\u{E00F}`).
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !('\u{E000}'..='\u{E00F}').contains(c))
        .collect::<String>()
        .replace('\n', " / ")
}

fn start(workspace: &Path) -> Result<Player> {
    let artifact = workspace.join("artifacts/tutorial-walkthrough");
    std::fs::create_dir_all(&artifact)?;
    let state = tempfile::tempdir()?;
    let app = App::load(&workspace.join("content"), state.path(), SIZE)?;
    // The profile lives as long as the run.
    std::mem::forget(state);
    let mut p = Player {
        app,
        last: Instant::now(),
        input: Vec::new(),
        recorded: Vec::new(),
        gain: (0.0, 0.0),
        goals: Vec::new(),
        bottom: None,
        center: None,
        said: VecDeque::new(),
        artifact,
    };
    // First launch: accept the default controls, then play the Tutorial the
    // welcome offers.
    let wall = Instant::now();
    while !p.app.ui.is_open(ScreenId::MainMenu) {
        ensure!(wall.elapsed() < Duration::from_secs(30), "No main menu");
        p.frame()?;
    }
    if p.app.ui.top_id() == ScreenId::DefaultControls {
        p.click(ScreenId::DefaultControls, "DefaultControlsGui.apply();")?;
        p.frame()?;
    }
    ensure!(
        p.app.ui.top_id() == ScreenId::MessageBox,
        "The first run did not offer the Tutorial: {:?}",
        p.app.ui.stack()
    );
    p.click(ScreenId::MessageBox, "Play Tutorial")?;
    let wall = Instant::now();
    while !(matches!(p.app.ui.core.conn, ConnectionState::InGame { .. })
        && p.app.presented_local().is_some())
    {
        ensure!(
            wall.elapsed() < Duration::from_secs(180),
            "The Tutorial did not load: {:?}",
            p.app.ui.core.conn
        );
        p.frame()?;
    }
    ensure!(
        p.app
            .scene_map()
            .is_some_and(|m| m.eq_ignore_ascii_case(bri_sim::tutorial::MAP_ID)),
        "Play Tutorial started {:?}",
        p.app.scene_map()
    );
    Ok(p)
}

// ---------------------------------------------------------------- lessons

/// Mission positions (native axes: x, up, -Torque y).
const LOOK_TARGET: Vec3 = Vec3::new(-9.29179, 104.136, 97.7424);

fn look_and_move(p: &mut Player) -> Result<()> {
    p.until("the first prompt", 20.0, |p| {
        p.said.iter().any(|s| s.contains("to look around"))
    })?;
    p.calibrate()?;
    // "Move the mouse to look around": face the sign behind the spawn.
    p.look_at(LOOK_TARGET)?;
    p.goal("Look", 5.0)?;
    // Walk off the spawn pillar into the first room.
    p.face(p.angles().0, 0.0)?;
    p.walk_to(Vec2::new(-8.5, 95.5), 0.5)?;
    p.goal("Move", 5.0)
}

fn jump(p: &mut Player) -> Result<()> {
    // Through the door into the jump room; the jump tip there gives jumping.
    p.walk_to(Vec2::new(-8.5, 97.2), 0.4)?;
    p.walk_to(Vec2::new(-8.5, 99.4), 0.3)?;
    p.wait(0.3)?;
    // A running jump across the pit.
    let mut jumped = false;
    p.walk(&[Vec2::new(-8.5, 110.5)], 0.6, |p| {
        if !jumped && p.feet()?.z > 100.1 {
            jumped = true;
            p.set("jump", true)?;
        }
        Ok(())
    })?;
    p.set("jump", false)?;
    p.goal("Jump", 5.0)
}

fn duck(p: &mut Player) -> Result<()> {
    // East through the side door into the crawlway room.
    p.walk(
        &[
            Vec2::new(-3.0, 109.9),
            Vec2::new(1.5, 109.9),
            Vec2::new(6.5, 104.5),
        ],
        0.4,
        |_| Ok(()),
    )?;
    p.set("crouch", true)?;
    p.walk_to(Vec2::new(6.5, 99.8), 0.3)?;
    p.goal("Duck", 8.0)?;
    p.set("crouch", false)?;
    p.frame()
}

fn bricks(p: &mut Player) -> Result<()> {
    // South into the brick room.
    p.walk(&[Vec2::new(6.5, 99.6), Vec2::new(6.5, 96.0)], 0.4, |_| {
        Ok(())
    })?;
    // "Press B to get bricks": open the brick selector, take a brick and
    // buy it into the inventory.
    p.press("openBSD")?;
    p.until("the brick selector", 5.0, |p| {
        p.app.ui.top_id() == ScreenId::BrickSelector
    })?;
    // Add-Ons can extend the first section beyond the viewport. Search
    // through the real field so the desired tile is visible before clicking.
    p.click(ScreenId::BrickSelector, "BSD_Search")?;
    p.type_text("2x2")?;
    let brick = p.brick_index("2x2")?;
    let icon = format!("BSD_Result{brick}");
    // A double click puts it in the cart; the Buy button purchases it.
    p.click(ScreenId::BrickSelector, &icon)?;
    p.click(ScreenId::BrickSelector, &icon)?;
    p.click(ScreenId::BrickSelector, "BSD_BuyBricks();")?;
    p.until("the brick selector to close", 5.0, |p| {
        !p.app.ui.is_open(ScreenId::BrickSelector)
    })?;
    p.goal("Bricks", 5.0)
}

fn equipped(p: &Player) -> bri_client::building::Equipment {
    p.app
        .building()
        .map_or(bri_client::building::Equipment::None, |b| {
            b.equipment().clone()
        })
}

fn brick_count(p: &Player) -> usize {
    p.app.network_view().map_or(0, |v| v.world.bricks.len())
}

/// Aim at `spot`, click to put the ghost brick there and plant `count`
/// bricks up from it, shifting the ghost up a brick between plants.
fn plant_column(p: &mut Player, spot: Vec3, count: usize) -> Result<()> {
    p.look_at(spot)?;
    let feet = p.feet()?;
    p.note(format!("column of {count} at {spot} from {feet}"));
    p.press("mouseFire")?;
    p.until("the ghost brick", 3.0, |p| {
        p.app.building().and_then(|b| b.ghost()).is_some_and(|g| {
            (g.position[0] - spot.x).abs() < 0.3 && (g.position[2] - spot.z).abs() < 0.3
        })
    })?;
    for i in 0..count {
        if i > 0 {
            p.press("shiftBrickUp")?;
        }
        let before = brick_count(p);
        p.press("plantBrick")?;
        p.until("the brick to plant", 3.0, |p| brick_count(p) > before)?;
    }
    Ok(())
}

fn build(p: &mut Player) -> Result<()> {
    // Through the brick door (gone now) into the build room.
    p.walk(&[Vec2::new(10.0, 91.0), Vec2::new(17.0, 91.0)], 0.4, |_| {
        Ok(())
    })?;
    // "Press ... to equip bricks", then build a staircase up to the hole
    // high in the north wall: seven columns of 2x2 bricks, 0.6 a step.
    p.press("useBricks")?;
    p.until("bricks in hand", 3.0, |p| {
        matches!(equipped(p), bri_client::building::Equipment::Brick(_))
    })?;
    for (i, z) in [97.5, 96.5, 95.5, 94.5, 93.5, 92.5, 91.5]
        .into_iter()
        .enumerate()
    {
        p.walk_to(Vec2::new(19.6, z), 0.25)?;
        plant_column(p, Vec3::new(21.5, 94.4, z), 7 - i)?;
    }
    // Up the stairs and through the hole.
    p.walk(&[Vec2::new(21.5, 89.5), Vec2::new(21.5, 99.8)], 0.4, |_| {
        Ok(())
    })?;
    p.goal("Build", 10.0)
}

/// Where a tool or weapon sits on its spawn brick.
fn item_spawn(p: &Player, item: &str) -> Result<Vec3> {
    p.app
        .network_view()
        .and_then(|v| {
            v.weapons
                .static_items
                .iter()
                .find(|i| i.item.eq_ignore_ascii_case(item))
                .map(|i| Vec3::from(i.position))
        })
        .with_context(|| format!("No {item} to pick up: {}", p.situation()))
}

fn has_item(p: &Player, item: &str) -> bool {
    p.app.network_view().is_some_and(|v| {
        v.tools.get(&v.owner).is_some_and(|t| {
            t.slots
                .iter()
                .flatten()
                .any(|s| s.eq_ignore_ascii_case(item))
        })
    })
}

/// Run over an item to pick it up.
fn pick_up(p: &mut Player, item: &str) -> Result<()> {
    let at = item_spawn(p, item)?;
    p.walk_to(Vec2::new(at.x, at.z), 0.3)?;
    p.until(&format!("{item} in the inventory"), 3.0, |p| {
        has_item(p, item)
    })
}

/// "Press Q to equip": the tools key takes out a tool (or puts tools away
/// again), the mouse wheel moves to the next one.
fn take_out(p: &mut Player, what: bri_client::building::Equipment) -> Result<()> {
    use bri_client::building::Equipment;
    for _ in 0..10 {
        let now = equipped(p);
        if now == what {
            return Ok(());
        }
        if matches!(now, Equipment::None | Equipment::Brick(_)) {
            p.press("useTools")?;
        } else {
            p.input.push(InputEvent::Wheel { delta: -1.0 });
            p.frame()?;
        }
        p.wait(0.2)?;
    }
    bail!("Could not take out {what:?}: {}", p.situation())
}

/// Bricks of the world whose centre lies in a column around (x, z).
fn column(p: &Player, x: f32, z: f32) -> Vec<Vec3> {
    p.app.network_view().map_or_else(Vec::new, |v| {
        let mut bricks: Vec<Vec3> = v
            .world
            .bricks
            .values()
            .map(|b| Vec3::from(b.position))
            .filter(|b| (b.x - x).abs() < 0.3 && (b.z - z).abs() < 0.3)
            .collect();
        bricks.sort_by(|a, b| b.y.total_cmp(&a.y));
        bricks
    })
}

/// Break a stack from the top until only bricks below `floor` are left,
/// hitting each top brick on its face toward the player from `stand`.
fn break_down(p: &mut Player, stand: Vec2, x: f32, z: f32, face: f32, floor: f32) -> Result<()> {
    for _ in 0..30 {
        let Some(top) = column(p, x, z).first().copied().filter(|b| b.y > floor) else {
            return Ok(());
        };
        let feet = p.feet()?;
        if Vec2::new(feet.x, feet.z).distance(stand) > 0.3 {
            p.walk_to(stand, 0.2)?;
        }
        p.look_at(Vec3::new(top.x, top.y, face))?;
        p.note(format!("hammering {top}"));
        p.press("mouseFire")?;
        // The hammer swings about three times a second.
        let _ = p.until("the brick to break", 0.8, |p| {
            column(p, x, z).first().is_none_or(|b| b.y < top.y - 0.1)
        });
        p.wait(0.2)?;
    }
    bail!(
        "The stack at {x}, {z} would not come down: {}",
        p.situation()
    )
}

fn hammer(p: &mut Player) -> Result<()> {
    // "Run over the hammer to pick it up", then take it out.
    pick_up(p, "v20.weapon.hammeritem")?;
    take_out(p, bri_client::building::Equipment::Tool(bri_weapons::HostTool::Break))?;
    // The hammer only breaks the top of a stack: take down both stacks in
    // the doorway (its lintel leaves no room to stand on a brick).
    let stand = Vec2::new(21.5, 110.6);
    p.walk_to(stand, 0.25)?;
    for x in [20.5, 22.5] {
        break_down(p, stand, x, 112.5, 112.0, 94.4)?;
    }
    p.walk(
        &[Vec2::new(21.5, 111.5), Vec2::new(21.5, 114.8)],
        0.4,
        |_| Ok(()),
    )?;
    p.goal("Break", 8.0)
}

/// Floor of the tunnel above the jet room, which leads west over the lit
/// room.
const TUNNEL_FLOOR: f32 = 111.4;

fn jet(p: &mut Player) -> Result<()> {
    // Onto the jet pad, where the tip gives back the jets, and up the
    // shaft above it to the tunnel.
    p.walk(
        &[Vec2::new(21.5, 117.0), Vec2::new(21.6, 121.0)],
        0.3,
        |_| Ok(()),
    )?;
    p.wait(0.3)?;
    p.face(std::f32::consts::FRAC_PI_2 * 3.0, 0.0)?;
    p.set("jet", true)?;
    p.until("the top of the jet shaft", 15.0, |p| {
        p.feet().is_ok_and(|f| f.y > TUNNEL_FLOOR + 0.6)
    })?;
    // Over the tunnel floor, then walk west along it through the goal.
    p.set("moveforward", true)?;
    p.until("the tunnel", 5.0, |p| p.feet().is_ok_and(|f| f.x < 18.8))?;
    p.set("moveforward", false)?;
    p.set("jet", false)?;
    p.frame()?;
    p.walk_to(Vec2::new(12.0, 121.0), 0.5)?;
    p.goal("Jet", 5.0)
}

fn light(p: &mut Player) -> Result<()> {
    // West along the tunnel to its hole and down into the dark room.
    p.walk_to(Vec2::new(6.6, 121.0), 0.4)?;
    p.until("the drop into the dark room", 5.0, |p| {
        p.feet().is_ok_and(|f| f.y < 95.0)
    })?;
    p.settle()?;
    // "Press L to turn on your light", then out by the west door.
    p.press("useLight")?;
    p.until("the light", 3.0, |p| {
        p.app
            .network_view()
            .and_then(|v| v.vitals.get(&v.owner))
            .is_some_and(|v| v.light)
    })?;
    // Through the one gap in the partition, then out by the west door.
    p.walk(
        &[
            Vec2::new(4.2, 116.0),
            Vec2::new(1.0, 116.0),
            Vec2::new(1.0, 127.0),
            Vec2::new(-1.8, 127.0),
        ],
        0.2,
        |_| Ok(()),
    )?;
    p.goal("Light", 8.0)
}

fn vehicle_at(p: &Player, definition: &str) -> Option<(u64, Vec3)> {
    let view = p.app.network_view()?;
    let info = view
        .vehicles
        .values()
        .find(|v| v.definition == definition && !v.destroyed)?;
    let pose = view.vehicle_poses.get(&info.id)?;
    Some((info.id, Vec3::from(pose.position)))
}

fn mounted(p: &Player) -> Option<u64> {
    let view = p.app.network_view()?;
    view.vitals.get(&view.owner)?.mounted.map(|(id, _)| id)
}

/// Wrench a vehicle spawn brick and pick a vehicle in its dialog.
fn wrench_vehicle(p: &mut Player, pad: Vec3, vehicle: &str) -> Result<()> {
    use bri_ui::api::WrenchVariant;
    let dialog = ScreenId::Wrench(WrenchVariant::VehicleSpawn);
    p.look_at(pad)?;
    p.press("mouseFire")?;
    p.until("the vehicle spawn's wrench dialog", 3.0, |p| {
        p.app.ui.top_id() == dialog
    })?;
    p.click(dialog, "WrenchVehicleSpawn_Vehicles")?;
    p.type_text(vehicle)?;
    p.key(Key::Return)?;
    p.click(dialog, "wrenchVehicleSpawnDlg.send();")?;
    p.until("the wrench dialog to close", 3.0, |p| {
        !p.app.ui.is_open(dialog)
    })
}

const HORSE: &str = "v20.vehicle.horsearmor";

fn ride(p: &mut Player) -> Result<()> {
    // "Run over the wrench to pick it up", take it out and put a horse on
    // the vehicle spawn.
    pick_up(p, "v20.weapon.wrenchitem")?;
    take_out(p, bri_client::building::Equipment::Tool(bri_weapons::HostTool::Inspect))?;
    p.walk_to(Vec2::new(-5.5, 118.0), 0.3)?;
    wrench_vehicle(p, Vec3::new(-5.5, 94.6, 121.0), "Horse")?;
    p.until("the horse", 5.0, |p| vehicle_at(p, HORSE).is_some())?;
    // "Jump on top of the horse": run at it and jump.
    let (horse, at) = vehicle_at(p, HORSE).context("No horse")?;
    let mut jumped = false;
    p.walk(&[Vec2::new(at.x, at.z)], 0.3, |p| {
        let feet = p.feet()?;
        if !jumped && Vec2::new(feet.x - at.x, feet.z - at.z).length() < 1.8 {
            jumped = true;
            p.set("jump", true)?;
        }
        Ok(())
    })
    .or_else(|error| {
        if mounted(p).is_some() {
            Ok(())
        } else {
            Err(error)
        }
    })?;
    p.set("jump", false)?;
    p.until("riding the horse", 3.0, |p| mounted(p) == Some(horse))?;
    // West at a gallop and over the water.
    p.settle_view(-std::f32::consts::FRAC_PI_2)?;
    let mut jumped = false;
    p.walk(&[Vec2::new(-34.0, 121.0)], 0.8, |p| {
        if !jumped && p.feet()?.x < -16.8 {
            jumped = true;
            p.set("jump", true)?;
        }
        Ok(())
    })?;
    p.set("jump", false)?;
    p.goal("Ride", 5.0)
}

fn dismount(p: &mut Player) -> Result<()> {
    // "Press right mouse button to dismount" in the next area.
    p.walk_to(Vec2::new(-40.0, 121.0), 0.8)?;
    p.press("jet")?;
    p.until("off the horse", 3.0, |p| mounted(p).is_none())?;
    p.goal("Dismount", 5.0)
}

/// Click a brick with the wrench and apply the first choice (after NONE)
/// of one of its dialog's menus.
fn wrench_brick(p: &mut Player, target: Vec3, menu: &str) -> Result<()> {
    use bri_ui::api::WrenchVariant;
    let dialog = ScreenId::Wrench(WrenchVariant::Normal);
    p.look_at(target)?;
    p.press("mouseFire")?;
    p.until("the wrench dialog", 3.0, |p| p.app.ui.top_id() == dialog)?;
    p.click(dialog, menu)?;
    p.key(Key::Down)?;
    p.key(Key::Return)?;
    p.click(dialog, "wrenchDlg.send();")?;
    p.until("the wrench dialog to close", 3.0, |p| {
        !p.app.ui.is_open(dialog)
    })
}

fn said(p: &Player, text: &str) -> bool {
    p.center.as_deref().is_some_and(|c| plain(c).contains(text))
}

fn wrench(p: &mut Player) -> Result<()> {
    // South through the doorway into the wrench room.
    p.walk(
        &[
            Vec2::new(-38.5, 115.5),
            Vec2::new(-38.5, 112.0),
            Vec2::new(-38.5, 109.0),
        ],
        0.2,
        |_| Ok(()),
    )?;
    take_out(p, bri_client::building::Equipment::Tool(bri_weapons::HostTool::Inspect))?;
    // Light, then an emitter, then an item on the cone, as each prompt asks.
    let cone = Vec3::new(-38.5, 95.8, 106.0);
    for (asks, menu) in [
        ("Apply a Light", "Wrench_Lights"),
        ("Apply an Emitter", "Wrench_Emitters"),
        ("Apply an Item", "Wrench_Items"),
    ] {
        p.until(&format!("\"{asks}\""), 8.0, |p| said(p, asks))?;
        wrench_brick(p, cone, menu)?;
    }
    p.goal("Wrench", 8.0)
}

fn named_brick(p: &Player, name: &str) -> Option<Vec3> {
    p.app.network_view().and_then(|v| {
        v.world
            .bricks
            .values()
            .find(|b| {
                b.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|b| Vec3::from(b.position))
    })
}

fn print(p: &mut Player) -> Result<()> {
    // Through the wrench room's east door (open now) to the printer.
    p.walk(
        &[Vec2::new(-34.0, 106.0), Vec2::new(-30.0, 106.0)],
        0.25,
        |_| Ok(()),
    )?;
    pick_up(p, "v20.weapon.printgun")?;
    take_out(p, bri_client::building::Equipment::Tool(bri_weapons::HostTool::Print))?;
    // Spell OINKMOO: shoot each print brick and press its letter.
    for (i, letter) in "oinkmoo".chars().enumerate() {
        let name = format!("_TutorialPrintBrick{}", i + 1);
        let brick = named_brick(p, &name).with_context(|| format!("No {name}"))?;
        // Bricks 1-4 line the east wall, 5-7 the north wall.
        let (stand, face) = if i < 4 {
            (
                Vec2::new(brick.x - 2.5, brick.z),
                Vec3::new(brick.x - 0.3, brick.y, brick.z),
            )
        } else {
            (
                Vec2::new(brick.x, brick.z - 2.5),
                Vec3::new(brick.x, brick.y, brick.z - 0.3),
            )
        };
        p.walk_to(stand, 0.25)?;
        p.look_at(face)?;
        p.press("mouseFire")?;
        p.until("the print selector", 3.0, |p| {
            p.app.ui.top_id() == ScreenId::PrintSelector
        })?;
        p.key(Key::Letter(letter))?;
        p.until("the print selector to close", 3.0, |p| {
            !p.app.ui.is_open(ScreenId::PrintSelector)
        })?;
    }
    p.goal("Print", 8.0)
}

fn diving(p: &mut Player) -> Result<()> {
    // Out through the print room's south door (open now) and into the
    // first pool.
    p.walk(
        &[Vec2::new(-23.5, 101.0), Vec2::new(-23.5, 96.0)],
        0.3,
        |_| Ok(()),
    )?;
    // "Press crouch to dive": into the pool, down to the tunnel under the
    // wall, west along it, then up in the second pool and onto its rim.
    p.set("moveforward", true)?;
    let (mut diving, mut rising) = (false, false);
    let start = p.server_seconds();
    while !p.goals.iter().any(|g| g == "Diving") {
        ensure!(
            p.server_seconds() - start < 25.0,
            "Never came up in the second pool: {}",
            p.situation()
        );
        let feet = p.feet()?;
        let dive = feet.z < 93.5 && feet.x > -37.5 && feet.y > 86.4;
        let rise = feet.x <= -37.5;
        let heading = if feet.z > 92.5 {
            view_toward(Vec3::new(-23.5, feet.y, 91.5) - feet).0
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        if dive != diving {
            p.set("crouch", dive)?;
            diving = dive;
        }
        if rise != rising {
            p.set("jump", rise)?;
            rising = rise;
        }
        p.steer(heading, 0.0, 0.2);
        p.frame()?;
    }
    p.set("crouch", false)?;
    p.set("jump", false)?;
    p.set("moveforward", false)?;
    p.frame()?;
    p.settle()
}

/// Where a target's board is: its model's collision box sits a unit and
/// a bit above its origin.
fn board(target: &bri_sim::tutorial::TargetView, tick: f64) -> Vec3 {
    target.position(tick) + Vec3::new(0.0, 1.64, 0.0)
}

fn shooting(p: &mut Player) -> Result<()> {
    // Through the doorway south of the pools to the gun on its brick.
    p.walk(
        &[Vec2::new(-38.5, 86.0), Vec2::new(-38.5, 81.0)],
        0.3,
        |_| Ok(()),
    )?;
    pick_up(p, "v20.weapon.gunitem")?;
    take_out(
        p,
        bri_client::building::Equipment::Weapon("v20.weapon.gunitem".into()),
    )
    .or_else(|_| {
        if matches!(equipped(p), bri_client::building::Equipment::Weapon(_)) {
            Ok(())
        } else {
            bail!("The gun is not out: {:?}", equipped(p))
        }
    })?;
    p.walk_to(Vec2::new(-38.5, 79.0), 0.3)?;
    p.face(0.0, 0.0)?;
    p.until("\"Prepare for Target Practice!\"", 5.0, |p| {
        said(p, "Prepare for Target Practice")
    })?;
    // Shoot the nearest standing target on the range until the practice
    // is over, leading it by the bullet's flight.
    let start = p.server_seconds();
    let mut seen = std::collections::BTreeSet::new();
    let mut last_shot = 0.0;
    let mut shot = false;
    while !p.goals.iter().any(|g| g == "Shooting") {
        ensure!(
            p.server_seconds() - start < 90.0,
            "The target practice never ended: {}",
            p.situation()
        );
        let (tick, targets) = p
            .app
            .network_view()
            .map(|v| (v.tick as f64, v.targets.clone()))
            .unwrap_or_default();
        seen.extend(targets.iter().map(|t| t.id));
        let feet = p.feet()?;
        let eye = feet + Vec3::Y * 2.16;
        let aim = targets
            .iter()
            .filter(|t| !t.hit && !t.gone(tick + 12.0))
            .map(|t| {
                let flight = (board(t, tick) - eye).length() / 90.0;
                board(t, tick + f64::from(flight * TICKS) + 6.0)
            })
            .filter(|b| b.x > -42.0 && b.x < -35.0)
            .min_by(|a, b| a.z.total_cmp(&b.z).reverse());
        if let Some(aim) = aim {
            let (yaw, pitch) = view_toward(aim - eye);
            p.steer(yaw, pitch, 0.3);
            let (y, pi) = p.angles();
            let now = p.server_seconds();
            if wrap(yaw - y).abs() < 0.01 && (pitch - pi).abs() < 0.01 && now - last_shot > 0.35 {
                last_shot = now;
                p.set("mouseFire", true)?;
                p.frame()?;
                p.set("mouseFire", false)?;
            }
        }
        p.frame()?;
        if !shot && seen.len() >= 3 && std::env::var("BRI_TUTORIAL_SHOT").is_ok() {
            shot = true;
            let path = p.artifact.join("target-range.png");
            capture(&mut p.app, &path)?;
            p.note(format!("saved {}", path.display()));
        }
    }
    ensure!(
        seen.len() >= 50,
        "Only {} targets ever reached this client",
        seen.len()
    );
    // "(hit/launched targets hit with N% accuracy)": shots must land.
    let result = p
        .said
        .iter()
        .rev()
        .find(|s| s.contains("targets hit"))
        .cloned()
        .unwrap_or_default();
    let hit: u32 = result
        .split('(')
        .nth(1)
        .and_then(|r| r.split('/').next())
        .and_then(|n| n.trim().parse().ok())
        .with_context(|| format!("No shooting result in {result:?}"))?;
    ensure!(hit >= 40, "Only {hit} targets hit: {result}");
    Ok(())
}

/// One offscreen frame of the game view, saved as a PNG.
fn capture(app: &mut App, path: &Path) -> Result<()> {
    use bri_client::platform::RenderContext;
    use bri_ui::gpu::{Headless, UiRenderer};
    let gpu = Headless::new().context("offscreen renderer")?;
    let mut renderer = UiRenderer::new(&gpu.device, &gpu.queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    app.gpu_ready(&gpu.device, &gpu.queue, format)?;
    app.tick(Duration::from_millis(16))?;
    let extent = wgpu::Extent3d {
        width: SIZE.0,
        height: SIZE.1,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tutorial walkthrough capture"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    ensure!(
        app.render_scene(&mut RenderContext {
            device: &gpu.device,
            queue: &gpu.queue,
            encoder: &mut encoder,
            target: &view,
            format,
            size: SIZE,
            ui_renderer: &mut renderer,
        })?,
        "The App rendered no camera"
    );
    let row = (SIZE.0 * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("tutorial walkthrough readback"),
        size: u64::from(row) * u64::from(SIZE.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE.1),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(5))??;
    let mapped = buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|e| anyhow::anyhow!("readback: {e:?}"))?;
    let mut pixels = Vec::with_capacity((SIZE.0 * SIZE.1 * 4) as usize);
    for line in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&line[..SIZE.0 as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    app.gpu_stopped();
    image::save_buffer(path, &pixels, SIZE.0, SIZE.1, image::ColorType::Rgba8)?;
    Ok(())
}

#[test]
#[ignore = "requires converted native v20 content and loopback QUIC; no window, GPU or OS input"]
fn a_new_player_plays_the_tutorial_through_target_practice() -> Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut p = start(&workspace)?;
    let result = (|| -> Result<()> {
        look_and_move(&mut p)?;
        jump(&mut p)?;
        duck(&mut p)?;
        bricks(&mut p)?;
        build(&mut p)?;
        hammer(&mut p)?;
        jet(&mut p)?;
        light(&mut p)?;
        ride(&mut p)?;
        dismount(&mut p)?;
        wrench(&mut p)?;
        print(&mut p)?;
        diving(&mut p)?;
        shooting(&mut p)?;
        Ok(())
    })();
    p.save()?;
    result
}
