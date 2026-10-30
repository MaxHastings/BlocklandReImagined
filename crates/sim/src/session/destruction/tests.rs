use super::*;
use crate::{
    definitions::{Definitions, Special},
    testing::definition,
};
use rapier3d::prelude::*;

fn session() -> (Session, OwnerId) {
    session_with_vehicles(false)
}

fn session_with_vehicles(vehicles: bool) -> (Session, OwnerId) {
    let mut definitions = Definitions {
        entries: Default::default(),
    };
    for x in [1, 2, 4, 8, 16] {
        for z in [1, 2, 4, 8, 16] {
            for h in [1, 3, 6, 12, 24] {
                let id = format!("{x}x{z}x{h}");
                definitions
                    .entries
                    .insert(id.clone(), definition(&id, [x, z], h, Special::None, false));
            }
        }
    }
    let mut session = Session::new(
        Simulation::new(
            bri_world::World::new("Destruction".into(), "test".into(), vec![[1.0; 4]; 2]),
            definitions,
            vec![
                ColliderBuilder::cuboid(100.0, 0.5, 100.0).translation(Vector::new(0.0, -0.5, 0.0)),
            ],
        )
        .unwrap(),
    );
    if vehicles {
        session
            .set_vehicle_pack(bri_vehicles::testing::pack(), vec![])
            .unwrap();
    }
    session.set_lan_host(true);
    let owner = session
        .join("Builder".into(), Vec3::new(-10.0, 0.05, -10.0), false)
        .unwrap();
    (session, owner)
}

fn place(s: &mut Session, owner: OwnerId, definition: &str, at: [f32; 3], base: bool) -> BrickId {
    let mut brick = bri_world::Brick::new(ContentRef::Resolved(definition.into()), at, owner);
    brick.base_plate = base;
    s.simulation.insert_pieces(vec![brick]).unwrap()[0]
}

fn impact(radius: f32) -> bri_weapons::BrickImpact {
    bri_weapons::BrickImpact {
        direct: true,
        radius,
        max_volume: 6.0,
        max_floating_volume: 6.0,
        force: 10.0,
    }
}

fn volume(s: &Session, definition: &ContentRef) -> u64 {
    let ContentRef::Resolved(id) = definition else {
        panic!("resolved brick")
    };
    let m = &s.simulation.definitions.entries[id].mesh;
    u64::from(m.footprint_studs[0]) * u64::from(m.footprint_studs[1]) * u64::from(m.height_plates)
}

#[test]
fn integrated_split_repeated_hit_save_and_respawn_preserve_original_volume() {
    let (mut s, owner) = session();
    let original = place(&mut s, owner, "8x8x3", [0.0, 0.3, 0.0], false);
    s.blow_up_bricks(owner, None, Vec3::new(1.5, 0.3, -2.0), &impact(0.6))
        .unwrap();
    settle(&mut s);
    assert!(!s.live_damage_brick(original));
    let cues = s.take_cues();
    let thrown: u64 = cues
        .iter()
        .filter_map(|c| match &c.kind {
            CueKind::BrickKill {
                definition, death, ..
            } if *death != BrickDeath::Partition => Some(volume(&s, definition)),
            _ => None,
        })
        .sum();
    let remaining: u64 = s
        .simulation
        .state()
        .bricks
        .values()
        .filter(|b| b.colliding)
        .map(|b| volume(&s, &b.definition))
        .sum();
    assert!(thrown > 0 && remaining > 0);
    assert_eq!(thrown + remaining, 8 * 8 * 3);
    assert_eq!(s.saved_world().bricks.len(), 1);
    assert!(s.saved_world().bricks[&original].colliding);
    let piece = s.simulation.damage_pieces().next().unwrap();
    let blast = crate::session::debris::BrickBlast {
        origin: Vec3::ZERO,
        force: 10.0,
        radius: 0.02,
        struck: true,
    };
    s.knock_out_bricks(owner, &[(piece, blast)]).unwrap();
    settle(&mut s);
    assert!(!s.simulation.state().bricks.contains_key(&piece));
    assert!(!s.events.respawns.contains_key(&piece));
    s.events.respawns.insert(original, 0);
    s.step_events(&BTreeSet::new()).unwrap();
    assert_eq!(s.simulation.state().bricks.len(), 1);
    assert!(s.live_damage_brick(original));
    assert_eq!(s.simulation.damage_pieces().count(), 0);
    assert!(s.simulation.damage_flags(original).is_none());
}

