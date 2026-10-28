use super::*;
use crate::{
    definitions::{Definitions, Special},
    testing::definition,
};
use rapier3d::prelude::*;

fn session() -> (Session, OwnerId) {
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
    assert!(s.live_damage_brick(foreign));
}

fn settle(s: &mut Session) {
    for _ in 0..200 {
        s.step_damage_support().unwrap();
        if s.destruction.pending.is_empty()
            && s.destruction.searches.is_empty()
            && s.destruction.detach.is_none()
        {
            return;
        }
    }
    panic!("damage scheduler did not settle");
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
fn admitted_damage_keeps_its_owner_permissions_after_shooter_disconnects() {
    let (mut s, owner) = session();
    let brick = place(&mut s, owner, "1x1x3", [0.25, 0.3, 0.25], false);
    s.blow_up_bricks(owner, Some(brick), Vec3::new(0.25, 0.3, 0.25), &impact(0.))
        .unwrap();
    s.disconnect(owner).unwrap();
    settle(&mut s);
    assert!(!s.live_damage_brick(brick));
}
