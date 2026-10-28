//! Headless, opt-in travel trace of a real saved match with its saved rules.
//! No bot tasks, positions or vehicle controls are injected. Reports stay local.
use anyhow::{Context, Result, ensure};
use bri_chaos::fixture;
use bri_package::packages::{PackageEntry, PackageSet, Side};
use glam::Vec3;

fn env_list(name: &str, default: &[&str]) -> Vec<String> {
    std::env::var(name).map_or_else(
        |_| default.iter().map(|s| s.to_string()).collect(),
        |v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        },
    )
}
fn env_num(name: &str, default: usize) -> Result<usize> {
    std::env::var(name)
        .ok()
        .map(|v| v.parse::<usize>())
        .transpose()
        .with_context(|| format!("{name} must be a whole number"))
        .map(|v| v.unwrap_or(default))
}

fn packages(root: &std::path::Path, addons: &[String]) -> Result<PackageSet> {
    let mut packages = PackageSet::load_root(root)?;
    for id in addons {
        if packages.packages.iter().any(|p| &p.id == id) {
            continue;
        }
        let manifest: bri_package::library::PackageInfo = serde_json::from_slice(
            &std::fs::read(root.join(format!("addons/{id}/package.json")))
                .with_context(|| format!("Add-On {id} is not in {}", root.display()))?,
        )?;
        packages.packages.push(PackageEntry {
            id: id.clone(),
            version: manifest.version.clone(),
            side: manifest.side().unwrap_or(Side::Shared),
            dir: format!("addons/{id}"),
            role: None,
        });
    }
    bri_package::library::follow_manifest_sides(root, &mut packages);
    bri_package::library::follow_companions(root, &mut packages);
    Ok(packages)
}