#[test]
fn upper_baseplate_stays_on_one_support_then_falls_and_floating_root_stays() {
    let (mut s, owner) = session();
    let a = place(&mut s, owner, "1x1x3", [0.25, 0.3, 0.25], false);
    let b = place(&mut s, owner, "1x1x3", [-1.75, 0.3, -1.75], false);
    let floor = place(&mut s, owner, "8x8x1", [0.0, 0.7, 0.0], true);
    let top = place(&mut s, owner, "1x1x3", [0.25, 1.1, 0.25], false);
    let floating = place(&mut s, owner, "1x1x1", [20.25, 5.1, 20.25], true);
    s.blow_up_bricks(owner, Some(a), Vec3::new(0.25, 0.3, 0.25), &impact(0.0))
        .unwrap();
    settle(&mut s);
    assert!(s.live_damage_brick(floor) && s.live_damage_brick(top));
    s.blow_up_bricks(owner, Some(b), Vec3::new(-1.75, 0.3, -1.75), &impact(0.0))
        .unwrap();
    settle(&mut s);
    assert!(!s.live_damage_brick(floor) && !s.live_damage_brick(top));
    let groups: Vec<_> = s
        .take_cues()
        .into_iter()
        .filter_map(|c| match c.kind {
            CueKind::BrickKill {
                death: BrickDeath::Section { group, .. },
                brick,
                ..
            } => Some((group, brick)),
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].0, groups[1].0);
    assert_eq!(
        groups.iter().map(|p| p.1).collect::<BTreeSet<_>>(),
        [floor, top].into()
    );
    s.blow_up_bricks(
        owner,
        Some(floating),
        Vec3::new(20.25, 5.1, 20.25),
        &impact(0.0),
    )
    .unwrap();
    settle(&mut s);
    assert!(s.live_damage_brick(floating));
    assert!(s.saved_world().bricks.values().all(|b| b.colliding));
}

#[test]
fn ground_category_protects_only_foundations_and_permissions_still_apply() {
    let (mut s, owner) = session();
    s.tool_catalog
        .brick_names
        .insert("8x8x1".into(), "Baseplates/Plates/8x8".into());
    s.tool_catalog
        .brick_names
        .insert("8x8x3".into(), "Bricks/Large/8x8".into());
    s.tool_catalog.foundation_bricks.insert("8x8x3".into()); // Cubes share this catalog category.
    let plate = place(&mut s, owner, "8x8x1", [0.0, 0.1, 0.0], true);
    let block = place(&mut s, owner, "8x8x3", [10.0, 0.3, 0.0], true);
    s.tool_catalog.foundation_bricks.insert("8x8x1".into());
    s.blow_up_bricks(owner, Some(plate), Vec3::new(0., 0.1, 0.), &impact(0.))
        .unwrap();
    settle(&mut s);
    assert!(s.protected_foundation(plate));
    assert!(!s.protected_foundation(block));
    s.blow_up_bricks(owner, None, Vec3::new(11.5, 0.3, -2.0), &impact(0.6))
        .unwrap();
    settle(&mut s);
    assert!(!s.live_damage_brick(block));
    s.set_lan_host(false);
    let other = s
        .join("Other".into(), Vec3::new(-20.0, 0.05, -10.0), false)
        .unwrap();
    let foreign = place(&mut s, other, "1x1x3", [-10.25, 0.3, 10.25], false);
    s.blow_up_bricks(
        owner,
        Some(foreign),
        Vec3::new(-10.25, 0.3, 10.25),
        &impact(0.0),
    )
    .unwrap();
    settle(&mut s);
    assert!(s.live_damage_brick(foreign));
}

fn settle(s: &mut Session) {
    for _ in 0..200 {
        s.step_damage_support().unwrap();
        if s.destruction.pending.is_empty()
            && s.destruction.searches.is_empty()
            && s.destruction.detach.is_empty()
        {
            return;
        }
    }
    panic!("damage scheduler did not settle");
}

