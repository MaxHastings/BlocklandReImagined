//! v20's Bedroom "Demo Pong" save, a two-player Pong game made only of
//! wrench events: relays, enable/toggle state machines, print counters and a
//! bouncing `pongProjectile`. It stresses the event engine end to end; see
//! `docs/audits/pong-events.md`.
//!
//! Layout (native Y-up, v20 y = -z): the court is the plane x = -52.75.
//! Player A's column of six paddle cells is at z 134.75, player B's at
//! z 126.75, cell 1 on top (y 290.5) down to cell 6 (y 287.5). One cell per
//! side is the white paddle; a ball hitting a black cell scores for the
//! other side. Each side's `+` and `-` print bricks move its paddle through
//! a chain of `_pong_AU*`/`_pong_AD*` relay bricks whose rows are switched
//! on and off to remember where the paddle is.
use bri_sim::{
    definitions::Definitions,
    player::MoveInput,
    presentation::CueKind,
    session::{Session, ToolCatalog},
    simulation::Simulation,
};
use glam::Vec3;
use rapier3d::prelude::*;
use serde_json::json;
use std::path::Path;

const PONG: &str =
    "worlds-pass-005/8a3130ab3cd542e8cac5dbabdee87e80f6eec7f7ba041d995610aee47156d50c.world.json";
const DIGITS: &str = "print/print_letters_default/";
/// The paddle columns' court-facing faces.
const FACE_A: f32 = 134.5;
const FACE_B: f32 = 127.0;
/// Floor top and ceiling bottom of the court.
const FLOOR: f32 = 287.2;
const CEILING: f32 = 290.8;

fn json(path: &Path) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Side {
    A,
    B,
}

struct Pong {
    s: Session,
    owner: u64,
    sequence: u64,
    reset: u64,
    /// Each side's paddle-up and paddle-down print bricks.
    up: [u64; 2],
    down: [u64; 2],
    /// Paddle cells by side, cell 1 first.
    cells: [Vec<u64>; 2],
    /// Last tick each side clicked, so clicks are 125 ms apart.
    clicked: [u64; 2],
}