#[test]
#[ignore = "needs generated content and BRI_DRIVE_WORLD saved build"]
fn saved_match_vehicle_routes() -> Result<()> {
    let (Some(root), Some(path)) = (fixture::content_root(), std::env::var_os("BRI_DRIVE_WORLD"))
    else {
        eprintln!("skipped: set BRI_CONTENT and BRI_DRIVE_WORLD to trace a saved match");
        return Ok(());
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("BRI_DRIVE_REPORT").context("set BRI_DRIVE_REPORT")?,
    );
    let seconds = env_num("BRI_DRIVE_SECONDS", 120)?;
    ensure!(
        (1..=600).contains(&seconds),
        "BRI_DRIVE_SECONDS must be 1..=600"
    );
    let build = bri_world::build::decode(&std::fs::read(path)?)?;
    let world_name = build.world.name.clone();
    let bricks = build.world.bricks.len();
    let mut saved = build.minigame.context("save needs its mini-game")?;
    let principal = build
        .world
        .owners
        .values()
        .next()
        .context("save owner")?
        .principal
        .clone();
    saved["owner"] = serde_json::json!(principal);
    let mut key = [0_u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&principal[i * 2..i * 2 + 2], 16)?;
    }
    let principal = bri_admin::Principal(key);
    let addons = env_list(
        "BRI_DRIVE_ADDONS",
        &[
            "blockhead_bot",
            "gamemode_slayer",
            "gamemode_slayer_ctf",
            "weapon_hegrenade",
        ],
    );
    let packages = packages(&root, &addons)?;
    let mut s = bri_net::dedicated::load_packages(&root, &packages, build.world)?.session;
    s.set_server_settings(bri_admin::ServerSettings {
        max_bots: 32,
        ..s.server_settings().clone()
    })?;
    s.hold_minigame(saved);
    s.join_verified(
        "Watching host".into(),
        Vec3::new(0.0, 100.0, 0.0),
        true,
        Some(principal),
    )?;
    let mut samples = Vec::new();
    let mut bot_samples = Vec::new();
    let mut projectiles = Vec::new();
    let mut seen_projectiles = std::collections::BTreeSet::new();
    let mut damage = Vec::new();
    let mut damage_read = s.damage_recorded();
    let mut deaths = Vec::new();
    let mut seen_deaths = std::collections::BTreeSet::new();
    let mut rounds = Vec::new();
    let mut seen_rounds = std::collections::BTreeSet::new();
    let nav_detail = std::env::var_os("BRI_DRIVE_NAV_DETAIL").is_some();
    for i in 0..seconds * 120 {
        s.step()?;
        let tick = s.simulation().state().tick;
        let weapons = s.weapon_view();
        for p in weapons.fired() {
            if seen_projectiles.insert(p.id) {
                projectiles.push(serde_json::json!({
                    "tick": tick, "second": i as f64 / 120.0, "id": p.id,
                    "source": p.source.0, "definition": p.definition,
                    "origin": p.origin, "position": p.position, "velocity": p.velocity,
                    "images_at_observation": weapons.images.get(&p.source.0),
                    "mounted_at_observation": s.mounted(p.source.0),
                }));
            }
        }
        let new_damage = s.damage_recorded() - damage_read;
        let recent: Vec<_> = s.damage_results().collect();
        ensure!(
            new_damage <= recent.len() as u64,
            "damage observations were lost"
        );
        for d in recent.iter().skip(recent.len() - new_damage as usize) {
            damage.push(serde_json::json!({
                "tick": d.tick, "second": i as f64 / 120.0, "victim": d.victim,
                "source": d.source, "amount": d.amount, "projectile": d.projectile,
            }));
        }
        damage_read = s.damage_recorded();
        for d in s.death_results() {
            if seen_deaths.insert((d.victim, d.tick, format!("{:?}", d.life))) {
                deaths.push(serde_json::json!({
                    "tick": d.tick, "second": i as f64 / 120.0, "victim": d.victim,
                    "killer": d.killer, "game": d.game, "round": d.round, "life": format!("{:?}", d.life),
                }));
            }
        }
        for r in s.round_results() {
            if seen_rounds.insert((r.game, r.round, r.tick)) {
                rounds.push(serde_json::json!({
                    "tick": r.tick, "second": i as f64 / 120.0, "game": r.game,
                    "round": r.round, "players": r.players, "owners": r.owners, "teams": r.teams,
                }));
            }
        }
        if i % 120 == 0 {
            let thoughts = s.bot_thoughts();
            let motion = s.motion_states();
            let vitals = s.vitals();
            let inventories = s.tool_inventories();
            for t in &thoughts {
                let state = motion
                    .iter()
                    .find(|(p, _)| p.owner == t.bot)
                    .map(|(p, _)| p);
                bot_samples.push(serde_json::json!({
                    "second": i / 120, "id": t.bot, "state": state, "vitals": vitals.get(&t.bot),
                    "behaviour": t.behaviour, "leg": t.leg, "visible": t.visible,
                    "remembered": format!("{:?}", t.remembered), "task": format!("{:?}", t.task),
                    "goal": t.goal, "next": t.next, "route_end": t.route_end,
                    "short": t.route_short, "steps": t.path_steps, "searching": t.searching,
                    "search_phase": t.search_phase, "objective": t.objective,
                    "objective_diagnostic": t.objective_diagnostic, "objective_detail": t.objective_detail.as_ref().map(|d| serde_json::json!({
                        "desired": d.desired, "action": d.action, "provider": d.provider, "phase": d.phase, "route": d.route,
                    })),
                    "acted": t.acted, "planned": format!("{:?}", t.planned), "gate": t.gate,
                    "walking_intent": t.input, "driving": t.driving,
                    "sight_starved": t.sight_starved, "unnoticed_hit": t.unnoticed_hit,
                    "inventory": inventories.get(&t.bot), "images": weapons.images.get(&t.bot),
                    "goof": t.surprise.interrupt, "surprise_gate": t.surprise.gate,
                    "decisions": t.surprise.decisions.iter().map(|d| serde_json::json!({
                        "domain": d.domain, "tick": d.tick, "plain": d.plain, "chosen": d.chosen,
                        "varied": d.varied, "reason": d.reason,
                    })).collect::<Vec<_>>(),
                    "team": {"allies": t.team.allies, "terms": t.team.terms, "said": t.team.said},
                }));
            }
            let infos = s.vehicle_infos();
            for p in s.vehicle_poses() {
                let info = infos.iter().find(|v| v.id == p.id).unwrap();
                samples.push(serde_json::json!({
                    "second": i / 120, "id": p.id, "definition": info.definition,
                    "position": p.position, "rotation": p.rotation, "velocity": p.velocity, "wheel_contact": p.wheel_contact, "wheel_suspension": p.wheel_suspension,
                    "occupants": info.occupants,
                    "bots": thoughts.iter().filter(|t| s.mounted(t.bot).is_some_and(|(id,_)| id == p.id)).map(|t| serde_json::json!({
                        "id": t.bot, "behaviour": t.behaviour, "goal": t.goal, "next": t.next, "route_end": t.route_end, "objective_diagnostic": t.objective_diagnostic, "objective_detail": format!("{:?}", t.objective_detail),
                        "short": t.route_short, "steps": t.path_steps, "searching": t.searching, "acted": t.acted, "driving": t.driving,
                        "navigation": nav_detail.then(|| s.bot_drive_diagnostic(t.bot)).flatten(),
                    })).collect::<Vec<_>>(),
                }));
            }
        }
    }
    eprintln!("games={:?} chat={:?}", s.minigame_views(), s.chat());
    ensure!(!samples.is_empty(), "the saved match spawned no vehicles");
    let (failures, details) = s.bot_step_failures();
    ensure!(failures == 0, "bot control failures: {details:?}");
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &serde_json::json!({"world": world_name, "bricks": bricks, "seconds": seconds, "bot_step_failures": failures, "samples": samples, "bot_samples": bot_samples, "projectiles": projectiles, "damage": damage, "deaths": deaths, "rounds": rounds, "games": s.minigame_views(), "packages": packages, "rules_state": s.package_store(), "host": "passive owner, saved rules preserved", "chat": format!("{:?}", s.chat())}),
        )?,
    )?;
    Ok(())
}