#[test]
fn response_timing_includes_support_proof_and_complete_section_delivery() {
    for radius_rocket in [false, true] {
        for layers in [1, 24, 188] {
            let (mut s, owner) = session();
            let support = place(&mut s, owner, "1x1x3", [0.25, 0.3, 0.25], false);
            place(&mut s, owner, "16x16x1", [0., 0.7, 0.], true);
            let bricks = (0..layers)
                .flat_map(|y| {
                    (0..8).flat_map(move |x| {
                        (0..8).map(move |z| {
                            bri_world::Brick::new(
                                ContentRef::Resolved("2x2x3".into()),
                                [-3.5 + x as f32, 1.1 + y as f32 * 0.6, -3.5 + z as f32],
                                owner,
                            )
                        })
                    })
                })
                .collect();
            s.simulation.insert_pieces(bricks).unwrap();
            s.step_damage_support().unwrap();
            s.take_cues();
            let parameters = if radius_rocket {
                bri_weapons::BrickImpact {
                    direct: false,
                    radius: 3.,
                    max_volume: 30.,
                    max_floating_volume: 60.,
                    force: 30.,
                }
            } else {
                impact(0.)
            };
            s.blow_up_bricks(
                owner,
                (!radius_rocket).then_some(support),
                Vec3::new(0.25, 0.3, 0.25),
                &parameters,
            )
            .unwrap();
            let mut direct = None;
            let mut first_section = None;
            let mut ready = None;
            let mut micros = Vec::new();
            for tick in 1..=1200 {
                s.simulation.set_tick(tick);
                let before = s.destruction_work().query_work;
                let clock = std::time::Instant::now();
                s.step_damage_support().unwrap();
                micros.push(clock.elapsed().as_micros());
                assert!(s.destruction_work().query_work - before <= u64::from(WORK_PER_TICK));
                for cue in s.take_cues() {
                    match cue.kind {
                        CueKind::BrickKill { brick, death, .. }
                            if brick == support
                                && matches!(death, BrickDeath::Hit | BrickDeath::Blast) =>
                        {
                            direct.get_or_insert(tick);
                        }
                        CueKind::BrickKill {
                            death: BrickDeath::Section { .. },
                            ..
                        } => {
                            first_section.get_or_insert(tick);
                        }
                        CueKind::BrickSectionReady { .. } => {
                            ready = Some(tick);
                        }
                        _ => {}
                    }
                }
                if ready.is_some()
                    && s.destruction.pending.is_empty()
                    && s.destruction.searches.is_empty()
                    && s.destruction.detach.is_empty()
                {
                    break;
                }
            }
            eprintln!(
                "server collapse {} source members, radius-rocket={radius_rocket}: direct={direct:?}, first-section={first_section:?}, ready={ready:?} ticks; work={:?}",
                layers * 64 + 1,
                s.destruction_work()
            );
            assert!(
                direct.is_some_and(|tick| tick <= 2),
                "the struck support must break promptly"
            );
            assert!(ready.is_some(), "complete section never delivered");
            eprintln!(
                "server destruction step: mean {}us, max {}us",
                micros.iter().sum::<u128>() / micros.len() as u128,
                micros.iter().max().unwrap()
            );
            let limit = if layers == 1 {
                6
            } else if layers == 24 {
                24
            } else {
                120
            };
            assert!(
                ready.unwrap() <= limit,
                "large collapse delivery stalled: {ready:?} ticks > {limit}"
            );
        }
    }
}

#[test]
fn support_cursor_keeps_connections_inside_interlocking_mesh_bounds() {
    let (mut s, owner) = session();
    for (id, rows) in [("recess-a", ["-", "u", "-"]), ("recess-b", ["d", "-", "-"])] {
        let mut d = definition(id, [1, 1], 3, Special::None, false);
        d.mesh.attachment_rows = rows.into_iter().map(str::to_string).collect();
        s.simulation.definitions.entries.insert(id.into(), d);
    }
    let a = place(&mut s, owner, "recess-a", [0.25, 0.3, 0.25], false);
    let b = place(&mut s, owner, "recess-b", [0.25, 0.3, 0.25], false);
    let mut probe = SupportProbe::new(&s.simulation, a);
    let mut work = WORK_PER_TICK;
    assert!(probe.step(&s.simulation, &mut work, |_| true).unwrap());
    assert!(
        probe.neighbors.contains(&b),
        "recessed authored stud connection was lost"
    );
}

#[test]
fn a_hole_in_a_supported_slab_does_not_invent_material_seams() {
    let (mut s, owner) = session();
    place(&mut s, owner, "1x1x3", [-1.75, 0.3, -1.75], false);
    let roof = place(&mut s, owner, "8x8x1", [0., 0.7, 0.], true);
    s.blow_up_bricks(owner, None, Vec3::new(1., 0.7, 1.), &impact(0.3))
        .unwrap();
    settle(&mut s);
    assert!(!s.live_damage_brick(roof));
    assert!(
        s.simulation
            .damage_pieces()
            .any(|id| s.live_damage_brick(id))
    );
    assert!(
        !s.take_cues().iter().any(|c| matches!(
            c.kind,
            CueKind::BrickKill {
                death: BrickDeath::Section { .. },
                ..
            }
        )),
        "partition faces preserve the intact slab's lateral material"
    );
}

