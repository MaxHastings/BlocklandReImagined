//! Clips, reserves and reloads, driven by the Commando sample's rifle: the
//! same `weapons.json` an Add-On author writes, with every field it leaves
//! out taking its default.
use bri_weapons::*;
use glam::Vec3;
use std::path::Path;

struct Open;
impl Query for Open {
    fn sweep(&mut self, _: Vec3, _: Vec3, _: Filter) -> Option<Hit> {
        None
    }
    fn radius(&mut self, _: Vec3, _: f32, _: usize) -> Vec<Nearby> {
        Vec::new()
    }
    fn can_affect(&self, _: ActorId, _: TargetId) -> bool {
        true
    }
    fn can_catch(&self, _: ActorId, _: ActorId) -> bool {
        true
    }
}

const RIFLE: &str = "sample-commando-rifle:weapon/rifle";
const RIFLE_IMAGE: &str = "sample-commando-rifle:image/rifle";
const A: ActorId = ActorId(1);

fn rifle_pack() -> Pack {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/samples/sample-commando-rifle/assets/weapons.json");
    Pack::from_json(&std::fs::read(path).unwrap()).unwrap()
}

fn armed() -> WeaponsWorld {
    let mut w = WeaponsWorld::new(rifle_pack()).unwrap();
    w.add_actor(A, 5).unwrap();
    let slot = w.give(A, RIFLE).unwrap();
    w.equip(A, Some(slot)).unwrap();
    step(&mut w, 40);
    w
}

fn step(w: &mut WeaponsWorld, ticks: usize) -> Vec<Event> {
    (0..ticks).flat_map(|_| w.step(&mut Open)).collect()
}

fn shots(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::Spawned { .. }))
        .count()
}

/// Press and release the trigger once, then let the bolt cycle.
fn shoot(w: &mut WeaponsWorld) -> usize {
    w.trigger(A, true).unwrap();
    let mut events = step(w, 2);
    w.trigger(A, false).unwrap();
    events.extend(step(w, 60));
    shots(&events)
}

fn state(w: &WeaponsWorld) -> String {
    w.image_state(A, 0).unwrap().1.name.clone()
}

#[test]
fn an_authored_pack_fills_in_what_it_leaves_out() {
    let pack = rifle_pack();
    let image = &pack.images[RIFLE_IMAGE];
    assert_eq!(image.id, RIFLE_IMAGE, "ids come from their keys");
    assert_eq!(pack.items[RIFLE].id, RIFLE);
    assert!(pack.items[RIFLE].can_drop);
    assert!(image.states.iter().all(|s| s.wait && s.allow_change));
    assert_eq!(image.ammo.unwrap().magazine, 8);
    assert_eq!(image.zoom.unwrap().fov, 20.0);
    assert!(!image.zoom.unwrap().crosshair);
    assert!(image.crosshair);
    let round = &pack.projectiles["sample-commando-rifle:projectile/round"];
    assert_eq!(round.gravity, 1.0);
    assert!(round.collide_players);
    let shot = pack.sound("Sample-Commando-Rifle:Shot").unwrap();
    assert_eq!(shot.file, "sounds/shot.wav");
    assert!(pack.sound("sample-commando-rifle:empty").unwrap().local);
}

#[test]
fn firing_empties_the_clip_and_an_empty_clip_reloads_from_the_reserve() {
    let mut w = armed();
    assert_eq!(state(&w), "Ready");
    assert_eq!(
        w.held_rounds(A, 0),
        Some((
            Rounds {
                clip: 8,
                reserve: 24
            },
            8
        ))
    );
    for fired in 1..=7 {
        assert_eq!(shoot(&mut w), 1);
        assert_eq!(w.held_rounds(A, 0).unwrap().0.clip, 8 - fired);
    }
    // The last round: the clip empties and the reload starts by itself.
    assert_eq!(shoot(&mut w), 1);
    assert_eq!(state(&w), "Reload");
    // A pull mid-reload fires nothing.
    assert_eq!(shoot(&mut w), 0);
    step(&mut w, 150);
    assert_eq!(state(&w), "Ready");
    assert_eq!(
        w.held_rounds(A, 0).unwrap().0,
        Rounds {
            clip: 8,
            reserve: 16
        }
    );
}

