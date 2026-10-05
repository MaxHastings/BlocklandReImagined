//! An empty-handed bot that has no attack arms itself from an item it can
//! see lying in reach: one on an item brick or one dropped in the world,
//! whose image has a native attack (`hand_combat::item_attacks`). Picking it
//! up is the ordinary contact pickup; the bot only walks to it. An item whose
//! pickup a package script decides is the package's business and is left
//! alone. An item it reached without getting, or could not reach in time, is
//! passed over for a while.
use super::*;

/// How far off it looks for an item to arm itself with.
const REACH: f32 = 24.0;
/// How long it goes after one item before passing it over.
const TIMEOUT: u64 = 10 * 120;
/// How long it stands at an item it does not get before passing it over.
const AT_ITEM: u64 = 120;
/// How long an item it passed over stays passed over.
const SKIP: u64 = 30 * 120;

/// Where an item to arm with lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// On an item brick.
    Brick(bri_world::BrickId),
    /// Dropped in the world.
    Drop(u64),
}

/// The item a bot is going for, and the items it passed over.
#[derive(Clone, Debug, Default)]
pub(super) struct Arming {
    /// The item, since when it went for it, and since when it stood at it.
    going: Option<(Source, u64, Option<u64>)>,
    /// Items passed over, until when.
    skipped: Vec<(Source, u64)>,
}

impl Arming {
    /// Stop going for an item (it armed, or it no longer wants to).
    pub(super) fn clear(&mut self) {
        self.going = None;
    }
}

/// The nearest item in sight that would give `bot` an attack, and where it
/// lies.
fn nearest(session: &Session, bot: OwnerId, feet: Vec3, tick: u64) -> Option<(Source, Vec3)> {
    let peer = session.peers.get(&bot)?;
    let eye = peer.player.eye();
    let scale = peer.player.state().scale;
    let skipped = &session.bots.brains.get(&bot)?.arming.skipped;
    let passed = |source: Source| {
        skipped
            .iter()
            .any(|(s, until)| *s == source && tick < *until)
    };
    let statics = session
        .item_spawners
        .items
        .iter()
        .filter(|(_, item)| tick >= item.available_at)
        .map(|(id, item)| {
            (
                Source::Brick(*id),
                item.item.as_str(),
                Vec3::from(item.position),
            )
        });
    let drops = session
        .weapons
        .drops()
        .filter(|d| session.weapons.pickup_ready(ActorId(bot), d.id))
        .map(|d| (Source::Drop(d.id), d.item.as_str(), d.position));
    statics
        .chain(drops)
        .filter(|(source, _, at)| feet.distance(*at) <= REACH && !passed(*source))
        .filter(|(_, item, _)| {
            hand_combat::item_attacks(session, item, scale) && !session.pickup_scripted(item)
        })
        .filter(|(_, _, at)| session.simulation.sight(eye, *at, REACH).is_some())
        .map(|(source, _, at)| (source, at))
        .min_by(|a, b| feet.distance(a.1).total_cmp(&feet.distance(b.1)))
}

/// Where `bot`, which has no attack, goes to arm itself, if anywhere. It
/// keeps going for the item it chose while that is still there, and passes
/// it over when it cannot get it.
pub(super) fn arm_point(
    session: &mut Session,
    bot: OwnerId,
    feet: Vec3,
    tick: u64,
) -> Option<Vec3> {
    let going = session.bots.brains.get(&bot)?.arming.going;
    let found = nearest(session, bot, feet, tick);
    let brain = session.bots.brains.get_mut(&bot)?;
    let arming = &mut brain.arming;
    arming.skipped.retain(|(_, until)| tick < *until);
    let Some((source, at)) = found else {
        arming.going = None;
        return None;
    };
    let (since, standing) = match going {
        Some((s, since, standing)) if s == source => (since, standing),
        _ => (tick, None),
    };
    let standing = if flat(at - feet).length() < 1.0 {
        Some(standing.unwrap_or(tick))
    } else {
        None
    };
    if tick >= since + TIMEOUT || standing.is_some_and(|at| tick >= at + AT_ITEM) {
        arming.going = None;
        arming.skipped.push((source, tick + SKIP));
        return None;
    }
    arming.going = Some((source, since, standing));
    Some(at)
}