#[test]
fn original_flags_restore_and_deliberate_part_edits_commit_visible_geometry() {
    let (mut s, owner) = session();
    let root = place(&mut s, owner, "8x8x3", [0., 0.3, 0.], false);
    s.simulation
        .mutate(root, |b| {
            b.visible = false;
            b.raycast = false;
        })
        .unwrap();
    s.blow_up_bricks(owner, None, Vec3::new(1.5, 0.3, -2.), &impact(0.6))
        .unwrap();
    settle(&mut s);
    assert!(
        s.simulation
            .damage_pieces()
            .all(|id| !s.simulation.state().bricks[&id].visible
                && !s.simulation.state().bricks[&id].raycast)
    );
    s.respawn_bricks(&[root]).unwrap();
    let b = &s.simulation.state().bricks[&root];
    assert!(!b.visible && !b.raycast && b.colliding && !b.damage_hidden);
    s.simulation
        .mutate(root, |b| {
            b.visible = true;
            b.raycast = true;
        })
        .unwrap();
    s.blow_up_bricks(owner, None, Vec3::new(1.5, 0.3, -2.), &impact(0.6))
        .unwrap();
    settle(&mut s);
    let part = s.simulation.damage_pieces().next().unwrap();
    s.simulation.mutate(part, |b| b.color = 1).unwrap();
    s.sync_damage_lifecycle();
    assert!(!s.simulation.state().bricks.contains_key(&root));
    assert_eq!(s.simulation.damage_part_count(), 0);
    assert!(!s.events.respawns.contains_key(&root));
    assert_eq!(s.saved_world().bricks[&part].color, 1);
}

#[test]
fn deleting_an_original_removes_its_entire_family_and_marks_replica_changes() {
    let (mut s, owner) = session();
    let root = place(&mut s, owner, "8x8x3", [0., 0.3, 0.], false);
    s.blow_up_bricks(owner, None, Vec3::new(1.5, 0.3, -2.), &impact(0.6))
        .unwrap();
    settle(&mut s);
    let parts: Vec<_> = s.simulation.damage_pieces().collect();
    assert!(!parts.is_empty());
    s.take_dirty();
    s.simulation
        .remove_many(
            &Actor {
                administrator: true,
                ..Default::default()
            },
            &[root],
        )
        .unwrap();
    let dirty = s.take_dirty();
    assert!(
        parts
            .iter()
            .all(|p| dirty.contains(p) && !s.simulation.state().bricks.contains_key(p))
    );
    assert!(!s.events.respawns.contains_key(&root));
}

#[test]
fn exhausted_partition_budget_holds_the_block_instead_of_amplifying_the_blast() {
    let (mut s, owner) = session();
    let root = place(&mut s, owner, "8x8x3", [0., 0.3, 0.], false);
    s.blow_up_bricks(owner, None, Vec3::new(1.5, 0.3, -2.), &impact(0.6))
        .unwrap();
    s.destruction.pending.front_mut().unwrap().made = PIECES_PER_BLAST;
    settle(&mut s);
    assert!(s.live_damage_brick(root));
    assert!(s.take_cues().is_empty());
    assert!(s.destruction.work.capacity_holds > 0);
}

#[test]
fn temporary_split_allowances_defer_without_consuming_or_mutating_the_hit() {
    let (mut s, owner) = session();
    let root = place(&mut s, owner, "8x8x3", [0., 0.3, 0.], false);
    s.blow_up_bricks(owner, None, Vec3::new(1.5, 0.3, -2.), &impact(0.6))
        .unwrap();
    let mut job = s.destruction.pending.pop_front().unwrap();
    let mut probe = SupportProbe::new(&s.simulation, root);
    let mut work = 100_000;
    assert!(
        probe
            .step(&s.simulation, &mut work, |id| s.live_damage_brick(id))
            .unwrap()
    );
    let blast = job.blast;
    for (pieces, bytes) in [(0, MUTATION_BYTES_PER_TICK), (PIECES_PER_BLAST, 0)] {
        s.destruction.tick_pieces_left = pieces;
        s.destruction.mutation_bytes_left = bytes;
        assert!(
            s.apply_damage(&mut job, root, &probe, blast)
                .unwrap()
                .is_none()
        );
        assert!(s.live_damage_brick(root));
        assert_eq!(s.simulation.damage_part_count(), 0);
        assert_eq!(job.made, 0);
        assert!(s.take_cues().is_empty());
    }
    s.destruction.tick_pieces_left = PIECES_PER_BLAST;
    s.destruction.mutation_bytes_left = MUTATION_BYTES_PER_TICK;
    for _ in 0..crate::presentation::MAX_CUES {
        s.cues.emit(0, CueKind::Plant, [0.; 3]);
    }
    let cursor = s.cues.cursor();
    assert!(
        s.apply_damage(&mut job, root, &probe, blast)
            .unwrap()
            .is_none()
    );
    assert_eq!(s.cues.cursor(), cursor, "retry must not drop or add a cue");
    assert!(s.live_damage_brick(root));
    assert_eq!(job.made, 0);
    s.take_cues();
    assert!(
        s.apply_damage(&mut job, root, &probe, blast)
            .unwrap()
            .is_some()
    );
    assert!(
        !s.live_damage_brick(root),
        "the retained hit must work with fresh allowance"
    );
    assert!(job.made > 0);
    assert!(!s.take_cues().is_empty());
}