impl Pong {
    fn load() -> anyhow::Result<Self> {
        Self::load_with(|_| {})
    }
    /// Load the save after `edit` changes it.
    fn load_with(edit: impl FnOnce(&mut bri_world::World)) -> anyhow::Result<Self> {
        let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let catalog = bri_events::Catalog::load(content.join("events-pack-002/catalog.json"))?;
        let brick_catalog =
            serde_json::from_value(json(&content.join("stock-catalog-004/stock-catalog.json"))?)?;
        let effects =
            serde_json::from_value(json(&content.join("effects-pass-004/effects.json"))?)?;
        let materials = serde_json::from_value(json(
            &content.join("brick-materials-001/brick-materials.json"),
        )?)?;
        let weapons = bri_weapons::Pack::from_json(&std::fs::read(
            content.join("weapons-pack-009/weapons.json"),
        )?)?;
        let mut world = bri_world::persistence::load(&content.join(PONG))?;
        assert_eq!(world.name, "Demo Pong");
        edit(&mut world);
        let definitions = Definitions::load(
            &content.join("stock-catalog-004"),
            &content.join("maps-pass-007"),
        )?;
        let mut s = Session::new(Simulation::new(
            world,
            definitions,
            vec![
                ColliderBuilder::cuboid(500.0, 0.5, 500.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )?);
        let mut tools = ToolCatalog::from_native(&brick_catalog, &effects, &materials)?;
        tools.install_items(weapons.items.keys().cloned())?;
        s.set_weapon_pack(weapons)?;
        s.set_tool_catalog(tools)?;
        s.set_event_catalog(catalog, Vec::<String>::new())?;
        s.set_spawn_points(vec![Vec3::new(0.0, 0.05, 0.0)])?;
        let owner = s.join("Tester".into(), Vec3::new(0.0, 0.05, 0.0), true)?;
        let bricks = &s.simulation().state().bricks;
        let find = |f: &dyn Fn(&bri_world::Brick) -> bool| -> u64 {
            let found: Vec<u64> = bricks
                .iter()
                .filter(|(_, b)| f(b))
                .map(|(id, _)| *id)
                .collect();
            assert_eq!(found.len(), 1);
            found[0]
        };
        let print = |b: &bri_world::Brick, name: &str, z: f32| {
            matches!(&b.print, Some(bri_world::ContentRef::Unresolved { name: n, .. }) if n == name)
                && (b.position[2] - z).abs() < 0.01
                && b.position[0] < -48.0
                && b.position[0] > -48.5
        };
        let reset = find(&|b| b.events.iter().any(|r| r.output == "setPrintCount"));
        let up = [
            find(&|b| print(b, "Letters/-plus", 134.75)),
            find(&|b| print(b, "Letters/-plus", 126.75)),
        ];
        let down = [
            find(&|b| print(b, "Letters/-minus", 134.75)),
            find(&|b| print(b, "Letters/-minus", 126.75)),
        ];
        let cells = ["A", "B"].map(|side| {
            (1..=6)
                .map(|i| find(&|b| b.name.as_deref() == Some(&format!("_pong_Paddle{side}{i}"))))
                .collect()
        });
        let mut pong = Self {
            s,
            owner,
            sequence: 1,
            reset,
            up,
            down,
            cells,
            clicked: [0; 2],
        };
        pong.step()?;
        Ok(pong)
    }
    fn tick(&self) -> u64 {
        self.s.simulation().state().tick
    }
    fn step(&mut self) -> anyhow::Result<()> {
        self.sequence += 1;
        self.s
            .movement(self.owner, self.sequence, MoveInput::default())?;
        self.s.step()?;
        let diagnostics = self.s.take_event_diagnostics();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let notices = self.s.take_notices();
        assert!(
            !notices.iter().any(|n| n.starts_with("Weapon runtime")),
            "{notices:?}"
        );
        Ok(())
    }
    fn click(&mut self, brick: u64) {
        self.s
            .fire_brick_input(brick, "onActivate", Some(self.owner));
    }
    fn brick(&self, id: u64) -> &bri_world::Brick {
        &self.s.simulation().state().bricks[&id]
    }
    fn named(&self, name: &str) -> Vec<&bri_world::Brick> {
        self.s
            .simulation()
            .state()
            .bricks
            .values()
            .filter(|b| b.name.as_deref() == Some(name))
            .collect()
    }
    fn score(&self, side: Side) -> u8 {
        let name = match side {
            Side::A => "_pong_ScoreA",
            Side::B => "_pong_ScoreB",
        };
        match &self.named(name)[0].print {
            Some(bri_world::ContentRef::Resolved(p)) => {
                p.strip_prefix(DIGITS).unwrap().parse().unwrap()
            }
            Some(bri_world::ContentRef::Unresolved { name, .. }) => {
                name.strip_prefix("Letters/").unwrap().parse().unwrap()
            }
            None => panic!("score brick lost its print"),
        }
    }
    fn ball(&self) -> Option<bri_weapons::Projectile> {
        let view = self.s.weapon_view();
        assert!(view.projectiles.len() <= 1, "one ball at a time");
        view.projectiles.into_iter().next()
    }
    /// The side's paddle cell (1..=6): the white one, whose scoring rows
    /// are off. Exactly one per side.
    fn paddle(&self, side: Side) -> usize {
        let cells = &self.cells[side as usize];
        let white: Vec<usize> = (0..6)
            .filter(|i| self.brick(cells[*i]).color == 15)
            .map(|i| i + 1)
            .collect();
        assert_eq!(white.len(), 1, "{side:?} paddle cells {white:?}");
        let cell = self.brick(cells[white[0] - 1]);
        assert!(
            !cell.events[2].enabled && cell.events[5].enabled,
            "a white cell does not score"
        );
        white[0]
    }
    /// Click the side's buttons toward `cell`, at most once per 125 ms.
    fn steer(&mut self, side: Side, cell: usize) {
        let tick = self.tick();
        if tick < self.clicked[side as usize] + 15 {
            return;
        }
        let at = self.paddle(side);
        let button = match cell.cmp(&at) {
            std::cmp::Ordering::Less => self.up[side as usize],
            std::cmp::Ordering::Greater => self.down[side as usize],
            std::cmp::Ordering::Equal => return,
        };
        self.clicked[side as usize] = tick;
        self.click(button);
    }
    /// The side the ball is heading to, and the paddle cell it will reach.
    fn forecast(&self) -> Option<(Side, usize)> {
        let ball = self.ball()?;
        if ball.position.y < FLOOR {
            return None; // still leaving the serve brick
        }
        let (side, face) = if ball.velocity.z > 0.0 {
            (Side::A, FACE_A)
        } else {
            (Side::B, FACE_B)
        };
        let t = (face - ball.position.z) / ball.velocity.z;
        // Fold the flight between floor and ceiling.
        let span = CEILING - FLOOR;
        let mut y = (ball.position.y + ball.velocity.y * t - FLOOR).rem_euclid(2.0 * span);
        if y > span {
            y = 2.0 * span - y;
        }
        let cell = ((span - y) / 0.6).ceil().clamp(1.0, 6.0) as usize;
        Some((side, cell))
    }
    /// Run up to `ticks`, steering each side's paddle to block (`true`) or
    /// away from the ball (`false`), until `until` holds.
    fn play(
        &mut self,
        ticks: u64,
        block: [bool; 2],
        mut until: impl FnMut(&Self) -> bool,
    ) -> anyhow::Result<bool> {
        for _ in 0..ticks {
            if let Some((side, cell)) = self.forecast() {
                let target = if block[side as usize] {
                    cell
                } else if cell > 3 {
                    1
                } else {
                    6
                };
                self.steer(side, target);
            }
            self.step()?;
            if until(self) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn sounds(&mut self, name: &str) -> Vec<[f32; 3]> {
        self.s
            .take_cues()
            .into_iter()
            .filter_map(|c| match c.kind {
                CueKind::WeaponSound { profile } if profile == name => Some(c.position),
                _ => None,
            })
            .collect()
    }
}

#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn demo_pong_plays_like_v20() -> anyhow::Result<()> {
    let mut pong = Pong::load()?;
    // The save was made just after B won 10-4: B's counter wrapped to 0,
    // B's win light is on and the bumpers delete balls.
    assert_eq!((pong.score(Side::A), pong.score(Side::B)), (4, 0));
    assert!(pong.named("_pong_WinLightB")[0].light.is_some());
    assert!(
        pong.named("_pong_Bumper")
            .iter()
            .all(|b| b.events[0].enabled)
    );
    assert_eq!((pong.paddle(Side::A), pong.paddle(Side::B)), (3, 3));

    // The reset ramp: scores to 0, lights and bumpers off, serve in 2 s.
    pong.click(pong.reset);
    let start = pong.tick();
    pong.step()?;
    assert_eq!((pong.score(Side::A), pong.score(Side::B)), (0, 0));
    assert!(pong.named("_pong_WinLightB")[0].light.is_none());
    assert_eq!(pong.named("_pong_WinLightB")[0].color_effect, 0);
    assert!(
        pong.named("_pong_Bumper")
            .iter()
            .all(|b| !b.events[0].enabled)
    );
    assert!(pong.play(300, [true; 2], |p| p.ball().is_some())?);
    let served = pong.tick() - start;
    assert!((239..=241).contains(&served), "served after {served} ticks");
    // The ball starts in the serve brick in the floor and gets out.
    let ball = pong.ball().unwrap();
    let serve = pong.named("_pong_Serve")[0].position;
    assert!(ball.position.distance(Vec3::from(serve)) < 0.01);
    assert!(pong.play(120, [true; 2], |p| {
        p.ball().is_some_and(|b| b.position.y > FLOOR)
    })?);

    // Both paddles block: a rally with no points. Paddle hits bounce the
    // ball back across the court with the wall sound.
    pong.sounds("pongWallHitSound");
    let first = pong.ball().unwrap().id;
    pong.play(120 * 12, [true; 2], |_| false)?;
    assert_eq!((pong.score(Side::A), pong.score(Side::B)), (0, 0));
    assert_eq!(pong.ball().unwrap().id, first, "the rally ball survives");
    let hits = pong.sounds("pongWallHitSound");
    let paddle_hits = |face: f32| hits.iter().filter(|p| (p[2] - face).abs() < 0.01).count();
    assert!(
        paddle_hits(FACE_A) >= 3 && paddle_hits(FACE_B) >= 3,
        "{} A and {} B paddle hits",
        paddle_hits(FACE_A),
        paddle_hits(FACE_B)
    );

    // A stops blocking: the ball explodes on a black A cell and B scores;
    // a new ball is served 33 ms later.
    assert!(pong.play(120 * 10, [false, true], |p| p.score(Side::B) == 1)?);
    assert_eq!(pong.score(Side::A), 0);
    assert!(pong.ball().is_none(), "the scoring ball exploded");
    assert_eq!(pong.sounds("pongScoreSound").len(), 1);
    let scored = pong.tick();
    assert!(pong.play(10, [false, true], |p| p.ball().is_some())?);
    assert_eq!(pong.tick() - scored, 4, "re-served after 33 ms");

    // B stops blocking instead: A scores.
    assert!(pong.play(120 * 10, [true, false], |p| p.score(Side::A) == 1)?);
    assert_eq!(pong.score(Side::B), 1);

    // B lets everything through: A reaches 10, the counter wraps to 0 and
    // onPrintCountOverFlow lights A's win light and arms the bumpers,
    // which delete the next serve.
    assert!(pong.play(120 * 120, [true, false], |p| p.score(Side::A) == 0)?);
    assert_eq!(pong.score(Side::B), 1);
    let light = pong.named("_pong_WinLightA")[0];
    assert!(light.light.is_some() && light.color_effect == 3);
    assert!(
        pong.named("_pong_Bumper")
            .iter()
            .all(|b| b.events[0].enabled)
    );
    pong.play(120 * 3, [true; 2], |_| false)?;
    assert!(pong.ball().is_none(), "the bumpers stop play");
    assert_eq!((pong.score(Side::A), pong.score(Side::B)), (0, 1));

    // Reset starts a new game.
    pong.click(pong.reset);
    pong.step()?;
    assert_eq!((pong.score(Side::A), pong.score(Side::B)), (0, 0));
    assert!(pong.named("_pong_WinLightA")[0].light.is_none());
    assert!(pong.play(300, [true; 2], |p| p.ball().is_some())?);
    Ok(())
}

/// v20 `getPrintCount` reads the digit a counter shows the first time it
/// counts, so a loaded save keeps counting from its printed score.
#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn loaded_counter_counts_from_its_print() -> anyhow::Result<()> {
    let mut pong = Pong::load()?;
    assert_eq!(pong.score(Side::A), 4);
    // A black B cell hit scores for A.
    let cell = pong.cells[Side::B as usize][5];
    pong.s
        .fire_brick_input(cell, "onProjectileHit", Some(pong.owner));
    pong.step()?;
    assert_eq!(pong.score(Side::A), 5);
    Ok(())
}

/// The paddle buttons walk each paddle one cell per click and stop at the
/// ends, through the relay bricks' enable/disable state machine. Clicks are
/// 125 ms apart: B's `+` cancels its own pending events 100 ms after a
/// click (see `b_up_swallows_clicks_inside_100_ms`).
#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn paddle_buttons_move_one_cell_and_stop_at_the_ends() -> anyhow::Result<()> {
    let mut pong = Pong::load()?;
    for side in [Side::A, Side::B] {
        let i = side as usize;
        let mut expected = pong.paddle(side);
        for (button, moves) in [(pong.down[i], 5), (pong.up[i], 7), (pong.down[i], 2)] {
            for _ in 0..moves {
                pong.click(button);
                for _ in 0..15 {
                    pong.step()?;
                }
                expected = if button == pong.up[i] {
                    (expected - 1).max(1)
                } else {
                    (expected + 1).min(6)
                };
                assert_eq!(pong.paddle(side), expected, "{side:?}");
            }
        }
        assert_eq!(pong.paddle(side), 3);
    }
    // Two clicks inside 33 ms: the second cancels the first's pending
    // relays (zero-delay cancelEvents), so the paddle moves once.
    pong.click(pong.down[0]);
    pong.step()?;
    pong.click(pong.down[0]);
    for _ in 0..10 {
        pong.step()?;
    }
    assert_eq!(pong.paddle(Side::A), 4);
    Ok(())
}

/// An authentic quirk of the save: B's `+` button has its `cancelEvents`
/// row on a 100 ms delay (every other button's is immediate), so a second
/// click inside 100 ms is cancelled by the first click's late cancel.
#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn b_up_swallows_clicks_inside_100_ms() -> anyhow::Result<()> {
    let mut pong = Pong::load()?;
    for (side, gap, moved) in [(Side::B, 10, 1), (Side::A, 10, 2), (Side::B, 15, 2)] {
        let start = pong.paddle(side);
        for _ in 0..2 {
            pong.click(pong.up[side as usize]);
            for _ in 0..gap {
                pong.step()?;
            }
        }
        for _ in 0..30 {
            pong.step()?;
        }
        assert_eq!(start - pong.paddle(side), moved, "{side:?} {gap}");
        while pong.paddle(side) < 3 {
            pong.click(pong.down[side as usize]);
            for _ in 0..15 {
                pong.step()?;
            }
        }
    }
    Ok(())
}

