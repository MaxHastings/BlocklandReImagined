//! Headless native-pack integration check. Does not open a window or play audio.
use bri_minigames::*;
use std::{error::Error, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: catalog_smoke <catalog.json>")?;
    let bytes = std::fs::read(path)?;
    if bytes.len() > 1024 * 1024 {
        return Err("catalog input too large".into());
    }
    let catalog: Catalog = serde_json::from_slice(&bytes)?;
    let mut world = MinigamesWorld::new(catalog.clone(), PolicyMode::Internet, true)?;
    let mut players = Vec::new();
    for i in 1..=8 {
        let p = world.connect(AccountId(i), format!("Headless {i}"), i == 1)?;
        world.set_ready(p, true)?;
        players.push(p);
    }
    world.execute(Command::Create {
        actor: players[0],
        color: 0,
        settings: Settings::default(),
    })?;
    world.execute(Command::Create {
        actor: players[4],
        color: 1,
        settings: Settings::default(),
    })?;
    let games = [
        world.player(players[0])?.game.unwrap(),
        world.player(players[4])?.game.unwrap(),
    ];
    for (i, p) in players.iter().enumerate() {
        if i != 0 && i != 4 {
            world.execute(Command::Join {
                actor: *p,
                game: games[i / 4],
            })?;
        }
    }
    let mut settings = Settings::default();
    for id in &catalog.player_types {
        settings.player_type = id.clone();
        let effects = world.execute(Command::Configure {
            actor: players[0],
            settings: settings.clone(),
        })?;
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::ApplyEquipment { .. }))
                .count(),
            4
        );
    }
    for id in catalog.items.keys() {
        settings.loadout[0] = Some(id.clone());
        world.execute(Command::Configure {
            actor: players[0],
            settings: settings.clone(),
        })?;
        let expected = catalog.items[id].as_ref();
        let equipment = world.game(games[0])?.settings.equipment(&catalog);
        assert_eq!(equipment.start_ball.as_ref(), expected);
        for _ in 0..6 {
            world.step()?;
        }
    }
    let start = Instant::now();
    let mut checks = 0_u64;
    let mut effects = 0_usize;
    for tick in 0..12000 {
        effects += world.step()?.len();
        for source in &players {
            for target in &players {
                let source_game = world.player(*source)?.game;
                let target_game = world.player(*target)?.game;
                if let Ok(t) = world.target_for_player(*target) {
                    assert_eq!(
                        world.can_damage(world.projectile_source(*source)?, t),
                        if source_game == target_game {
                            Decision::Allow
                        } else {
                            Decision::Deny(Denial::DifferentGame)
                        }
                    );
                    checks += 1;
                }
            }
        }
        if tick % 240 == 0 {
            for (killer, victim) in [(players[0], players[1]), (players[4], players[5])] {
                if let LifeState::Alive { life } = world.player(victim)?.life {
                    effects += world.died(victim, life, Some(killer))?.len();
                }
            }
        }
        for p in &players {
            if let LifeState::Dead { ready_at, .. } = world.player(*p)?.life
                && ready_at <= world.tick()
            {
                effects += world.execute(Command::Respawn { actor: *p })?.len();
            }
        }
    }
    let elapsed = start.elapsed();
    let snapshot = world.save()?;
    let restored = MinigamesWorld::restore(&snapshot, catalog.clone())?;
    assert_eq!(snapshot, restored.save()?);
    println!(
        "{}",
        serde_json::json!({"status":"pass","players":8,"minigames":2,
        "player_types":catalog.player_types.len(),"items":catalog.items.len(),"ticks":12000,
        "damage_checks":checks,"effects":effects,"elapsed_ms":elapsed.as_secs_f64()*1000.0,
        "snapshot_bytes":snapshot.len(),"tick_hz":TICKS_PER_SECOND})
    );
    Ok(())
}