#[test]
fn admitted_damage_keeps_its_owner_permissions_after_shooter_disconnects() {
    let (mut s, owner) = session();
    let brick = place(&mut s, owner, "1x1x3", [0.25, 0.3, 0.25], false);
    s.blow_up_bricks(owner, Some(brick), Vec3::new(0.25, 0.3, 0.25), &impact(0.))
        .unwrap();
    s.disconnect(owner).unwrap();
    settle(&mut s);
    assert!(!s.live_damage_brick(brick));
}

#[test]
fn independent_sections_share_commit_budget_and_report_complete_shape() {
    let (mut s, owner) = session();
    let scope = s.damage_scope(owner, Vec3::ZERO).unwrap();
    for group in 1..=2 {
        let ids: BTreeSet<_> = (0..160)
            .map(|i| {
                place(
                    &mut s,
                    owner,
                    "1x1x3",
                    [group as f32 * 10. + 0.25, 5.3 + i as f32 * 0.6, 0.25],
                    false,
                )
            })
            .collect();
        s.destruction.detach.push_back(Detached {
            scope: scope.clone(),
            ids: Arc::new(ids),
            next: None,
            group,
            started: 0,
            origin: Vec3::new(group as f32 * 10., 53., 0.),
            half: Vec3::new(0.25, 48., 0.25),
            pivot: Vec3::new(group as f32 * 10., 5., 0.),
            members: 160,
            area: None,
        });
    }
    let _ = s.simulation.take_support_edits();
    s.run_damage_support().unwrap();
    let first = s.take_cues();
    let mut groups = BTreeMap::new();
    for c in &first {
        c.validate().unwrap();
        if let CueKind::BrickKill {
            death:
                BrickDeath::Section {
                    group,
                    members,
                    half,
                    pivot,
                    ..
                },
            ..
        } = c.kind
        {
            assert_eq!(members, 160);
            assert_eq!(half, [0.25, 48., 0.25]);
            assert_eq!(pivot[1], 5.);
            *groups.entry(group).or_insert(0) += 1;
        }
    }
    assert_eq!(
        groups.len(),
        2,
        "a second proven group starts before the first drains"
    );
    assert_eq!(groups.values().sum::<usize>(), COMMIT_PER_TICK);
    let mut total = first
        .iter()
        .filter(|c| matches!(c.kind, CueKind::BrickKill { .. }))
        .count();
    let mut barriers = 0;
    while !s.destruction.detach.is_empty() {
        let before = s.destruction.work.detached_bricks;
        s.run_damage_support().unwrap();
        assert!(s.destruction.work.detached_bricks - before <= COMMIT_PER_TICK as u64);
        let cues = s.take_cues();
        total += cues
            .iter()
            .filter(|c| matches!(c.kind, CueKind::BrickKill { .. }))
            .count();
        barriers += cues
            .iter()
            .filter(|c| matches!(c.kind, CueKind::BrickSectionReady { members: 160, .. }))
            .count();
    }
    assert_eq!(total, 320);
    assert_eq!(barriers, 2);
}

#[test]
fn section_barrier_seals_only_committed_material_and_defers_respawn_until_delivery_ends() {
    let (mut s, owner) = session();
    let mut scope = s.damage_scope(owner, Vec3::ZERO).unwrap();
    Arc::get_mut(&mut scope).unwrap().delay = 1;
    let count = COMMIT_PER_TICK + 32;
    let ids: BTreeSet<_> = (0..count)
        .map(|i| {
            place(
                &mut s,
                owner,
                "1x1x3",
                [0.25, 5.3 + i as f32 * 0.6, 0.25],
                false,
            )
        })
        .collect();
    let first = *ids.first().unwrap();
    s.destruction.detach.push_back(Detached {
        scope,
        ids: Arc::new(ids),
        next: None,
        group: 1,
        started: 0,
        origin: Vec3::new(0., 5. + count as f32 * 0.3, 0.),
        half: Vec3::new(0.25, count as f32 * 0.3, 0.25),
        pivot: Vec3::new(0., 5., 0.),
        members: count as u32,
        area: None,
    });
    let _ = s.simulation.take_support_edits();
    s.run_damage_support().unwrap();
    assert_eq!(s.simulation.section_due(first), Some(u64::MAX));
    assert!(
        !s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, CueKind::BrickSectionReady { .. }))
    );
    let interrupted = s.destruction.detach.pop_front().unwrap();
    assert_eq!(interrupted.remaining().count(), 32);
    assert!(interrupted.remaining().all(|&id| s.live_damage_brick(id)));
    s.finish_section(&interrupted);
    s.run_damage_support().unwrap();
    assert_eq!(s.simulation.section_due(first), Some(1));
    assert!(
        s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, CueKind::BrickSectionReady { members, .. } if members as usize == COMMIT_PER_TICK))
    );
    s.respawn_bricks(&[first]).unwrap();
    assert_eq!(s.simulation.section_due(first), None);
}