/// Every brick's paint and colour FX, in brick order.
fn colours(pong: &Pong) -> Vec<(u64, u8, u8)> {
    let mut all: Vec<_> = pong
        .s
        .simulation()
        .state()
        .bricks
        .iter()
        .map(|(id, b)| (*id, b.color, b.color_effect))
        .collect();
    all.sort_unstable();
    all
}

/// A paddle cell's colour, glow and rows agree: white glowing cells bounce
/// the ball (rows 5-6 on), black plain ones score (rows 0-4 on).
fn assert_cells_agree(pong: &Pong, context: &str) {
    for side in [Side::A, Side::B] {
        for (i, cell) in pong.cells[side as usize].iter().enumerate() {
            let b = pong.brick(*cell);
            let white = b.color == 15;
            assert!(
                b.color_effect == if white { 3 } else { 0 }
                    && (0..7).all(|row| b.events[row].enabled == (white == (row >= 5))),
                "{context}: {side:?} cell {} colour {} fx {} rows {:?}",
                i + 1,
                b.color,
                b.color_effect,
                b.events.iter().map(|r| r.enabled).collect::<Vec<_>>()
            );
        }
    }
}

/// Hammering the paddle buttons never leaves a colour behind. Each button
/// glows (`setColorFX 3`) and un-glows 100 ms later, and each move repaints
/// two paddle cells through the relay bricks. Clicks here come a tick to
/// 133 ms apart across all four buttons, and some land two to a tick, as
/// when a server hitch delivers queued clicks together. So reverts, relays
/// and B's late `cancelEvents` from different clicks come due together. v20
/// stamps each click with its own millisecond and runs every scheduled row
/// from one queue in time order: every glow ends and each paddle stays one
/// white cell.
#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn hammered_paddle_buttons_restore_every_colour() -> anyhow::Result<()> {
    let mut pong = Pong::load()?;
    let original = colours(&pong);
    let buttons = [pong.up[0], pong.up[1], pong.down[0], pong.down[1]];
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut random = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for round in 0..60 {
        for _ in 0..=random(12) {
            pong.click(buttons[random(4) as usize]);
            if random(4) == 0 {
                pong.click(buttons[random(4) as usize]);
            }
            for _ in 0..=random(16) {
                pong.step()?;
                assert_cells_agree(&pong, &format!("round {round} tick {}", pong.tick()));
            }
        }
        for _ in 0..60 {
            pong.step()?;
        }
        for button in buttons {
            assert_eq!(
                pong.brick(button).color_effect,
                0,
                "round {round}: button glows"
            );
        }
        assert_cells_agree(&pong, &format!("round {round}"));
        for side in [Side::A, Side::B] {
            pong.paddle(side);
        }
    }
    // Walk both paddles home: every brick is back on its loaded colour.
    for side in [Side::A, Side::B] {
        while pong.paddle(side) != 3 {
            let i = side as usize;
            let button = if pong.paddle(side) > 3 {
                pong.up[i]
            } else {
                pong.down[i]
            };
            pong.click(button);
            for _ in 0..15 {
                pong.step()?;
            }
        }
    }
    assert_eq!(colours(&pong), original);
    Ok(())
}

