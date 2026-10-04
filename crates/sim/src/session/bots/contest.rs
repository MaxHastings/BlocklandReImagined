//! Contest: opponents moving one body toward their own goals.
//!
//! Each side's grounded objective keeps its own intention for the body
//! (claims coordinate allies only, `claims::Resource::contends`), and the
//! ordinary push/hammer executor still decides how to move it. This piece
//! only answers whether an opponent is contesting the body and, when one
//! is, where to meet it: ahead along its actual velocity, by the kind's
//! `contest` tuning. Physics, credit and the authored rules decide who
//! scores; nothing here names a game or its content.
use super::claims::Resource;
use super::*;

/// Walking speed used to estimate how soon the bot reaches the body.
const WALK: f32 = 4.0;
/// Below this horizontal speed the body is treated as at rest.
const MOVING: f32 = 0.25;

/// An opponent (bot or player) intends to move, or last moved, this body.
pub(super) fn contested(session: &Session, bot: OwnerId, object: ObjectRef) -> bool {
    let ObjectRef::Vehicle(vehicle) = object else {
        return false;
    };
    let tick = session.simulation.state().tick;
    let opponent = |o: OwnerId| o != bot && !session.bot_allies(bot, o);
    session.mover_credit(object).is_some_and(opponent)
        || session
            .bots
            .claims
            .claimants_on(vehicle, tick)
            .any(opponent)
}

/// Physically at a body an opponent contests: within the kind's
/// `contest.engage` of its centre. Engagement renews this side's claim the
/// way its own credited progress would; the objective's approach deadline
/// still needs real delivery progress, so a lost contest stays bounded.
pub(super) fn engaged(session: &Session, bot: OwnerId, resource: Resource, feet: Vec3) -> bool {
    let Resource::Body { vehicle } = resource else {
        return false;
    };
    let object = ObjectRef::Vehicle(vehicle);
    let Some(reach) = session.bots.brains.get(&bot).map(|b| b.kind.contest.engage) else {
        return false;
    };
    session
        .object_centre(object)
        .is_some_and(|centre| flat(centre - feet).length() <= reach)
        && contested(session, bot, object)
}

/// How far ahead of the body's current centre a contesting bot approaches:
/// its actual horizontal velocity over the time to reach it, bounded by the
/// kind's `contest.lead_seconds` and `contest.max_lead`. Zero when the body
/// is not contested or at rest.
pub(super) fn lead(
    session: &Session,
    bot: OwnerId,
    object: ObjectRef,
    feet: Vec3,
    centre: Vec3,
    velocity: Vec3,
) -> Vec3 {
    let Some(brain) = session.bots.brains.get(&bot) else {
        return Vec3::ZERO;
    };
    let tuning = &brain.kind.contest;
    let moving = flat(velocity);
    if !moving.is_finite() || moving.length() < MOVING || !contested(session, bot, object) {
        return Vec3::ZERO;
    }
    let seconds = (flat(centre - feet).length() / WALK).min(tuning.lead_seconds);
    lead_offset(moving, seconds, tuning.max_lead)
}

fn lead_offset(moving: Vec3, seconds: f32, max_lead: f32) -> Vec3 {
    let offset = (moving * seconds.max(0.0)).clamp_length_max(max_lead.max(0.0));
    if offset.is_finite() {
        offset
    } else {
        Vec3::ZERO
    }
}

/// The way to push a contested body an opponent is driving back against
/// `toward` (its horizontal velocity within 60 degrees of straight back,
/// faster than walking pace's half): turned by the kind's
/// `contest.clear_degrees` to the side whose rear is nearer the bot, so
/// the body is knocked out of its line instead of being met head on (which
/// leaves the defender carried back with it). Otherwise `toward`.
pub(super) fn clearing(
    session: &Session,
    bot: OwnerId,
    object: ObjectRef,
    feet: Vec3,
    centre: Vec3,
    velocity: Vec3,
    toward: Vec3,
) -> Vec3 {
    let Some(brain) = session.bots.brains.get(&bot) else {
        return toward;
    };
    let degrees = brain.kind.contest.clear_degrees;
    if degrees <= 0.0 || !contested(session, bot, object) {
        return toward;
    }
    clear_heading(feet, centre, velocity, toward, degrees)
}

fn clear_heading(feet: Vec3, centre: Vec3, velocity: Vec3, toward: Vec3, degrees: f32) -> Vec3 {
    let moving = flat(velocity);
    let Some(heading) = flat(toward).try_normalize() else {
        return toward;
    };
    if moving.length() < WALK * 0.5 || moving.normalize().dot(heading) > -0.5 {
        return toward;
    }
    let turns = [degrees, -degrees].map(|d| glam::Quat::from_rotation_y(d.to_radians()) * heading);
    let rear = |t: Vec3| flat(centre - t - feet).length();
    if rear(turns[0]) <= rear(turns[1]) {
        turns[0]
    } else {
        turns[1]
    }
}