#[test]
fn exhausted_presentation_admission_holds_source_until_capacity_is_available() {
    let (mut s, owner) = session();
    let scope = s.damage_scope(owner, Vec3::ZERO).unwrap();
    let id = place(&mut s, owner, "1x1x3", [0.25, 5.3, 0.25], false);
    let mut search = Search::new(scope, Vec::new());
    search.push(id, &s.simulation);
    search.frontier.clear(); // This fixture starts after support proof.
    s.destruction.searches.push_back(search);
    s.destruction.reservations.insert(
        (0, 999),
        Reservation {
            members: MAX_SECTION_MEMBERS,
            looks: 0,
            quads: 0,
            draw_quads: 0,
            buckets: 0,
            until: u64::MAX,
        },
    );
    let _ = s.simulation.take_support_edits();
    s.run_damage_support().unwrap();
    assert!(s.live_damage_brick(id));
    assert!(s.destruction.work.capacity_holds > 0);
    assert!(s.take_cues().is_empty());
    s.destruction.reservations.clear();
    s.run_damage_support().unwrap();
    assert!(!s.live_damage_brick(id));
    assert!(
        s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, CueKind::BrickSectionReady { members: 1, .. }))
    );
    s.respawn_bricks(&[id]).unwrap();
    assert!(
        s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, CueKind::BrickSectionRetired { group: 1, .. }))
    );
    assert!(s.destruction.reservations.is_empty());
}

#[test]
fn many_small_collapses_do_not_hold_a_new_section_for_the_lease() {
    let (mut s, owner) = session();
    let scope = s.damage_scope(owner, Vec3::ZERO).unwrap();
    // Simulate a busy match's recent small collapses. Their combined material
    // is tiny, but the old independent 64-group admission cap held the next
    // small structure standing until a thirty-second reservation expired.
    for group in 1..=64 {
        s.destruction.reservations.insert(
            (0, group),
            Reservation {
                members: 1,
                looks: 1,
                quads: 6,
                draw_quads: 6,
                buckets: 1,
                until: PRESENTATION_LEASE,
            },
        );
    }
    s.destruction.next_group = 64;
    let id = place(&mut s, owner, "1x1x3", [0.25, 5.3, 0.25], false);
    let mut search = Search::new(scope, vec![id]);
    search.push(id, &s.simulation);
    s.destruction.searches.push_back(search);
    let _ = s.simulation.take_support_edits();
    s.run_damage_support().unwrap();
    assert!(
        !s.live_damage_brick(id),
        "a small unsupported section waited behind metadata reservations"
    );
    assert!(
        s.take_cues()
            .iter()
            .any(|c| matches!(c.kind, CueKind::BrickSectionReady { members: 1, .. }))
    );
    assert_eq!(s.destruction.work.capacity_holds, 0);
}

