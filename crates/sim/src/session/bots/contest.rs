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
}
