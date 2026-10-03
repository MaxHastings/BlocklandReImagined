//! Unsampled optimized diagnostic for sixteen real NPC controllers, rule
//! discovery/model budgets and ordinary mixed-inventory fighting. No timing
//! assertions: machine-specific distributions accompany control progress.
use anyhow::{Result, ensure};
use bri_chaos::fixture;
use bri_events::rules::{Compare, Condition, Datum, Property, Subject};
use bri_events::{Row, Slot, Target, Value};
use bri_sim::player::MoveInput;
use bri_sim::session::{Command, MiniGameRequest, Session, ToolCatalog};
use bri_world::{Brick, ContentRef, VehicleSpawn, World, build::SavedBuild};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

fn row(output: &str, params: Vec<Value>, conditions: Vec<Condition>, delay_ms: u32) -> Row {
    Row {
        enabled: true,
        input: "onActivate".into(),
        output: output.into(),
        target: Target::Slot(if output == "setVariable" {
            Slot::SelfBrick
        } else {
            Slot::Player
        }),
        params,
        conditions,
        delay_ms,
        preserved: None,
    }
}
fn flag(key: &str, value: i64) -> Condition {
    Condition {
        subject: Subject::Player,
        property: Property::Variable,
        key: key.into(),
        compare: Compare::Equal,
        value: Datum::Number(value),
    }
}
fn session(flags: usize) -> Result<(Session, u64, Vec<u64>)> {
    let mut s = fixture::synthetic()?.session;
    s.set_event_catalog(bri_events::testing::catalog(), Vec::<String>::new())?;
    s.set_tool_catalog(ToolCatalog {
        items: fixture::synthetic_weapons()?
            .0
            .items
            .keys()
            .cloned()
            .collect(),
        vehicles: [fixture::BOT.into()].into(),
        vehicle_bricks: [fixture::PLATE.into()].into(),
        ..Default::default()
    })?;
    s.set_spawn_points(vec![Vec3::new(90.0, 0.05, 90.0)])?;
    let owner = s.join("Profiler".into(), Vec3::new(90.0, 0.05, 90.0), true)?;
    let mut world = World::new(
        "Combined objective fight".into(),
        "chaos/map".into(),
        vec![[1.0; 4]],
    );
    for n in 0..16 {
        let mut b = Brick::new(
            ContentRef::Resolved(fixture::PLATE.into()),
            [
                -7.75 + (n % 4) as f32 * 4.0,
                0.1,
                20.25 + (n / 4) as f32 * 4.0,
            ],
            owner,
        );
        b.vehicle = Some(Box::new(VehicleSpawn {
            vehicle: ContentRef::Resolved(fixture::BOT.into()),
            recolor: false,
        }));
        world.bricks.insert(n + 1, b);
    }
    let keys: Vec<_> = (0..flags).map(|n| format!("choice{n}")).collect();
    for (n, key) in keys.iter().enumerate() {
        let angle = n as f32 * std::f32::consts::TAU / flags as f32;
        let at = Vec3::new(angle.cos() * 14.0, 0.1, 30.0 + angle.sin() * 14.0);
        let at = [
            (at.x / 0.5).round() * 0.5 + 0.25,
            at.y,
            (at.z / 0.5).round() * 0.5 + 0.25,
        ];
        let mut b = Brick::new(ContentRef::Resolved(fixture::PLATE.into()), at, owner);
        b.name = Some(format!("authoredLatch{n}"));
        b.events.push(row(
            "setVariable",
            vec![Value::Int(1), Value::Text(key.clone()), Value::Int(1)],
            vec![flag(key, 0)],
            0,
        ));
        if n + 1 == flags {
            let guards = keys.iter().map(|k| flag(k, 1)).collect::<Vec<_>>();
            b.events.push(row(
                "addPlayerScore",
                vec![Value::Int(17)],
                guards.clone(),
                450,
            ));
            b.events.push(row("winRound", vec![], guards, 450));
            b.events.push(Row {
                input: "onRuleRoundEnd".into(),
                output: "setColorFX".into(),
                target: Target::Slot(Slot::SelfBrick),
                params: vec![Value::Int(1)],
                ..row("setColorFX", vec![], vec![], 0)
            });
        }
        world.bricks.insert(17 + n as u64, b);
    }
    world.next_brick_id = 17 + flags as u64;
    s.command(
        owner,
        100,
        Command::LoadBuild {
            build: Box::new(SavedBuild::new(world)),
            ownership: false,
        },
    )?;
    for _ in 0..10 {
        s.step()?;
    }
    let settings = bri_minigames::Settings {
        loadout: [
            Some(bri_weapons::testing::GUN_ITEM.into()),
            Some(bri_weapons::testing::ROCKET_ITEM.into()),
            Some(bri_weapons::testing::SPEAR_ITEM.into()),
            Some(bri_weapons::testing::SWORD_ITEM.into()),
            None,
        ],
        ..Default::default()
    };
    s.command(
        owner,
        101,
        Command::MiniGame(MiniGameRequest::Create { color: 0, settings }),
    )?;
    let bots = s
        .names()
        .keys()
        .copied()
        .filter(|o| s.is_bot(*o))
        .collect::<Vec<_>>();
    ensure!(bots.len() == 16, "all sixteen controllers spawned");
    Ok((s, owner, bots))
}
struct Evidence {
    objective_ticks: usize,
    fight_ticks: usize,
    max_score: i64,
    damaged: bool,
    diagnostic_samples: usize,
}
fn profile(
    s: &mut Session,
    humans: &[u64],
    bots: &[u64],
    label: &str,
    count: usize,
) -> Result<Evidence> {
    let mut nanos = Vec::with_capacity(count);
    let mut projectiles = BTreeSet::new();
    let mut diagnostic = BTreeMap::<String, usize>::new();
    let mut objective_ticks = 0;
    let mut fight_ticks = 0;
    let mut active_nanos = Vec::new();
    let mut damaged = false;
    let mut max_tick = 0;
    let mut objective_searches = 0;
    let mut objective_reused = 0;
    let mut previous_counters = BTreeMap::new();
    let mut windows = Vec::<(usize, usize, f64)>::new();
    let mut previous_health = BTreeMap::new();
    for tick in 0..count {
        if tick % 1200 == 0 {
            windows.push((0, 0, 0.0));
        }
        for human in humans {
            let vitals = &s.vitals()[human];
            if !vitals.alive
                && !vitals.respawn_held
                && s.simulation().state().tick >= vitals.respawn_tick
            {
                // A participating human would click to respawn; use that
                // ordinary command so the mixed phase keeps a live opponent.
                s.command(
                    *human,
                    (1 << 41) + s.simulation().state().tick,
                    Command::Respawn,
                )?;
            }
            s.movement(
                *human,
                (1 << 40) + s.simulation().state().tick,
                MoveInput::default(),
            )?;
        }
        let start = Instant::now();
        s.step()?;
        let elapsed = start.elapsed().as_nanos();
        if nanos.get(max_tick).is_none_or(|old| *old < elapsed) {
            max_tick = tick;
        }
        nanos.push(elapsed);
        projectiles.extend(
            s.weapon_view()
                .fired()
                .filter(|p| bots.contains(&p.source.0))
                .map(|p| p.id),
        );
        let thoughts = s.bot_thoughts();
        if thoughts.iter().any(|b| {
            s.vitals()[&b.bot].health > 0.0 && matches!(b.behaviour, "fight" | "objective")
        }) {
            active_nanos.push(elapsed);
        }
        damaged |= humans.iter().any(|h| s.vitals()[h].health < 100.0);
        for human in humans {
            let health = s.vitals()[human].health;
            if let Some(old) = previous_health.insert(*human, health) {
                windows.last_mut().unwrap().2 += f64::from((old - health).max(0.0));
            }
        }
        for thought in thoughts {
            let alive = s.vitals()[&thought.bot].health > 0.0;
            objective_ticks += usize::from(alive && thought.behaviour == "objective");
            fight_ticks += usize::from(alive && thought.behaviour == "fight");
            let window = windows.last_mut().unwrap();
            window.0 += usize::from(alive && thought.behaviour == "objective");
            window.1 += usize::from(alive && thought.behaviour == "fight");
            let next = (thought.objective_searches, thought.objective_reused);
            let old = previous_counters
                .insert(thought.bot, next)
                .unwrap_or_default();
            objective_searches += next.0.saturating_sub(old.0);
            objective_reused += next.1.saturating_sub(old.1);
            if let Some(d) = thought.objective_diagnostic {
                *diagnostic.entry(d.into()).or_default() += 1;
            }
        }
    }
    nanos.sort_unstable();
    let p =
        |fraction: f64| nanos[((nanos.len() - 1) as f64 * fraction).ceil() as usize] as f64 / 1e6;
    let scores = bots
        .iter()
        .filter_map(|b| s.vitals().get(b).map(|v| v.score))
        .collect::<Vec<_>>();
    eprintln!(
        "{label}: ticks={count} ms[p50={:.3},p95={:.3},p99={:.3},max={:.3}] max_tick={max_tick} >5ms={} >50ms={} distinct_live_projectile_ids={} fight_controller_ticks={fight_ticks} objective_controller_ticks={objective_ticks} scores={scores:?} diagnostics={diagnostic:?}",
        p(0.5),
        p(0.95),
        p(0.99),
        p(1.0),
        nanos.iter().filter(|n| **n > 5_000_000).count(),
        nanos.iter().filter(|n| **n > 50_000_000).count(),
        projectiles.len()
    );
    eprintln!(
        "{label} objective_searches={objective_searches} objective_reused={objective_reused} windows_10s[objective_ticks,fight_ticks,observed_human_health_loss]={windows:?}"
    );
    active_nanos.sort_unstable();
    if !active_nanos.is_empty() {
        let active = |f: f64| {
            active_nanos[((active_nanos.len() - 1) as f64 * f).ceil() as usize] as f64 / 1e6
        };
        eprintln!(
            "{label} active_control: ticks={} ms[p50={:.3},p95={:.3},p99={:.3},max={:.3}]",
            active_nanos.len(),
            active(0.5),
            active(0.95),
            active(0.99),
            active(1.0)
        );
    }
    ensure!(
        s.take_event_diagnostics().is_empty(),
        "ordinary creator events remain valid"
    );
    Ok(Evidence {
        objective_ticks,
        fight_ticks,
        max_score: scores.into_iter().max().unwrap_or(0),
        damaged,
        diagnostic_samples: diagnostic.values().sum(),
    })
}
#[test]
#[ignore = "optimized diagnostic; reports timing without thresholds"]
fn sixteen_objective_controllers_and_mixed_inventory_combat() -> Result<()> {
    let ticks = std::env::var("BRI_COMBINED_TICKS")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(7200);
    ensure!(ticks > 0, "positive duration");
    for flags in [4, 8, 16] {
        let (mut s, owner, bots) = session(flags)?;
        let objective = profile(
            &mut s,
            &[owner],
            &bots,
            &format!("independent-{flags}-latch-model"),
            ticks,
        )?;
        if flags <= 8 {
            ensure!(
                objective.objective_ticks > 0 && objective.max_score >= 17,
                "objective phase must make real control/score progress"
            );
            ensure!(
                s.simulation()
                    .state()
                    .bricks
                    .values()
                    .any(|b| b.color_effect == 1),
                "objective phase must reach its authored round-end observer"
            );
        } else {
            ensure!(
                objective.objective_ticks > 0 || objective.diagnostic_samples > 0,
                "worst model must run or surface its bounded diagnostic"
            );
        }
        // A fresh authored session prevents a completed quiet round from
        // making the combat phase a benchmark of resting winners.
        let (mut s, owner, bots) = session(flags)?;
        let enemy = s.join("Combat target".into(), Vec3::new(-15.0, 0.05, 48.0), false)?;
        let game = s.minigame_views()[0].id;
        s.set_spawn_points(vec![Vec3::new(-15.0, 0.05, 48.0)])?;
        s.command(enemy, 1, Command::MiniGame(MiniGameRequest::Join { game }))?;
        let combat = profile(
            &mut s,
            &[owner, enemy],
            &bots,
            &format!("mixed-combat-{flags}-latch-model"),
            ticks,
        )?;
        ensure!(
            combat.fight_ticks > 0 && combat.damaged,
            "mixed phase needs active fighting and real damage evidence"
        );
    }
    Ok(())
}