/// Where a bot covers a loose body a teammate is delivering: the kind's
/// `contest.cover_distance` behind the body against `heading` (so between
/// the body and where an opponent would send it), and `contest.cover_side`
/// to the side of that line it is already on. Several teammates covering
/// one body take slots in id order, alternating sides and each pair a
/// `cover_distance` further back, so they do not stand on each other.
/// None when the kind does not cover, the resource is not a loose body, or
/// there is no heading.
pub(super) fn cover(
    session: &Session,
    bot: OwnerId,
    resource: Resource,
    feet: Vec3,
    heading: Vec3,
) -> Option<Vec3> {
    let Resource::Body { vehicle } = resource else {
        return None;
    };
    let brain = session.bots.brains.get(&bot)?;
    let tuning = &brain.kind.contest;
    let centre = session.object_centre(ObjectRef::Vehicle(vehicle))?;
    let tick = session.simulation.state().tick;
    let claimants: Vec<OwnerId> = session.bots.claims.claimants_on(vehicle, tick).collect();
    // Teammate bots near the body that are not working it: the covers.
    let covers: Vec<OwnerId> = session
        .bots
        .brains
        .keys()
        .copied()
        .filter(|o| (*o == bot || session.bot_allies(bot, *o)) && !claimants.contains(o))
        .filter(|o| {
            session.peers.get(o).is_some_and(|p| {
                p.combat.alive
                    && flat(Vec3::from(p.player.state().feet) - centre).length()
                        <= brain.kind.objective_radius
            })
        })
        .collect();
    let slot = covers.iter().position(|o| *o == bot).unwrap_or(0);
    // The teammate working the body, whose side a lone cover leaves free.
    let worker = claimants
        .iter()
        .find(|o| **o != bot && session.bot_allies(bot, **o))
        .and_then(|o| session.peers.get(o))
        .map(|p| Vec3::from(p.player.state().feet));
    let velocity = session
        .vehicles
        .world
        .as_ref()
        .and_then(|w| {
            w.vehicle_snapshot(
                &session.simulation.physics,
                bri_vehicles::VehicleId(vehicle),
            )
        })
        .map_or(Vec3::ZERO, |v| Vec3::from(v.velocity));
    let point = cover_point(
        feet,
        centre,
        heading,
        tuning.cover_distance,
        tuning.cover_side,
        (slot, covers.len().max(1)),
        worker,
    )?;
    let point = out_of_the_way(point, centre, velocity, heading, tuning.cover_side);
    // Short of any wall between the body and that point (behind a goal
    // line, a pitch wall, a build), rather than pressed against it.
    let offset = flat(point - centre);
    let length = offset.length();
    let direction = offset.try_normalize()?;
    Some(match session.world_ray(centre, direction, length) {
        Some(hit) => {
            let mut short = centre + direction * (hit - WALL_CLEARANCE).max(0.0);
            short.y = point.y;
            short
        }
        None => point,
    })
}

/// A body driven back toward the cover runs down the line it stands on:
/// then it stands level with the body and twice `side` off its path
/// instead, out of the way, so the opponent's drive is not deflected off
/// it into its own goal.
fn out_of_the_way(point: Vec3, centre: Vec3, velocity: Vec3, heading: Vec3, side: f32) -> Vec3 {
    let moving = flat(velocity);
    let Some(heading) = flat(heading).try_normalize() else {
        return point;
    };
    if moving.length() < WALK * 0.5 || moving.normalize().dot(heading) > -0.5 {
        return point;
    }
    let path = moving.normalize();
    let across = Vec3::new(-path.z, 0.0, path.x);
    let sign = if flat(point - centre).dot(across) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut aside = centre + across * sign * side.max(1.0) * 2.0;
    aside.y = point.y;
    aside
}

/// How far short of a wall a cover point stands: a Blockhead's width and
/// a little.
const WALL_CLEARANCE: f32 = 1.5;

