//! Where a shot is drawn from. The host flies a fired projectile from the
//! shooter's eye along their aim, so it lands where the crosshair is. v20
//! started it at the held image's `muzzlePoint` and, with
//! `correctMuzzleVector`, aimed it from there at the point the eye looked
//! at. Drawn from the host's path alone, a tracer, a trail and a bullet
//! start in the shooter's face instead of at the gun.
//!
//! Each shot is drawn from the muzzle of the image its shooter holds, as
//! this client draws that image, and closes on the host's path along a
//! straight line to where the aim meets the world: the line v20's
//! projectile flew. Where it lands, and everything it does, is the host's.
//! Nothing is sent for it.
use bri_sim::session::WeaponView;
use bri_weapons::{ActorId, Projectile};
use glam::Vec3;
use std::borrow::Cow;
use std::collections::BTreeMap;

/// A shot first seen older than this (a player who joined while it flew)
/// is drawn where the host has it.
const FRESH_TICKS: u32 = 30;
/// A muzzle further than this from the eye (scaled with the shooter) is not
/// the shooter's gun: the shot is drawn where the host has it.
const MAX_MUZZLE_OFFSET: f32 = 4.0;
/// The shortest distance a shot closes on its path over.
const MIN_REACH: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Shot {
    /// From the host's start (the eye) to the drawn muzzle.
    offset: Vec3,
    /// The host's start.
    origin: Vec3,
    /// How far along its path the shot meets the drawn line: where the aim
    /// meets the world, or as far as it can fly.
    reach: f32,
}

#[derive(Default)]
pub struct ShotOrigins {
    shots: BTreeMap<u64, Option<Shot>>,
}

impl ShotOrigins {
    /// `view` with each fired projectile drawn from its shooter's muzzle,
    /// closing on the host's path. `muzzle` is where the shooter's held
    /// image fires from as drawn now; `reach` how far a shot from a point
    /// along a direction flies before it meets the world, up to a distance.
    pub fn shown<'a>(
        &mut self,
        view: &'a WeaponView,
        muzzle: impl Fn(ActorId) -> Option<Vec3>,
        reach: impl Fn(Vec3, Vec3, f32) -> f32,
        range: impl Fn(&Projectile) -> f32,
    ) -> Cow<'a, WeaponView> {
        let fired: Vec<&Projectile> = view.fired().collect();
        self.shots.retain(|id, _| fired.iter().any(|p| p.id == *id));
        for p in &fired {
            self.shots.entry(p.id).or_insert_with(|| {
                if p.age > FRESH_TICKS || p.bounced || p.stuck {
                    return None;
                }
                let speed = p.velocity.length();
                if speed < 1e-3 {
                    return None;
                }
                let offset = muzzle(p.source)? - p.origin;
                let most = MAX_MUZZLE_OFFSET * p.scale.max(0.2);
                if !offset.is_finite() || offset.length() > most {
                    return None;
                }
                let direction = p.velocity / speed;
                let range = range(p).max(MIN_REACH);
                let reach = reach(p.origin, direction, range).clamp(MIN_REACH, range);
                Some(Shot {
                    offset,
                    origin: p.origin,
                    reach,
                })
            });
        }
        let mut shown = Cow::Borrowed(view);
        for (index, p) in view.projectiles.iter().enumerate() {
            let Some(Some(shot)) = self.shots.get(&p.id) else {
                continue;
            };
            let share = drawn_share(shot, p);
            if share <= 0.0 {
                continue;
            }
            shown.to_mut().projectiles[index].position += shot.offset * share;
        }
        shown
    }
}