/// Every brick output with a timed revert, on one plain court brick: each
/// click switches it now and back 100 ms later, and cancels its own pending
/// rows 100 ms later like B's `+`. However the clicks overlap, the brick
/// ends as it started.
#[test]
#[ignore = "requires the converted native worlds, event catalog and content packs"]
fn timed_reverts_of_every_brick_output_always_land() -> anyhow::Result<()> {
    let mut target = 0;
    let mut pong = Pong::load_with(|world| {
        target = world
            .bricks
            .iter()
            .find(|(_, b)| b.events.is_empty() && b.name.is_none() && !b.base_plate)
            .map(|(id, _)| *id)
            .unwrap();
        let brick = world.bricks.get_mut(&target).unwrap();
        let color = brick.color;
        let pairs = [
            (
                "setColor",
                json!({ "Color": (color + 1) % 16 }),
                json!({ "Color": color }),
            ),
            ("setColorFX", json!({ "Int": 3 }), json!({ "Int": 0 })),
            (
                "setRendering",
                json!({ "Bool": false }),
                json!({ "Bool": true }),
            ),
            (
                "setColliding",
                json!({ "Bool": false }),
                json!({ "Bool": true }),
            ),
            (
                "setRayCasting",
                json!({ "Bool": false }),
                json!({ "Bool": true }),
            ),
            (
                "setLight",
                json!({ "Datablock": "v20/light/alarmlighta" }),
                json!({ "Datablock": null }),
            ),
            (
                "setEmitter",
                json!({ "Datablock": "v20/emitter/burnemittera" }),
                json!({ "Datablock": null }),
            ),
        ];
        let row = |delay: u32, output: &str, param: Option<&serde_json::Value>| {
            serde_json::from_value(json!({
                "enabled": true,
                "input": "onActivate",
                "delay_ms": delay,
                "target": { "Slot": "SelfBrick" },
                "output": output,
                "params": param.into_iter().collect::<Vec<_>>(),
            }))
            .unwrap()
        };
        for (output, on, off) in &pairs {
            brick.events.push(row(0, output, Some(on)));
            brick.events.push(row(100, output, Some(off)));
        }
        brick.events.push(row(100, "cancelEvents", None));
    })?;
    let start = |p: &Pong| {
        let b = p.brick(target);
        (
            b.color,
            b.color_effect,
            b.visible,
            b.colliding,
            b.raycast,
            b.light.clone(),
            b.emitter.clone(),
        )
    };
    let original = start(&pong);
    let mut seed = 0x51_7cc1_b727_220a_u64;
    let mut random = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for round in 0..80 {
        for _ in 0..=random(8) {
            for _ in 0..=random(2) {
                pong.click(target);
            }
            for _ in 0..=random(16) {
                pong.step()?;
            }
        }
        for _ in 0..30 {
            pong.step()?;
        }
        assert_eq!(start(&pong), original, "round {round}");
    }
    Ok(())
}