#[test]
fn a_reload_is_asked_for_only_when_it_can_happen() {
    let mut w = armed();
    assert!(!w.request_reload(A).unwrap(), "a full clip has no room");
    assert_eq!(shoot(&mut w), 1);
    assert!(w.request_reload(A).unwrap());
    step(&mut w, 1);
    assert_eq!(state(&w), "Reload");
    // Switching away cancels the reload: no rounds move.
    w.equip(A, None).unwrap();
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 200);
    assert_eq!(w.held_rounds(A, 0).unwrap().0.clip, 7);
    assert!(w.request_reload(A).unwrap());
    step(&mut w, 160);
    assert_eq!(
        w.held_rounds(A, 0).unwrap().0,
        Rounds {
            clip: 8,
            reserve: 23
        }
    );
}

#[test]
fn a_dry_clip_clicks_until_rounds_arrive_and_new_items_start_full() {
    let mut w = armed();
    w.add_reserve(A, RIFLE, -24).unwrap();
    for _ in 0..8 {
        assert_eq!(shoot(&mut w), 1);
    }
    assert_eq!(state(&w), "Empty");
    w.trigger(A, true).unwrap();
    let events = step(&mut w, 2);
    assert_eq!(shots(&events), 0);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Sound { profile, .. } if profile == "sample-commando-rifle:empty"
    )));
    w.trigger(A, false).unwrap();
    step(&mut w, 40);
    assert_eq!(w.add_reserve(A, RIFLE, 5).unwrap(), 5);
    step(&mut w, 160);
    assert_eq!(
        w.item_rounds(A, RIFLE).unwrap(),
        Rounds {
            clip: 5,
            reserve: 0
        }
    );
    // Items set afresh (a respawn) start full again.
    w.set_inventory(A, &[Some(RIFLE.into()), None, None, None, None])
        .unwrap();
    assert_eq!(
        w.item_rounds(A, RIFLE).unwrap(),
        Rounds {
            clip: 8,
            reserve: 24
        }
    );
    // Rounds survive a save.
    w.equip(A, Some(0)).unwrap();
    step(&mut w, 40);
    assert_eq!(shoot(&mut w), 1);
    let bytes = serde_json::to_vec(&w.save()).unwrap();
    let restored = WeaponsWorld::restore(rifle_pack(), &bytes).unwrap();
    assert_eq!(restored.item_rounds(A, RIFLE).unwrap().clip, 7);
}

#[test]
fn sounds_merge_with_their_package_and_bad_ammo_is_refused() {
    let base = Pack {
        schema_version: SCHEMA,
        id: "base".into(),
        items: Default::default(),
        images: Default::default(),
        projectiles: Default::default(),
        damage_types: Default::default(),
        explosions: Default::default(),
        sounds: Default::default(),
        definitions: vec![],
        resources: vec![],
        diagnostics: vec![],
    };
    let (merged, notes) = base.merge(vec![(
        "sample-commando-rifle/assets".into(),
        rifle_pack(),
    )]);
    assert!(notes.is_empty(), "{notes:?}");
    merged.validate().unwrap();
    let shot = merged.sound("sample-commando-rifle:shot").unwrap();
    assert_eq!(
        sound_root(Path::new("/content/weapons-pack-009"), shot),
        Path::new("/content/sample-commando-rifle/assets")
    );
    let mut bad = rifle_pack();
    bad.images.get_mut(RIFLE_IMAGE).unwrap().ammo = Some(Ammo {
        magazine: 0,
        reserve: 0,
    });
    assert!(bad.validate().is_err());
    let mut bad = rifle_pack();
    bad.images.get_mut(RIFLE_IMAGE).unwrap().states[1].reload = Some(99);
    assert!(bad.validate().is_err());
    let mut bad = rifle_pack();
    bad.sounds.get_mut("sample-commando-rifle:shot").unwrap().file = "../../evil.wav".into();
    assert!(bad.validate().is_err());
}
