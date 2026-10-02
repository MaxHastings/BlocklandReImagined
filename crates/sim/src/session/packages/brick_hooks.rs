//! `on_brick`: Add-On rules hear when bricks of the kinds they name are
//! planted, loaded, painted, renamed or removed (Slayer's
//! `slayerPrepareBrick`, `onColorChange`, `setNTObjectName` and
//! `onRemove`: its "set for <teams>" print, Region Boundary bricks, Team
//! Bot Holes, `Team_<n>` names). The engine sees every brick change through
//! the changed-brick log, so plants by hand, loads, the paint can and
//! wrench outputs are all heard the same way; the rules decide what a brick
//! means.
use super::game_hooks::declaring;
use super::*;
use bri_package_runtime::rhai::Map;
use bri_package_runtime::script::{BrickView, brick_map};

/// Most brick events delivered in one tick; a load's remainder waits for
/// the next.
const MAX_EVENTS_PER_TICK: usize = 2048;

/// What a watched brick was when last seen.
#[derive(Debug, Clone)]
struct Seen {
    view: BrickView,
}

/// Watched bricks and who changed them.
#[derive(Debug, Default)]
pub(in crate::session) struct BrickWatch {
    seen: BTreeMap<BrickId, Seen>,
    /// The player behind a brick's latest change (a plant by hand, the
    /// paint can, the wrench), until it is heard.
    actors: BTreeMap<BrickId, OwnerId>,
    /// Changes not yet delivered: brick, event, player, the brick as it was.
    pending: VecDeque<(BrickId, &'static str, Option<OwnerId>, BrickView)>,
    started: bool,
}

impl Session {
    /// A player planted or edited `brick` by hand.
    pub(in crate::session) fn note_brick_actor(&mut self, owner: OwnerId, brick: BrickId) {
        if let Some(host) = self.packages.as_mut()
            && host
                .catalog
                .behaviours()
                .any(|(_, b)| !b.on_brick.is_empty())
        {
            host.brick_watch.actors.insert(brick, owner);
        }
    }

    fn watched(&self, kind: &str) -> bool {
        self.packages.as_ref().is_some_and(|h| {
            h.catalog
                .behaviours()
                .any(|(_, b)| b.on_brick.iter().any(|k| k == "*" || k == kind))
        })
    }

    /// Turn this tick's brick changes into `on_brick` events and deliver
    /// them.
    pub(in crate::session) fn deliver_brick_changes(&mut self, changed: &BTreeSet<BrickId>) {
        let Some(host) = self.packages.as_ref() else {
            return;
        };
        if !host
            .catalog
            .behaviours()
            .any(|(_, b)| !b.on_brick.is_empty())
        {
            return;
        }
        let ids: Vec<BrickId> = if host.brick_watch.started {
            changed.iter().copied().collect()
        } else {
            // The bricks already standing when the rules start count as
            // loaded.
            self.simulation.state().bricks.keys().copied().collect()
        };
        let mut events = Vec::new();
        for id in ids {
            let now = self.brick_view(id).filter(|v| self.watched(&v.kind));
            let host = self.packages.as_mut().expect("checked");
            let actor = host.brick_watch.actors.remove(&id);
            match (host.brick_watch.seen.get(&id), now) {
                (None, Some(view)) => {
                    let event = if actor.is_some() { "planted" } else { "loaded" };
                    events.push((id, event, actor, view.clone()));
                    host.brick_watch.seen.insert(id, Seen { view });
                }
                (Some(old), Some(view)) => {
                    if old.view.kind != view.kind {
                        events.push((id, "removed", actor, old.view.clone()));
                        events.push((id, "planted", actor, view.clone()));
                    } else {
                        if old.view.color != view.color {
                            events.push((id, "painted", actor, view.clone()));
                        }
                        if old.view.name != view.name {
                            events.push((id, "named", actor, view.clone()));
                        }
                    }
                    host.brick_watch.seen.insert(id, Seen { view });
                }
                (Some(old), None) => {
                    events.push((id, "removed", actor, old.view.clone()));
                    host.brick_watch.seen.remove(&id);
                }
                (None, None) => {}
            }
        }
        let host = self.packages.as_mut().expect("checked");
        host.brick_watch.started = true;
        host.brick_watch.pending.extend(events);
        let take = host.brick_watch.pending.len().min(MAX_EVENTS_PER_TICK);
        let due: Vec<_> = host.brick_watch.pending.drain(..take).collect();
        for (id, event, actor, view) in due {
            let hooks: Vec<String> = {
                let host = self.packages.as_ref().expect("checked");
                declaring(host, |_| true)
                    .into_iter()
                    .filter(|p| {
                        host.catalog.behaviours().any(|(id, b)| {
                            id == p && b.on_brick.iter().any(|k| k == "*" || *k == view.kind)
                        })
                    })
                    .collect()
            };
            let mut info = Map::new();
            info.insert("brick".into(), brick_map(&view));
            let player = actor.map_or(Dynamic::UNIT, |o| Dynamic::from_int(o as i64));
            for package in hooks {
                let _ = self.run_package(
                    &package,
                    "on_brick",
                    vec![
                        event.into(),
                        Dynamic::from_int(id as i64),
                        player.clone(),
                        Dynamic::from_map(info.clone()),
                    ],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(&package);
            }
        }
    }
}