fn cover_point(
    feet: Vec3,
    centre: Vec3,
    heading: Vec3,
    behind: f32,
    side: f32,
    (slot, covers): (usize, usize),
    worker: Option<Vec3>,
) -> Option<Vec3> {
    let heading = flat(heading).try_normalize()?;
    if behind <= 0.0 || !behind.is_finite() || !centre.is_finite() || !feet.is_finite() {
        return None;
    }
    let across = Vec3::new(-heading.z, 0.0, heading.x);
    // Alone, the side the teammate working the body is not on, else the
    // side it is on itself; several, alternating by slot.
    let side_of = |at: Vec3| flat(at - centre).dot(across);
    let sign = if covers > 1 {
        if slot % 2 == 0 { 1.0 } else { -1.0 }
    } else if let Some(w) = worker.filter(|w| side_of(*w).abs() > 0.5) {
        -side_of(w).signum()
    } else if side_of(feet) < 0.0 {
        -1.0
    } else {
        1.0
    };
    // Each further pair stands further back and wider.
    let rank = (1 + slot / 2) as f32;
    let mut point = centre - heading * behind * rank + across * sign * side.max(0.0) * rank;
    point.y = feet.y;
    point.is_finite().then_some(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_follows_velocity_and_respects_its_bounds() {
        let v = Vec3::new(3.0, 0.0, -4.0);
        assert!(lead_offset(v, 0.5, 10.0).abs_diff_eq(v * 0.5, 1e-6));
        assert!((lead_offset(v, 5.0, 2.0).length() - 2.0).abs() < 1e-5);
        assert_eq!(lead_offset(v, -1.0, 2.0), Vec3::ZERO);
        assert_eq!(lead_offset(v, 1.0, 0.0), Vec3::ZERO);
        assert_eq!(lead_offset(Vec3::NAN, 1.0, 2.0), Vec3::ZERO);
    }

    #[test]
    fn a_cover_steps_off_the_path_of_a_body_driven_back_at_it() {
        let centre = Vec3::new(0.0, 1.0, 0.0);
        let north = Vec3::NEG_Z;
        let cover = Vec3::new(3.0, 0.0, 6.0);
        // Driven back south at it: level with the body, off its path.
        let aside = out_of_the_way(cover, centre, Vec3::new(0.0, 0.0, 4.0), north, 3.0);
        assert!(aside.abs_diff_eq(Vec3::new(6.0, 0.0, 0.0), 1e-5), "{aside}");
        // Going the team's way, or slowly: the cover point stands.
        assert_eq!(
            out_of_the_way(cover, centre, Vec3::new(0.0, 0.0, -4.0), north, 3.0),
            cover
        );
        assert_eq!(
            out_of_the_way(cover, centre, Vec3::new(0.0, 0.0, 1.0), north, 3.0),
            cover
        );
    }

    #[test]
    fn a_body_driven_straight_back_is_cleared_to_the_bots_side() {
        let centre = Vec3::new(0.0, 1.0, 0.0);
        let north = Vec3::NEG_Z;
        let back = Vec3::new(0.0, 0.0, 4.0);
        // The bot stands north-east of the body, the side it clears from.
        let east = clear_heading(Vec3::new(1.0, 0.0, -2.0), centre, back, north, 60.0);
        assert!(east.z < -0.4 && east.x < -0.8, "{east}");
        let west = clear_heading(Vec3::new(-1.0, 0.0, -2.0), centre, back, north, 60.0);
        assert!(west.z < -0.4 && west.x > 0.8, "{west}");
        // Slow, sideways or with it: unchanged.
        assert_eq!(
            clear_heading(Vec3::X, centre, back * 0.2, north, 60.0),
            north
        );
        assert_eq!(
            clear_heading(Vec3::X, centre, Vec3::X * 4.0, north, 60.0),
            north
        );
        assert_eq!(clear_heading(Vec3::X, centre, -back, north, 60.0), north);
    }

    #[test]
    fn cover_stands_behind_the_body_on_the_side_it_is_on() {
        let centre = Vec3::new(0.0, 1.0, 0.0);
        let north = Vec3::NEG_Z;
        let one = (0, 1);
        let left = cover_point(
            Vec3::new(-2.0, 0.0, 1.0),
            centre,
            north,
            6.0,
            3.0,
            one,
            None,
        )
        .unwrap();
        assert!(left.abs_diff_eq(Vec3::new(-3.0, 0.0, 6.0), 1e-5), "{left}");
        let right = cover_point(
            Vec3::new(2.0, 0.0, -1.0),
            centre,
            north,
            6.0,
            3.0,
            one,
            None,
        )
        .unwrap();
        assert!(right.abs_diff_eq(Vec3::new(3.0, 0.0, 6.0), 1e-5), "{right}");
        assert_eq!(
            cover_point(Vec3::ZERO, centre, north, 0.0, 3.0, one, None),
            None
        );
        assert_eq!(
            cover_point(Vec3::ZERO, centre, Vec3::Y, 6.0, 3.0, one, None),
            None
        );
        // Three covers: both sides, then one further back, wherever they stand.
        let slots: Vec<Vec3> = (0..3)
            .map(|k| {
                cover_point(
                    Vec3::new(2.0, 0.0, 0.0),
                    centre,
                    north,
                    6.0,
                    3.0,
                    (k, 3),
                    None,
                )
                .unwrap()
            })
            .collect();
        assert!(
            slots[0].abs_diff_eq(Vec3::new(3.0, 0.0, 6.0), 1e-5),
            "{slots:?}"
        );
        assert!(
            slots[1].abs_diff_eq(Vec3::new(-3.0, 0.0, 6.0), 1e-5),
            "{slots:?}"
        );
        assert!(
            slots[2].abs_diff_eq(Vec3::new(6.0, 0.0, 12.0), 1e-5),
            "{slots:?}"
        );
        // Alone beside a teammate working the body on its own side: the
        // other side.
        let worker = Some(Vec3::new(1.5, 0.0, 1.5));
        let free = cover_point(
            Vec3::new(2.0, 0.0, 3.0),
            centre,
            north,
            6.0,
            3.0,
            one,
            worker,
        );
        assert!(
            free.unwrap().abs_diff_eq(Vec3::new(-3.0, 0.0, 6.0), 1e-5),
            "{free:?}"
        );
    }
}
