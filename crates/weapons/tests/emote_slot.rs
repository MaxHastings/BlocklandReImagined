//! The emote slot (v20 image slot 3, `Player::emote`): an image worn
//! there follows its own states and runs their commands for the wearer
//! until another replaces it or the slot is emptied. The pack is our own.
use bri_weapons::*;
use glam::Vec3;

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

const A: ActorId = ActorId(1);

/// Commands the emote slot ran over `ticks` ticks.
fn commands(w: &mut WeaponsWorld, ticks: usize) -> Vec<String> {
    (0..ticks)
        .flat_map(|_| w.step(&mut Open))
        .filter_map(|e| match e {
            Event::ToolFire {
                hand: EMOTE_SLOT,
                command: Some(c),
                ..
            } => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn an_emote_image_runs_its_state_commands_until_replaced() {
    let json = format!(
        r#"{{
            "schema_version": {SCHEMA},
            "id": "kit",
            "images": {{
                "kit:image/mend": {{
                    "commands": {{
                        "states": {{ "onheal": "kit:heal" }},
                        "mount": "kit:worn"
                    }},
                    "states": [
                        {{ "name": "Activate", "ticks": 2, "timeout": 1 }},
                        {{ "name": "Heal", "ticks": 5, "script": "onHeal", "timeout": 1 }}
                    ]
                }},
                "kit:image/love": {{
                    "states": [{{ "name": "Glow", "ticks": 4, "timeout": 0 }}]
                }}
            }}
        }}"#
    );
    let mut w = WeaponsWorld::new(Pack::from_json(json.as_bytes()).unwrap()).unwrap();
    w.add_actor(A, 5).unwrap();
    assert_eq!(w.emote_state(A), None);
    w.emote(A, Some("kit:image/mend")).unwrap();
    // Going on runs `mount`; then each pass through `Heal` runs `heal`,
    // with no trigger to press: the slot follows timeouts alone.
    let ran = commands(&mut w, 26);
    assert_eq!(ran[0], "kit:worn");
    assert_eq!(&ran[1..], ["kit:heal"; 5], "{ran:?}");
    assert_eq!(w.emote_state(A), Some(("kit:image/mend", "Heal")));
    // Another image replaces it: its commands stop.
    w.emote(A, Some("kit:image/love")).unwrap();
    assert!(commands(&mut w, 20).is_empty());
    assert_eq!(w.emote_state(A), Some(("kit:image/love", "Glow")));
    // Wearing it again restarts it; emptying the slot stops it.
    w.emote(A, Some("kit:image/mend")).unwrap();
    assert!(commands(&mut w, 8).contains(&"kit:heal".to_string()));
    w.emote(A, None).unwrap();
    assert!(commands(&mut w, 20).is_empty());
    assert_eq!(w.emote_state(A), None);
    // An image the pack lacks empties the slot too.
    w.emote(A, Some("kit:image/mend")).unwrap();
    w.emote(A, Some("kit:image/missing")).unwrap();
    assert!(commands(&mut w, 20).iter().all(|c| c == "kit:worn"));
    assert_eq!(w.emote_state(A), None);
}
