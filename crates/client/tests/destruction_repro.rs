//! Read-only external-save repro: stock rocket commands feed authoritative
//! section cues through the ordinary client query/debris paths, without a window.
use anyhow::{Context, Result, ensure};
use bri_client::{
    brick_debris::BrickDebris, building::Building, content::ClientContent, network::WorldChanges,
    old_saves::Converter,
};
use bri_net::protocol::{PublicWorld, public_bricks};
use bri_sim::{
    player::MoveInput,
    session::{ActionAim, Command},
};
use glam::Vec3;
use serde_json::json;
#[path = "support/destruction_capture.rs"]
mod capture;
use std::{collections::BTreeSet, io::Write, path::PathBuf, time::Instant};

struct Case {
    name: &'static str,
    save: &'static str,
    map: &'static str,
    feet: [f32; 3],
    camera: [f32; 3],
    yaw: f32,
    pitch: f32,
}

#[test]
#[ignore = "needs regenerated content and Maxwell's two private rocket repro saves"]
fn two_stock_rockets_replay_authority_and_client_section_motion() -> Result<()> {
    let hz: u32 = std::env::var("BRI_DESTRUCTION_HZ")
        .unwrap_or_else(|_| "60".into())
        .parse()
        .context("BRI_DESTRUCTION_HZ must be 30, 60 or 120")?;
    ensure!(
        matches!(hz, 30 | 60 | 120),
        "BRI_DESTRUCTION_HZ must be 30, 60 or 120"
    );
    let limit: Option<usize> = std::env::var("BRI_DESTRUCTION_LIMIT")
        .ok()
        .map(|v| {
            v.parse()
                .context("BRI_DESTRUCTION_LIMIT must be an integer")
        })
        .transpose()?;
    ensure!(
        limit.is_none_or(|limit| limit <= bri_client::brick_debris::MAX_LIMIT),
        "BRI_DESTRUCTION_LIMIT exceeds the supported quality range"
    );
    let root = PathBuf::from(std::env::var("BRI_CONTENT").context("set BRI_CONTENT")?);
    let saves =
        PathBuf::from(std::env::var("BRI_DESTRUCTION_SAVES").context("set BRI_DESTRUCTION_SAVES")?);
    let out =
        PathBuf::from(std::env::var("BRI_DESTRUCTION_OUT").context("set BRI_DESTRUCTION_OUT")?);
    std::fs::create_dir_all(&out)?;
    let content = ClientContent::load(&root)?;
    let converter = Converter::new(&content)?;
    let pack: bri_weapons::Pack =
        serde_json::from_slice(&std::fs::read(content.paths.weapons.join("weapons.json"))?)?;
    pack.validate()?;
    for case in [
        Case {
            name: "tower",
            save: "Jetpuff's Towers.bls",
            map: "v20/add-ons/map_bedroom/bedroom.mis",
            feet: [53.7488, 286.4093, 90.1197],
            camera: [53.6246, 288.5658, 90.1868],
            yaw: -2.066252,
            pitch: 0.185,
        },
        Case {
            name: "pirate",
            save: "Pirate World.bls",
            map: "v20/add-ons/map_kitchen/kitchen.mis",
            feet: [-409.0501, 173.01, -24.2240],
            camera: [-409.0243, 175.1665, -24.3628],
            yaw: 0.183740,
            pitch: 0.005,
        },
    ] {
        let build =
            converter.convert(&std::fs::read(saves.join(case.save))?, case.name, case.map)?;
        let initial_bricks = build.bricks.len();
        let schema =
            bri_world::World::new(case.name.into(), case.map.into(), build.palette.clone());
        let mut host =
            bri_net::dedicated::load_packages(&root, &content.paths.packages, schema)?.session;
        host.set_lan_host(true); // The same single-player/LAN authority as a local host.
        let owner = host.join("Rocket repro".into(), Vec3::from(case.feet), true)?;
        let mut sequence = 1;
        host.command(
            owner,
            sequence,
            Command::LoadBuild {
                build: Box::new(bri_world::build::SavedBuild::new(build)),
                ownership: false,
            },
        )?;
        let slot = host.give_item(owner, "v20.weapon.rocketlauncheritem")?;
        sequence += 1;
        host.command(owner, sequence, Command::EquipTool { slot: Some(slot) })?;
        let loaded = content.paths.load_map(case.map, None)?;
        let mut building = Building::new(
            host.simulation().definitions.clone(),
            loaded.query_colliders,
        )?;
        building.set_breakables(&loaded.breakables);
        building.attach_terrain(loaded.terrain);
        let public = |host: &bri_sim::session::Session| {
            let state = host.simulation().state();
            PublicWorld {
                name: state.name.clone(),
                map_id: state.map_id.clone(),
                palette: state.palette.clone(),
                bricks: public_bricks(&state.bricks),
            }
        };
        let mut world = public(&host);
        building.sync_world(&world)?;
        building.set_world_tick(host.simulation().state().tick);
        let mut debris = BrickDebris::new();
        if let Some(limit) = limit {
            debris.set_limit(limit);
        }
        let meshes = host
            .simulation()
            .definitions
            .entries
            .iter()
            .map(|(id, d)| (id.clone(), d.mesh.clone()))
            .collect();
        let mut capture = std::env::var_os("BRI_DESTRUCTION_RENDER")
            .map(|_| capture::Capture::new(&content, meshes))
            .transpose()?;
        let mut max_debt = 0.0_f32;

        let mut trace = std::io::BufWriter::new(std::fs::File::create(
            out.join(format!("{}-frames.jsonl", case.name)),
        )?);
        let mut cues_out = std::io::BufWriter::new(std::fs::File::create(
            out.join(format!("{}-cues.jsonl", case.name)),
        )?);
        let aim = ActionAim {
            yaw: case.yaw,
            pitch: case.pitch,
        };
        let mut movement = 0;
        let mut shots = vec![];
        let mut projectiles = BTreeSet::new();
        let mut section_groups = BTreeSet::new();
        let mut shots_at = vec![];
        // Cover the ordinary 10-second solid and 3-second fade lifecycle,
        // after both stock image fire/reload cycles. Server ticks remain fixed;
        // only presentation cadence and the existing quality budget vary.
        for frame in 0..hz * 20 {
            let mut cues = vec![];
            for _ in 0..bri_world::TICKS_PER_SECOND as u32 / hz {
                movement += 1;
                host.movement(
                    owner,
                    movement,
                    MoveInput {
                        yaw: case.yaw,
                        pitch: case.pitch,
                        ..Default::default()
                    },
                )?;
                let ready = host
                    .weapon_view()
                    .images
                    .get(&owner)
                    .and_then(|images| images.iter().find(|i| i.hand == 0))
                    .and_then(|image| {
                        pack.images
                            .get(&image.image)
                            .and_then(|d| d.states.iter().find(|s| s.name == image.state))
                    })
                    .is_some_and(|state| state.down.is_some());
                if shots.len() < 2 && ready {
                    let player = host
                        .snapshot()
                        .players
                        .into_iter()
                        .find(|p| p.owner == owner)
                        .context("missing shooter")?;
                    let eye = player.eye(host.player_tuning(owner).context("missing tuning")?);
                    shots_at.push(json!({"feet":player.feet,"eye":eye.to_array(),"expected_camera":case.camera}));
                    for down in [true, false] {
                        sequence += 1;
                        host.command_with_aim(
                            owner,
                            sequence,
                            Command::WeaponTrigger { down },
                            Some(aim),
                        )?;
                    }
                    shots.push(host.simulation().state().tick);
                }
                host.step()?;
                for p in host.weapon_view().fired() {
                    projectiles.insert(p.id);
                }
                cues.extend(host.take_cues());
            }
            let tick = host.simulation().state().tick;
            let current = public(&host);
            let changed: BTreeSet<_> = world
                .bricks
                .keys()
                .chain(current.bricks.keys())
                .copied()
                .filter(|id| world.bricks.get(id) != current.bricks.get(id))
                .collect();
            if !changed.is_empty() || world.palette != current.palette {
                building.sync_world_changes(
                    &current,
                    Some(&WorldChanges {
                        bricks: changed,
                        palette: world.palette != current.palette,
                    }),
                )?;
                debris.sync_world(&current);
            }
            world = current;
            building.set_world_tick(tick);
            for cue in &cues {
                debris.explosion_cue(cue, &pack);
                writeln!(cues_out, "{}", serde_json::to_string(cue)?)?;
            }
            debris.cues(&cues, &building)?;
            let before = Instant::now();
            if let Some(c) = &capture {
                debris.prepare_models(&c.models);
            }
            debris.advance(1.0 / hz as f32, &building)?;
            let advance_ms = before.elapsed().as_secs_f64() * 1000.0;
            if let Some(c) = &mut capture {
                c.upload(&debris, &world.palette)?;
                if [hz / 2, hz, hz * 3 / 2, hz * 2, hz * 3, hz * 4, hz * 6].contains(&frame) {
                    c.save(
                        &world,
                        case.camera,
                        case.yaw,
                        case.pitch,
                        &out.join(format!("{}-{frame:04}.png", case.name)),
                    )?;
                }
            }
            let sections: Vec<_> = debris.section_snapshots().collect();
            for s in &sections {
                for chunk in &s.chunks {
                    max_debt = max_debt.max(chunk.debt);
                }
                if s.chunks.is_empty() && s.shown > 0.0 && s.impulse.is_none() {
                    ensure!(
                        s.rotation == [0., 0., 0., 1.],
                        "unsupported assembly acquired an invented hinge"
                    );
                }
            }
            for s in &sections {
                section_groups.insert(s.key);
            }
            let work = host.destruction_work();
            writeln!(
                trace,
                "{}",
                json!({"frame":frame,"tick":tick,"advance_ms":advance_ms,
                "preparation_work":debris.diagnostics.section_preparation_work,
                "motion_work":debris.diagnostics.section_motion_work,
                "fallback_steps":debris.diagnostics.section_fallback_steps,
                "sections":sections,"source_bricks":world.bricks.len(),"debris":debris.len(),
                "support_visits":work.support_visits,"query_work":work.query_work,"detached":work.detached_bricks})
            )?;
        }
        // The old paused moving queries saturated the 100-ms debt cap for
        // seconds. These saved workloads must make progress within that cap.
        if let Some(c) = &capture {
            ensure!(
                c.models.diagnostics.looks_built > 0,
                "GPU repro never uploaded debris geometry"
            );
        }
        trace.flush()?;
        cues_out.flush()?;
        let report = json!({"case":case.name,"save":case.save,"map":case.map,"initial_bricks":initial_bricks,"client_hz":hz,
            "shots":shots,"shot_poses":shots_at,"projectiles":projectiles,"groups":section_groups,
            "dropped_cues":host.dropped_cues(),"limit":debris.limit(),"max_debt":max_debt,"rendered":capture.is_some()});
        std::fs::write(
            out.join(format!("{}-summary.json", case.name)),
            serde_json::to_vec_pretty(&report)?,
        )?;
        eprintln!("DESTRUCTION {report}");
        ensure!(
            max_debt < 0.1 - f32::EPSILON,
            "fallback motion saturated its catch-up window: {max_debt}"
        );
        ensure!(
            shots.len() == 2 && projectiles.len() == 2,
            "the repro must launch exactly two stock rockets"
        );
        ensure!(
            !section_groups.is_empty(),
            "the supplied view did not detach a section"
        );
        ensure!(
            host.dropped_cues() == 0,
            "the repro dropped authoritative cues"
        );
    }
    Ok(())
}
