//! What a match HUD reads besides plain values: the latest kills for kill
//! feeds and the other players as a motion tracker sees them, built each
//! frame from the replicated view (`bri_content::hud::Live`).
use bri_content::hud::{Contact, FeedKill, Relation};
use bri_sim::session::{KillLine, MiniGameView, Vitals};
use bri_world::OwnerId;
use glam::Vec3;
use std::collections::BTreeMap;

/// A team member's colour: their team's paint, RGBA 0 to 1.
fn team_color(
    owner: OwnerId,
    vitals: &BTreeMap<OwnerId, Vitals>,
    games: &[MiniGameView],
    palette: &[[f32; 4]],
) -> Option<[f32; 4]> {
    let team = vitals.get(&owner)?.team?;
    let game = games.iter().find(|g| g.members.contains(&owner))?;
    let paint = game.teams.iter().find(|t| t.id.0 == team)?.color;
    let c = palette.get(usize::from(paint))?;
    Some([c[0], c[1], c[2], 1.0])
}

/// The kept deaths as kill feeds show them, oldest first.
pub fn feed<'a>(
    kills: impl IntoIterator<Item = (f32, &'a KillLine)>,
    vitals: &BTreeMap<OwnerId, Vitals>,
    games: &[MiniGameView],
    palette: &[[f32; 4]],
) -> Vec<FeedKill> {
    kills
        .into_iter()
        .map(|(age, k)| FeedKill {
            age,
            killer: k.killer.zip(k.killer_name.clone()).map(|(owner, name)| {
                (
                    crate::app::plain_chat(&name),
                    team_color(owner, vitals, games, palette),
                )
            }),
            victim: (
                crate::app::plain_chat(&k.victim_name),
                team_color(k.victim, vitals, games, palette),
            ),
            weapon: k.weapon.clone(),
        })
        .collect()
}

/// Which side `other` is on from `viewer`'s: their team in a shared game
/// with teams, anyone in a shared game without, nobody otherwise.
pub fn relation(viewer: OwnerId, other: OwnerId, vitals: &BTreeMap<OwnerId, Vitals>) -> Relation {
    let (Some(a), Some(b)) = (vitals.get(&viewer), vitals.get(&other)) else {
        return Relation::Neutral;
    };
    match (a.minigame, b.minigame) {
        (Some(x), Some(y)) if x == y => match (a.team, b.team) {
            (Some(s), Some(t)) if s == t => Relation::Ally,
            _ => Relation::Enemy,
        },
        _ => Relation::Neutral,
    }
}

/// One other living player as a tracker sees them.
pub struct Seen {
    pub owner: OwnerId,
    pub feet: Vec3,
    pub velocity: Vec3,
    pub crouched: bool,
}

/// Every other living player relative to the viewer standing at `feet`
/// facing `forward` (its level part is ahead), at server tick `tick`.
pub fn contacts(
    viewer: OwnerId,
    feet: Vec3,
    forward: Vec3,
    seen: impl IntoIterator<Item = Seen>,
    vitals: &BTreeMap<OwnerId, Vitals>,
    tick: f64,
) -> Vec<Contact> {
    let ahead = Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::NEG_Z);
    let right = Vec3::new(-ahead.z, 0.0, ahead.x);
    seen.into_iter()
        .filter(|s| s.owner != viewer && vitals.get(&s.owner).is_some_and(|v| v.alive))
        .map(|s| {
            let d = s.feet - feet;
            let v = vitals.get(&s.owner);
            let fired = v.and_then(|v| {
                if v.firing {
                    Some(0.0)
                } else {
                    v.fired_tick
                        .map(|t| ((tick - t as f64) / 120.0).max(0.0) as f32)
                }
            });
            Contact {
                id: s.owner,
                right: d.dot(right),
                ahead: d.dot(ahead),
                above: d.y,
                speed: s.velocity.length(),
                crouched: s.crouched,
                fired,
                relation: relation(viewer, s.owner, vitals),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vitals(minigame: Option<u64>, team: Option<u32>) -> Vitals {
        Vitals {
            health: 100.0,
            alive: true,
            respawn_tick: 0,
            respawn_held: false,
            spawn_tick: 0,
            died_tick: None,
            death: None,
            firing: false,
            fired_tick: None,
            score: 0,
            minigame,
            team,
            invite: None,
            light: false,
            mounted: None,
            vehicle_health: None,
            ride: None,
            look_limits: None,
            control: Default::default(),
            camera_path: None,
            camera_point: None,
            talking: false,
            sitting: false,
            ghost: None,
            base_body: None,
            held: false,
        }
    }

    #[test]
    fn sides_come_from_the_shared_game_and_team() {
        let all = BTreeMap::from([
            (1, vitals(Some(1), Some(1))),
            (2, vitals(Some(1), Some(1))),
            (3, vitals(Some(1), Some(2))),
            (4, vitals(None, None)),
        ]);
        assert_eq!(relation(1, 2, &all), Relation::Ally);
        assert_eq!(relation(1, 3, &all), Relation::Enemy);
        assert_eq!(relation(1, 4, &all), Relation::Neutral);
    }

    #[test]
    fn a_contact_is_placed_by_the_viewers_facing() {
        let mut all = BTreeMap::from([(1, vitals(Some(1), None)), (2, vitals(Some(1), None))]);
        all.get_mut(&2).unwrap().fired_tick = Some(0);
        // Facing +X: a player 10 units along +X is straight ahead, one
        // along +Z is to the right.
        let seen = |feet: Vec3| Seen {
            owner: 2,
            feet,
            velocity: Vec3::new(3.0, 0.0, 4.0),
            crouched: true,
        };
        let c = contacts(
            1,
            Vec3::ZERO,
            Vec3::X,
            [seen(Vec3::new(10.0, 2.0, 0.0))],
            &all,
            240.0,
        );
        assert_eq!(c.len(), 1);
        assert!((c[0].ahead - 10.0).abs() < 1e-5 && c[0].right.abs() < 1e-5);
        assert_eq!((c[0].above, c[0].speed, c[0].fired), (2.0, 5.0, Some(2.0)));
        assert_eq!(c[0].relation, Relation::Enemy);
        let c = contacts(
            1,
            Vec3::ZERO,
            Vec3::X,
            [seen(Vec3::new(0.0, 0.0, 5.0))],
            &all,
            0.0,
        );
        assert!((c[0].right - 5.0).abs() < 1e-5, "{:?}", c[0]);
        // Crouched and slow enough to hide, but fired just now.
        assert!(c[0].noticed(1.0, false, 2.0));
        assert!(!c[0].noticed(1.0, false, 0.0));
    }
}