#[test]
fn timed_restore_keeps_an_occupied_family_intact_and_retries_after_bodies_leave() {
    for vehicle in [false, true] {
        let (mut s, owner) = session_with_vehicles(vehicle);
        let root = place(&mut s, owner, "8x8x12", [0., 1.2, 0.], false);
        let part = place(&mut s, owner, "1x1x3", [1.25, 0.3, 1.25], false);
        s.simulation.begin_damage_family(root);
        s.simulation.register_damage_parts(root, &[part]);
        let section = (0, 1);
        s.simulation.begin_section(
            section,
            Arc::new(BTreeSet::from([part])),
            Arc::new(BTreeSet::from([root])),
        );
        s.simulation.section_member(section, part);
        s.fake_kill_brick(
            root,
            super::super::debris::BrickBlast::fake_kill(Vec3::Y, Vec3::ZERO),
            1,
        )
        .unwrap();
        s.simulation.seal_section(section, 1);
        let clear = place(&mut s, owner, "8x8x12", [20., 1.2, 0.], false);
        s.fake_kill_brick(
            clear,
            super::super::debris::BrickBlast::fake_kill(Vec3::X * 20., Vec3::ZERO),
            1,
        )
        .unwrap();
        let occupant = if vehicle {
            let world = s.vehicles.world.as_mut().unwrap();
            world
                .spawn(
                    &mut s.simulation.physics,
                    bri_vehicles::Spawn {
                        id: bri_vehicles::VehicleId(1),
                        owner: bri_vehicles::OwnerId(owner),
                        definition: bri_vehicles::testing::CAR.into(),
                        transform: bri_vehicles::Transform {
                            position: [0., 0.2, 0.],
                            ..Default::default()
                        },
                        spawn_id: None,
                        respawn_ticks: None,
                        scale: 1.0,
                    },
                )
                .unwrap();
            for handle in world.colliders_of(&s.simulation.physics, bri_vehicles::VehicleId(1)) {
                s.simulation.physics.colliders[handle].user_data =
                    super::super::vehicles::VEHICLE_TAG | 1;
            }
            None
        } else {
            Some(
                s.join(
                    "In the former opening".into(),
                    Vec3::new(0., 0.05, 0.),
                    false,
                )
                .unwrap(),
            )
        };
        s.simulation.step().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(
            !s.live_damage_brick(root),
            "restored inside vehicle={vehicle}"
        );
        assert!(s.simulation.state().bricks.contains_key(&part));
        assert_eq!(s.simulation.damage_family_part_count(root), 1);
        assert!(!s.take_cues().iter().any(|cue| matches!(cue.kind,
            CueKind::BrickSectionRetired { started, group } if (started, group) == section
        )));
        assert!(
            s.live_damage_brick(clear),
            "one occupied restore cannot block unrelated work"
        );
        if let Some(occupant) = occupant {
            s.peers
                .get_mut(&occupant)
                .unwrap()
                .player
                .teleport(&mut s.simulation.physics, Vec3::new(10., 0.05, 0.), 0.)
                .unwrap();
        } else {
            s.vehicles
                .world
                .as_mut()
                .unwrap()
                .set_transform(
                    &mut s.simulation.physics,
                    bri_vehicles::VehicleId(1),
                    &bri_vehicles::Transform {
                        position: [10., 0.2, 0.],
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        s.simulation.step().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        assert!(s.live_damage_brick(root), "clear original was not retried");
        assert!(!s.simulation.state().bricks.contains_key(&part));
        assert!(s.simulation.damage_flags(root).is_none());
        assert!(s.take_cues().iter().any(|cue| matches!(cue.kind,
            CueKind::BrickSectionRetired { started, group } if (started, group) == section
        )));
    }
}

#[test]
fn timed_restore_admits_support_touch_openings_and_nonblocking_originals() {
    for case in ["support", "opening", "noncolliding", "water"] {
        let (mut s, owner) = session();
        let root = if case == "support" {
            place(&mut s, owner, "8x8x3", [0., 0.3, 0.], false)
        } else {
            if case == "opening" {
                s.simulation
                    .definitions
                    .entries
                    .get_mut("8x8x12")
                    .unwrap()
                    .shape = SharedShape::compound(vec![
                    (
                        Pose::translation(-1.8, 0., 0.),
                        SharedShape::cuboid(0.2, 1.2, 2.),
                    ),
                    (
                        Pose::translation(1.8, 0., 0.),
                        SharedShape::cuboid(0.2, 1.2, 2.),
                    ),
                ]);
            } else if case == "water" {
                s.simulation
                    .definitions
                    .entries
                    .get_mut("8x8x12")
                    .unwrap()
                    .special = Special::Water;
            }
            place(&mut s, owner, "8x8x12", [0., 1.2, 0.], false)
        };
        if case == "noncolliding" {
            s.simulation.mutate(root, |b| b.colliding = false).unwrap();
        }
        s.fake_kill_brick(
            root,
            super::super::debris::BrickBlast::fake_kill(Vec3::Y, Vec3::ZERO),
            1,
        )
        .unwrap();
        let height = if case == "support" { 0.6 } else { 0.05 };
        s.join(
            "Clear of actual restored solid".into(),
            Vec3::new(0., height, 0.),
            false,
        )
        .unwrap();
        s.simulation.step().unwrap();
        s.step_events(&BTreeSet::new()).unwrap();
        let brick = &s.simulation.state().bricks[&root];
        assert!(
            brick.visible && !brick.damage_hidden,
            "unnecessarily blocked {case}"
        );
        assert_eq!(
            brick.colliding,
            case != "noncolliding",
            "original flag changed in {case}"
        );
    }
}

#[test]
fn respawn_budget_charges_family_descendants_instead_of_only_originals() {
    let (mut s, owner) = session();
    let scope = s.damage_scope(owner, Vec3::ZERO).unwrap();
    let mut roots = Vec::new();
    for lane in 0..2 {
        let root = place(&mut s, owner, "8x8x3", [lane as f32 * 20., 0.3, 0.], false);
        s.simulation.begin_damage_family(root);
        let bricks = (0..1500)
            .map(|i| {
                bri_world::Brick::new(
                    ContentRef::Resolved("1x1x3".into()),
                    [
                        lane as f32 * 20. + 0.25 + (i % 20) as f32 * 0.5,
                        2.3 + (i / 20) as f32 * 0.6,
                        0.25,
                    ],
                    owner,
                )
            })
            .collect();
        let parts = s.simulation.insert_pieces(bricks).unwrap();
        s.simulation.register_damage_parts(root, &parts);
        s.hide_damage_bricks(&scope, &[root]).unwrap();
        s.events.respawns.insert(root, 0);
        roots.push(root);
    }
    s.step_events(&BTreeSet::new()).unwrap();
    assert!(s.live_damage_brick(roots[0]));
    assert!(!s.live_damage_brick(roots[1]));
    assert_eq!(s.simulation.damage_family_part_count(roots[1]), 1500);
    s.simulation.step().unwrap();
    s.step_events(&BTreeSet::new()).unwrap();
    assert!(s.live_damage_brick(roots[1]));
}

#[test]
fn shared_original_waits_for_every_section_and_sealing_does_not_restart_deadlines() {
    let (mut s, owner) = session();
    let root = place(&mut s, owner, "8x8x3", [0., 0.3, 0.], false);
    let a = place(&mut s, owner, "1x1x3", [0.25, 2.3, 0.25], false);
    let b = place(&mut s, owner, "1x1x3", [1.25, 2.3, 0.25], false);
    s.simulation.begin_damage_family(root);
    s.simulation.register_damage_parts(root, &[a, b]);
    for (group, id) in [(1, a), (2, b)] {
        s.simulation.begin_section(
            (0, group),
            Arc::new(BTreeSet::from([id])),
            Arc::new(BTreeSet::from([root])),
        );
        s.simulation.section_member((0, group), id);
    }
    s.simulation.seal_section((0, 1), 10);
    assert_eq!(s.simulation.section_due(root), Some(u64::MAX));
    s.simulation.seal_section((0, 2), 20);
    assert_eq!(s.simulation.section_due(root), Some(20));
    s.simulation.seal_section((0, 1), 30);
    assert_eq!(s.simulation.section_due(root), Some(20));
}

#[test]
fn common_edit_boundary_interrupts_pending_section_even_for_appearance_only_edits() {
    let (mut s, owner) = session();
    let id = place(&mut s, owner, "1x1x3", [0.25, 2.3, 0.25], false);
    let key = (0, 1);
    s.simulation.begin_section(
        key,
        Arc::new(BTreeSet::from([id])),
        Arc::new(BTreeSet::new()),
    );
    s.simulation.mutate(id, |b| b.color = 1).unwrap();
    assert!(s.simulation.section_interrupted(key));
    assert!(s.live_damage_brick(id));
    s.simulation.seal_section(key, 1);
    assert!(
        s.simulation
            .take_section_barriers(1)
            .iter()
            .any(|&(k, n, retired)| k == key && n == 0 && retired)
    );
}

#[test]
fn render_admission_counts_instanced_geometry_not_only_unique_meshes() {
    let (mut s, owner) = session();
    let template = crate::testing::portal("render-template", None).mesh.quads[0].clone();
    s.simulation
        .definitions
        .entries
        .get_mut("1x1x3")
        .unwrap()
        .mesh
        .quads = vec![template; crate::presentation::MAX_SECTION_LOOK_QUADS];
    let scope = s.damage_scope(owner, Vec3::ZERO).unwrap();
    let mut search = Search::new(scope, Vec::new());
    let ids: Vec<_> = (0..200)
        .map(|i| {
            place(
                &mut s,
                owner,
                "1x1x3",
                [0.25, 5.3 + i as f32 * 0.6, 0.25],
                false,
            )
        })
        .collect();
    for &id in &ids {
        search.push(id, &s.simulation);
    }
    search.frontier.clear();
    assert_eq!(search.looks.len(), 1);
    assert!(search.draw_quads > crate::presentation::MAX_SECTION_DRAW_QUADS);
    s.destruction.searches.push_back(search);
    let _ = s.simulation.take_support_edits();
    s.run_damage_support().unwrap();
    assert!(ids.iter().all(|&id| s.live_damage_brick(id)));
    assert!(s.take_cues().is_empty());
    assert!(s.destruction.work.capacity_holds > 0);
}
