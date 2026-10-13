//! Events Add-Ons send each other (`emit`, `listens`): an economy's
//! `paid`, an inventory's `moved`. The engine only carries them; what one
//! means is the sender's and the listener's.
use super::*;

/// Most events waiting for the next tick; more are dropped and the sender
/// is told, so a runaway sender cannot grow the queue.
const MAX_PENDING: usize = 1024;
/// Most events delivered in one tick. An event a listener sends while
/// hearing one waits for the next tick, so a chain of events moves one hop
/// a tick and never loops within one.
const MAX_A_TICK: usize = 256;

#[derive(Default)]
pub(super) struct AddOnEvents {
    pending: VecDeque<(String, script::Emitted)>,
}
impl AddOnEvents {
    /// Queue `event` from `package` for the next tick. False when the queue
    /// is full and it was dropped.
    pub(super) fn send(&mut self, package: &str, event: script::Emitted) -> bool {
        if self.pending.len() >= MAX_PENDING {
            return false;
        }
        self.pending.push_back((package.to_owned(), event));
        true
    }
}

/// Whether a `listens` entry hears `name` from `from`.
fn hears(listen: &str, from: &str, name: &str) -> bool {
    listen
        .split_once(':')
        .is_some_and(|(ns, event)| ns == from && (event == "*" || event == name))
}

impl Session {
    /// Deliver the events sent since the last tick to every Add-On that
    /// listens for them, in the order they were sent.
    pub(in crate::session) fn deliver_addon_events(&mut self) {
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        if host.addon_events.pending.is_empty() {
            return;
        }
        let take = host.addon_events.pending.len().min(MAX_A_TICK);
        let due: Vec<(String, script::Emitted)> =
            host.addon_events.pending.drain(..take).collect();
        let listeners: Vec<(String, Vec<String>)> = host
            .catalog
            .behaviours()
            .filter(|(_, b)| !b.listens.is_empty())
            .map(|(id, b)| (id.clone(), b.listens.clone()))
            .collect();
        for (from, event) in due {
            let data = rhai_value(&event.data);
            for (package, listens) in &listeners {
                if !listens.iter().any(|l| hears(l, &from, &event.name)) {
                    continue;
                }
                let _ = self.run_package(
                    package,
                    "on_addon_event",
                    vec![
                        Dynamic::from(from.clone()),
                        Dynamic::from(event.name.clone()),
                        data.clone(),
                    ],
                    Budget::Command,
                    None,
                    None,
                    None,
                );
                self.charge_work(package);
            }
        }
    }
}

fn rhai_value(value: &serde_json::Value) -> Dynamic {
    bri_package_runtime::rhai::serde::to_dynamic(value).unwrap_or(Dynamic::UNIT)
}

#[cfg(test)]
mod tests {
    use super::hears;

    #[test]
    fn a_listen_hears_its_event_or_every_event_of_its_add_on() {
        assert!(hears("economy:paid", "economy", "paid"));
        assert!(hears("economy:*", "economy", "paid"));
        assert!(!hears("economy:paid", "economy", "charged"));
        assert!(!hears("economy:*", "bank", "paid"));
    }
}
