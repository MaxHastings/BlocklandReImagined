//! Bot hardening scenarios: crowds and places that made bots do something a
//! player would not, each found by a probe, fixed, and kept here.
mod gauntlet;

use bri_chaos::fixture;
use bri_minigames::Settings;
use bri_sim::bot_kind::BotKind;
use bri_world::{Brick, OwnerId};
use gauntlet::*;
use glam::Vec3;
use std::collections::BTreeMap;

const FAR_A: Vec3 = Vec3::new(92.0, 0.05, 92.0);
const FAR_B: Vec3 = Vec3::new(92.0, 0.05, 86.0);
const GUN: &str = bri_weapons::testing::GUN_ITEM;
const BRAWLER: &str = "hardening:bot/brawler";
const NO_JETS: &str = "v20.player.playernojet";

fn brawler() -> BotKind {
    let mut k = blockhead_kinds().remove(0);
    k.id = BRAWLER.into();
    k.name = BRAWLER.into();
    k.melee = Some(melee(20.0, 2.5));
    k
}

fn loadout(items: &[&str]) -> [Option<String>; 5] {
    let mut out: [Option<String>; 5] = Default::default();
    for (slot, item) in items.iter().enumerate() {
        out[slot] = Some(item.to_string());
    }
    out
}

/// How a battle is laid out.
struct Spec {
    name: &'static str,
    /// Spawn bricks of red (side 0) and blue (side 1).
    red: Vec<Vec3>,
    blue: Vec<Vec3>,
    kinds: [&'static str; 2],
    extra_kinds: Vec<BotKind>,
    vehicle_kinds: Vec<&'static str>,
    /// Arena bricks (red's), and blue's own.
    bricks: Vec<Brick>,
    blue_bricks: Vec<Brick>,
    settings: Settings,
    /// Arena brick and item definitions for a package.
    brick_defs: Vec<(&'static str, &'static str)>,
    item_defs: Vec<(&'static str, &'static str)>,
}
impl Spec {
    fn new(name: &'static str, red: Vec<Vec3>, blue: Vec<Vec3>, items: &[&str]) -> Self {
        Self {
            name,
            red,
            blue,
            kinds: [fixture::BOT, fixture::BOT],
            extra_kinds: Vec::new(),
            vehicle_kinds: Vec::new(),
            bricks: Vec::new(),
            blue_bricks: Vec::new(),
            settings: Settings {
                loadout: loadout(items),
                use_all_players_bricks: true,
                ..Default::default()
            },
            brick_defs: Vec::new(),
            item_defs: Vec::new(),
        }
    }
}

/// Spawn points in a line across z at `x`, `count` of them 4 apart.
fn line(x: f32, y: f32, z: f32, count: usize) -> Vec<Vec3> {
    (0..count)
        .map(|i| Vec3::new(x, y, z + (i as f32 - (count as f32 - 1.0) / 2.0) * 4.0))
        .collect()
}

struct Battle {
    name: &'static str,
    arena: Arena,
    sides: BTreeMap<OwnerId, u32>,
}

/// Two builders' bots in one mini-game; `package` runs before anyone
/// joins.
fn battle(spec: Spec, package: impl FnOnce(&mut Arena)) -> Battle {
    let mut arena = Arena::custom(&spec.brick_defs, &spec.item_defs, &spec.vehicle_kinds);
    arena.kinds(spec.extra_kinds, &spec.vehicle_kinds);
    package(&mut arena);
    let red = arena.join("Red builder", FAR_A);
    let blue = arena.join("Blue builder", FAR_B);
    let mut spawns = Vec::new();
    let mut red_bricks = spec.bricks;
    for at in &spec.red {
        red_bricks.push(spawner(spec.kinds[0], *at));
        spawns.push((*at, 0));
    }
    let mut blue_bricks = spec.blue_bricks;
    for at in &spec.blue {
        spawns.push((*at, 1));
        blue_bricks.push(spawner(spec.kinds[1], *at));
    }
    arena.load(red, red_bricks);
    arena.load(blue, blue_bricks);
    let game = arena.minigame(red, spec.settings);
    arena.join_game(blue, game);
    arena.step(TICKS_PER_SECOND);
    let sides = arena.sides_by_spawn(&spawns);
    assert_eq!(
        sides.len(),
        spec.red.len() + spec.blue.len(),
        "every bot spawned"
    );
    Battle {
        name: spec.name,
        arena,
        sides,
    }
}

impl Battle {
    /// Play `seconds` (times the soak rounds), scoring every tick;
    /// `each` adds scenario progress.
    fn play(
        &mut self,
        floor: f32,
        seconds: usize,
        mut each: impl FnMut(&bri_sim::session::Session, &mut Report),
    ) -> Report {
        let mut scorer = Scorer::new(self.name, floor);
        for _ in 0..seconds * TICKS_PER_SECOND * rounds() {
            timed_step(&mut self.arena, &mut scorer.report);
            let s = &self.arena.s;
            let vitals = s.vitals();
            let sides = &self.sides;
            scorer.sample(s, sides, |bot| {
                sides
                    .iter()
                    .any(|(o, side)| Some(side) != sides.get(&bot) && vitals[o].alive)
            });
            each(s, &mut scorer.report);
        }
        finish(&scorer.report);
        scorer.report
    }
}

fn stairs(at: Vec3, steps: usize, rise: f32, width: usize) -> Vec<Brick> {
    let mut out = Vec::new();
    for i in 0..steps {
        let top = rise * (i as f32 + 1.0);
        let mut y = 0.0;
        while y < top - 0.01 {
            let (def, h) = if top - y >= 0.6 - 0.01 {
                (fixture::BRICK, 0.6)
            } else {
                (fixture::PLATE, 0.2)
            };
            for w in 0..width {
                if def == fixture::BRICK {
                    // 2 x 1 units: lay it along z.
                    let mut b = brick(
                        def,
                        Vec3::new(
                            at.x + i as f32 + 0.5,
                            y + h / 2.0,
                            at.z + w as f32 * 2.0 + 1.0,
                        ),
                    );
                    b.quarter_turns = 1;
                    out.push(b);
                } else {
                    for dz in 0..4 {
                        for dx in 0..2 {
                            out.push(brick(
                                def,
                                Vec3::new(
                                    at.x + i as f32 + 0.25 + dx as f32 * 0.5,
                                    y + h / 2.0,
                                    at.z + w as f32 * 2.0 + 0.25 + dz as f32 * 0.5,
                                ),
                            ));
                        }
                    }
                }
            }
            y += h;
        }
    }
    out
}

fn once(bricks: Vec<Brick>) -> Vec<Brick> {
    let mut seen = std::collections::BTreeSet::new();
    bricks
        .into_iter()
        .filter(|b| {
            seen.insert((
                (b.position[0] * 4.0).round() as i32,
                (b.position[1] * 4.0).round() as i32,
                (b.position[2] * 4.0).round() as i32,
            ))
        })
        .collect()
}

/// Sixteen a side in a block of walls, a deck with stairs: the first half minute
/// of a crowd that has to find its way across. One path-search budget shared
/// by thirty-two searches left most of them waiting, their bots standing at
/// their spawns with no route yet (nearly half of the first thirty seconds
/// stuck); a budget grown with the crowd leaves a few percent.
#[test]
fn thirty_two_bots_set_off_across_a_walled_map_without_waiting_to_find_a_way() {
    let spread = |x: f32| -> Vec<Vec3> {
        line(x, 0.0, 52.0, 16)
            .into_iter()
            .enumerate()
            .map(|(i, v)| Vec3::new(v.x + (i % 2) as f32 * 2.0, v.y, v.z))
            .collect()
    };
    let mut spec = Spec::new("walled_map_32", spread(-92.0), spread(-24.0), &[GUN]);
    let mut bricks = Vec::new();
    for (x, z0, z1) in [
        (-76.0, 20.0, 50.0),
        (-66.0, 56.0, 86.0),
        (-52.0, 20.0, 46.0),
        (-44.0, 58.0, 86.0),
    ] {
        for course in fixture::barrier_courses() {
            bricks.extend(wall(
                Vec3::new(x, course - 1.5, z0),
                Vec3::new(x, course - 1.5, z1),
            ));
        }
    }
    bricks.extend(floor(Vec3::new(-64.0, 0.0, 30.0), [1, 1], 3.0));
    bricks.extend(stairs(Vec3::new(-69.0, 0.0, 33.0), 5, 0.6, 3));
    spec.bricks = once(bricks);
    let mut b = battle(spec, |a| {
        a.s.set_server_settings(bri_admin::ServerSettings {
            max_bots: 32,
            ..a.s.server_settings().clone()
        })
        .unwrap();
    });
    assert_eq!(b.sides.len(), 32);
    let r = b.play(0.0, 30, |_, _| {});
    eprintln!(
        "walled_map_32: stuck {:.3}, circling {:.3}, kills {}",
        r.share(r.stuck),
        r.share(r.circling),
        r.kills
    );
    assert!(r.share(r.stuck) < 0.12, "stuck {:.3}", r.share(r.stuck));
    assert!(r.kills > 0, "the sides met");
}

/// Allies in a file along a deck's edge: each steps round the one ahead,
/// and the side it picks is the edge's. None of them goes over. Without
/// the fix (a step round a body judged as the route's own, so a drop that
/// does not hurt is walked off) the four layouts lose five bots between them.
#[test]
fn allies_stepping_round_each_other_stay_on_a_deck() {
    let falls = deck_file_falls(&[
        (1.0, 54.5, 0.0),
        (1.0, 55.0, 0.0),
        (1.25, 55.0, 0.0),
        (2.0, 55.0, 0.0),
    ]);
    // A bot stepping off an ally's head can still carry its speed past the
    // edge, and where that happens differs between machines (physics floats):
    // one such fall in the four layouts is allowed. The bug cost 5 on the
    // machine it was found on.
    assert!(falls <= 1, "{falls} bots walked off the deck");
}

/// The same file nearer the edge: a bot the stall judge finds going nowhere
/// while it still moves (wobbling between two spots) jumped as if standing
/// still, and the jump carried it on over the edge. Now that jump is judged
/// where it comes down, as any hop. And a bot held at the edge that jumps
/// in place steered its footwork out over the edge in the air, where the
/// edge check measured the drop from its feet in the air (a drop that does
/// not hurt) rather than from the deck it left. Before the fixes the first
/// four layouts lost five bots on Linux and two on Windows CI after the
/// first, and the last two (the file and its enemy shifted a little) one
/// each on Linux; one is allowed for the machine-to-machine spread above.
#[test]
fn a_stalled_bot_on_the_move_does_not_jump_off_a_deck() {
    let falls = deck_file_falls(&[
        (0.75, 54.5, 0.0),
        (0.75, 55.5, 0.0),
        (1.25, 55.5, 0.0),
        (2.0, 55.5, 0.0),
        (2.0, 55.5, 0.185),
        (2.0, 55.5, 0.222),
    ]);
    assert!(falls <= 1, "{falls} bots went over the deck's edge");
}

/// Four allied brawlers in a file along the south edge of a 24 x 16 deck
/// six up, `spacing` apart at `z` (the edge is z = 56), and one enemy, the
/// file and the enemy moved by `shift` (the enemy seven times as far east
/// and three times as far north): the bots that went over the edge in 40 s
/// of each layout, summed.
fn deck_file_falls(layouts: &[(f32, f32, f32)]) -> u64 {
    let trace_failures = std::env::var_os("BRI_BOT_FAILURE_TRACE").is_some();
    let mut falls = 0;
    for &(spacing, z, shift) in layouts {
        let mut arena = Arena::custom(&[], &[], &[]);
        arena.kinds(vec![brawler()], &[]);
        let red = arena.join("Red builder", FAR_A);
        let blue = arena.join("Blue builder", FAR_B);
        let mut bricks = floor(Vec3::new(-79.0, 0.0, 40.0), [3, 2], 6.0);
        let file: Vec<Vec3> = (0..4)
            .map(|i| Vec3::new(-76.0 + shift + i as f32 * spacing, 6.0, z))
            .collect();
        for at in &file {
            bricks.push(spawner(BRAWLER, *at));
        }
        arena.load(red, bricks);
        let enemy = Vec3::new(-58.0 + shift * 7.0, 6.0, 47.0 - shift * 3.0);
        arena.load(blue, vec![spawner(BRAWLER, enemy)]);
        let settings = Settings {
            loadout: Default::default(),
            use_all_players_bricks: true,
            player_type: NO_JETS.into(),
            falling_damage: true,
            ..Default::default()
        };
        let game = arena.minigame(red, settings);
        arena.join_game(blue, game);
        arena.step(TICKS_PER_SECOND);
        let mut spawns: Vec<(Vec3, u32)> = file.iter().map(|at| (*at, 0)).collect();
        spawns.push((enemy, 1));
        let sides = arena.sides_by_spawn(&spawns);
        let mut scorer = Scorer::new("deck_file", 6.0);
        let mut trace_frames = std::collections::VecDeque::new();
        for tick in 0..40 * TICKS_PER_SECOND {
            arena.step(1);
            let vitals = arena.s.vitals();
            if trace_failures && tick % 12 == 0 {
                let states: Vec<_> = arena
                    .s
                    .motion_states()
                    .into_iter()
                    .filter(|(p, _)| sides.contains_key(&p.owner))
                    .map(|(p, _)| (p.owner, p.feet, p.velocity, p.grounded))
                    .collect();
                let thoughts: Vec<_> = arena
                    .s
                    .bot_thoughts()
                    .into_iter()
                    .map(|t| (t.bot, t.goal, t.next, t.acted, t.input))
                    .collect();
                trace_frames.push_back(format!("tick {tick}: {states:?}; {thoughts:?}"));
                if trace_frames.len() > 24 {
                    trace_frames.pop_front();
                }
            }
            let before_falls = scorer.report.fell;
            scorer.sample(&arena.s, &sides, |bot| {
                sides
                    .iter()
                    .any(|(o, side)| Some(side) != sides.get(&bot) && vitals[o].alive)
            });
            if trace_failures && scorer.report.fell > before_falls {
                eprintln!(
                    "deck fall spacing {spacing} z {z} shift {shift}: {}",
                    trace_frames.make_contiguous().join("\n")
                );
            }
        }
        let r = scorer.report;
        falls += r.fell;
        eprintln!(
            "deck_file spacing {spacing} z {z} shift {shift}: fell {}",
            r.fell
        );
        assert!(r.kills > 0, "a real fight: {}", r.kills);
    }
    falls
}