/// How much of the muzzle's offset is still drawn: all of it at the start,
/// none once the shot has flown as far as the drawn line meets its path,
/// or bounced, stuck or passed through an opening.
fn drawn_share(shot: &Shot, p: &Projectile) -> f32 {
    if p.bounced || p.stuck {
        return 0.0;
    }
    let travelled = (p.position - shot.origin).length();
    (1.0 - travelled / shot.reach).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shot(id: u64, position: Vec3, age: u32) -> Projectile {
        Projectile {
            id,
            definition: "rifle.round".into(),
            source: ActorId(7),
            position,
            velocity: Vec3::NEG_Z * 200.0,
            scale: 1.0,
            age,
            bounced: false,
            stuck: false,
            origin: Vec3::new(0.0, 2.0, 0.0),
            was_thrown: false,
            paint: None,
            heading: None,
        }
    }
    fn view(projectiles: Vec<Projectile>) -> WeaponView {
        WeaponView {
            projectiles,
            ..Default::default()
        }
    }
    /// The gun is held down and to the right of the eye.
    const MUZZLE: Vec3 = Vec3::new(0.4, 1.6, -0.8);
    fn muzzle(actor: ActorId) -> Option<Vec3> {
        (actor == ActorId(7)).then_some(MUZZLE)
    }
    /// A wall 40 units in front of the eye.
    fn wall(_: Vec3, _: Vec3, most: f32) -> f32 {
        most.min(40.0)
    }

    /// A shot is drawn leaving the muzzle, then flies a straight line to
    /// where the aim meets the wall, as v20's corrected muzzle vector flew
    /// it; from there on it is where the host has it.
    #[test]
    fn a_shot_leaves_the_muzzle_and_meets_the_aim() {
        let mut origins = ShotOrigins::default();
        let range = |_: &Projectile| 800.0;
        let eye = Vec3::new(0.0, 2.0, 0.0);
        let first = view(vec![shot(1, eye, 0)]);
        let drawn = origins.shown(&first, muzzle, wall, range);
        assert!(
            drawn.projectiles[0].position.distance(MUZZLE) < 1e-4,
            "{:?}",
            drawn.projectiles[0].position
        );
        // Half way to the wall it is half way across.
        let half = view(vec![shot(1, eye + Vec3::NEG_Z * 20.0, 12)]);
        let drawn = origins.shown(&half, muzzle, wall, range);
        let target = eye + Vec3::NEG_Z * 40.0;
        let on_line = MUZZLE.lerp(target, 0.5);
        assert!(drawn.projectiles[0].position.distance(on_line) < 1e-3);
        // At the wall, and after it, the host's path.
        for along in [40.0, 60.0] {
            let at = view(vec![shot(1, eye + Vec3::NEG_Z * along, 24)]);
            let drawn = origins.shown(&at, muzzle, wall, range);
            assert_eq!(drawn.projectiles[0].position, eye + Vec3::NEG_Z * along);
        }
    }

    /// Late joiners, bounced shots, other shooters' far-off muzzles and the
    /// host's own spawn and death effects are drawn where the host has them.
    #[test]
    fn only_fresh_shots_from_a_held_gun_move() {
        let mut origins = ShotOrigins::default();
        let range = |_: &Projectile| 800.0;
        let eye = Vec3::new(0.0, 2.0, 0.0);
        let old = shot(1, eye, FRESH_TICKS + 1);
        let mut bounced = shot(2, eye, 0);
        bounced.bounced = true;
        let mut stranger = shot(3, eye, 0);
        stranger.source = ActorId(8);
        let far = |_: ActorId| Some(eye + Vec3::X * 10.0);
        let mut spawn = shot(4, eye, 0);
        spawn.definition = bri_sim::session::SPAWN_PROJECTILE.into();
        let all = view(vec![old, bounced, stranger, spawn]);
        let drawn = origins.shown(&all, muzzle, wall, range);
        assert!(matches!(drawn, Cow::Borrowed(_)), "nothing moved");
        let mut fresh = ShotOrigins::default();
        let one = view(vec![shot(5, eye, 0)]);
        let drawn = fresh.shown(&one, far, wall, range);
        assert_eq!(drawn.projectiles[0].position, eye, "not the shooter's gun");
        // A shot that bounces after leaving is drawn on the host's path.
        let mut origins = ShotOrigins::default();
        origins.shown(&view(vec![shot(6, eye, 0)]), muzzle, wall, range);
        let mut later = shot(6, eye + Vec3::NEG_Z * 5.0, 6);
        later.bounced = true;
        let later = view(vec![later]);
        let drawn = origins.shown(&later, muzzle, wall, range);
        assert_eq!(drawn.projectiles[0].position, eye + Vec3::NEG_Z * 5.0);
        // Gone shots are forgotten.
        origins.shown(&view(Vec::new()), muzzle, wall, range);
        assert!(origins.shots.is_empty());
    }
}
