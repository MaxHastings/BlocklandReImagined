//! Limits and timing an Add-On's data meets only while the game runs, found
//! before it does.
//!
//! * [`weapon_timing`] reads each weapon image's state machine the way the
//!   weapons runtime steps it and reports the states that repeat with nothing
//!   to stop them (the dry-fire and reload loop of an empty gun) or pass with
//!   no time at all (which the runtime stops by unmounting the image), and a
//!   state whose callback changes ammunition in the same step that decides
//!   where the state goes next.
//! * [`state_budget`] sizes the state a behaviour declares against the
//!   per-value and per-player limits, and names the key that is closest to
//!   them.
//!
//! Nothing here runs a script. A finding is a warning: it says what the data
//! allows, and the Add-On may still never reach it.
use crate::{
    content::{Behaviour, StateKey},
    state::{MAX_GLOBAL_STATE_BYTES, MAX_PLAYER_STATE_BYTES},
};
use bri_package::diag::Diagnostic;
use bri_weapons::{Image, Pack, State};
use std::collections::{BTreeMap, BTreeSet};

/// What the weapons runtime decides from, per tick, besides time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Scenario {
    trigger: bool,
    ammo: bool,
    loaded: bool,
}

/// Ticks a second, for the seconds in a message.
const TICK_HZ: f32 = bri_weapons::TICK_HZ as f32;
/// The fraction of a limit at which a declared state is called large.
const LARGE: f64 = 0.5;
/// A value one script write can still grow, in bytes, below which a default
/// is never called large.
const SMALL_VALUE: usize = 256;

fn uses_edge(states: &[State], edge: fn(&State) -> bool) -> bool {
    states.iter().any(edge)
}

/// The state `state` goes to next in `s` and the ticks it takes to leave it,
/// in the order the runtime checks: loaded, ammo, trigger, then timeout once
/// the state's time is up. A state with a condition edge and without `wait`
/// takes it at once.
fn step(state: &State, index: usize, s: Scenario) -> Option<(usize, u32)> {
    let condition = if s.loaded {
        state.loaded
    } else {
        state.not_loaded
    }
    .or(if s.ammo { state.ammo } else { state.no_ammo })
    .or(if s.trigger { state.down } else { state.up });
    let (next, dwell) = match condition {
        Some(next) => (next, if state.wait { state.ticks } else { 0 }),
        None => (state.timeout?, state.ticks),
    };
    // A zero-time state that leads to itself waits a tick.
    Some((next, if next == index { dwell.max(1) } else { dwell }))
}

fn describe(s: Scenario, image: &Image) -> String {
    let states = &image.states;
    let mut parts = vec![if s.trigger {
        "the trigger held"
    } else {
        "the trigger up"
    }];
    if uses_edge(states, |x| x.ammo.is_some() || x.no_ammo.is_some()) {
        parts.push(if s.ammo {
            "ammunition"
        } else {
            "no ammunition"
        });
    }
    if uses_edge(states, |x| x.loaded.is_some() || x.not_loaded.is_some()) {
        parts.push(if s.loaded { "loaded" } else { "not loaded" });
    }
    parts.join(", ")
}

struct Finding {
    cycle: Vec<usize>,
    period: u32,
    scenarios: Vec<Scenario>,
}

/// Every cycle `image` can enter and be held in by a fixed condition.
fn cycles(image: &Image) -> Vec<Finding> {
    let states = &image.states;
    if states.is_empty() {
        return vec![];
    }
    let trigger = uses_edge(states, |x| x.down.is_some() || x.up.is_some());
    let ammo = uses_edge(states, |x| x.ammo.is_some() || x.no_ammo.is_some());
    let loaded = uses_edge(states, |x| x.loaded.is_some() || x.not_loaded.is_some());
    let mut scenarios = vec![];
    for t in [false, true].into_iter().take(if trigger { 2 } else { 1 }) {
        for a in [true, false].into_iter().take(if ammo { 2 } else { 1 }) {
            for l in [true, false].into_iter().take(if loaded { 2 } else { 1 }) {
                scenarios.push(Scenario {
                    trigger: t,
                    ammo: a,
                    loaded: l,
                });
            }
        }
    }
    // States any chain of edges reaches from the first.
    let mut reach = BTreeSet::from([0usize]);
    let mut queue = vec![0usize];
    while let Some(i) = queue.pop() {
        let s = &states[i];
        for next in [
            s.loaded,
            s.not_loaded,
            s.ammo,
            s.no_ammo,
            s.down,
            s.up,
            s.timeout,
        ]
        .into_iter()
        .flatten()
        {
            if next < states.len() && reach.insert(next) {
                queue.push(next);
            }
        }
    }
    let mut found: BTreeMap<Vec<usize>, Finding> = BTreeMap::new();
    for sc in scenarios {
        let next: Vec<Option<(usize, u32)>> = states
            .iter()
            .enumerate()
            .map(|(i, s)| step(s, i, sc).filter(|(n, _)| *n < states.len()))
            .collect();
        let mut done = BTreeSet::new();
        for &start in &reach {
            let mut path = vec![];
            let mut at = start;
            while !done.contains(&at) && !path.contains(&at) {
                path.push(at);
                match next[at] {
                    Some((n, _)) => at = n,
                    None => break,
                }
            }
            if let Some(&last) = path.last()
                && next[last].is_some()
                && let Some(from) = path.iter().position(|&p| p == at)
                && !done.contains(&at)
            {
                let mut cycle = path[from..].to_vec();
                let period = cycle
                    .iter()
                    .map(|&c| next[c].map_or(0, |(_, d)| d))
                    .sum::<u32>();
                let lowest = cycle.iter().enumerate().min_by_key(|(_, c)| **c).unwrap().0;
                cycle.rotate_left(lowest);
                found
                    .entry(cycle.clone())
                    .or_insert(Finding {
                        cycle,
                        period,
                        scenarios: vec![],
                    })
                    .scenarios
                    .push(sc);
            }
            done.extend(path);
        }
    }
    found.into_values().collect()
}

fn names(image: &Image, cycle: &[usize]) -> String {
    let mut out: Vec<&str> = cycle
        .iter()
        .map(|&i| image.states[i].name.as_str())
        .collect();
    if let Some(first) = out.first().copied() {
        out.push(first);
    }
    out.join(" → ")
}

fn when(image: &Image, scenarios: &[Scenario]) -> String {
    let mut text: Vec<String> = scenarios
        .iter()
        .take(2)
        .map(|s| describe(*s, image))
        .collect();
    if scenarios.len() > 2 {
        text.push("other conditions".into());
    }
    text.join(" or ")
}

/// Diagnostics for every image of `pack`, located at `at`.
pub fn weapon_timing(pack: &Pack, at: &str) -> Vec<Diagnostic> {
    let mut out = vec![];
    for (id, image) in &pack.images {
        let location = format!("{at}#{id}");
        for finding in cycles(image) {
            let route = names(image, &finding.cycle);
            if finding.period == 0 {
                out.push(
                    Diagnostic::warning(
                        "weapon.zero_time_cycle",
                        format!(
                            "{id}: with {}, the states {route} lead to one another in no time at all, \
                             so the game stops the image (\"instantaneous transition budget exceeded\") \
                             and unmounts it",
                            when(image, &finding.scenarios),
                        ),
                    )
                    .at(location.clone())
                    .hint("give one state in the loop a time (`ticks`), or take away the edge that closes it"),
                );
                continue;
            }
            // Only a loop through a rule the Add-On supplies is reported:
            // the engine's own callbacks (a pump shotgun's reload check
            // that sets the image loaded) are what end their loops, but
            // nothing guarantees a rule's operation changes the ammunition
            // or loaded flag the loop is waiting on.
            let idle = finding.scenarios.iter().all(|s| !s.trigger);
            let does: Vec<String> = finding
                .cycle
                .iter()
                .filter_map(|&c| {
                    let state = &image.states[c];
                    image
                        .commands
                        .for_script(&state.script)
                        .map(|command| format!("{} ({command})", state.script))
                })
                .collect();
            if idle && !does.is_empty() {
                out.push(
                    Diagnostic::warning(
                        "weapon.idle_cycle",
                        format!(
                            "{id}: with {}, the states {route} repeat every {} ticks ({:.2} s), \
                             running {} each time, for as long as that holds. Nothing the player does stops it",
                            when(image, &finding.scenarios),
                            finding.period,
                            finding.period as f32 / TICK_HZ,
                            does.join(", "),
                        ),
                    )
                    .at(location.clone())
                    .hint(
                        "end the loop in a state that waits for the trigger: for an empty gun, \
                         leave no `no_ammo` edge on the state it rests in, so dry fire and reload \
                         run only when the trigger or the reload key asks",
                    ),
                );
            }
        }
        for state in &image.states {
            let decided = state.ammo.is_some()
                || state.no_ammo.is_some()
                || state.loaded.is_some()
                || state.not_loaded.is_some();
            if let Some(command) = image.commands.for_script(&state.script)
                && decided
                && (state.ticks == 0 || !state.wait)
            {
                out.push(
                    Diagnostic::warning(
                        "weapon.callback_timing",
                        format!(
                            "{id}: state {} runs {command} for `{}`, but takes its ammunition or loaded edge in \
                             the same step. A rule's changes (ammunition, loaded) apply after the step, so the \
                             edge is decided from the values before the rule ran",
                            state.name, state.script
                        ),
                    )
                    .at(location.clone())
                    .hint("give the state at least 1 tick so the edge is decided in the step after the rule's changes apply"),
                );
            }
        }
    }
    out
}

fn bytes(key: &StateKey) -> usize {
    serde_json::to_vec(&key.default).map_or(0, |b| b.len())
}

fn budget(
    package: &str,
    scope: &str,
    keys: &BTreeMap<String, StateKey>,
    limit: usize,
    out: &mut Vec<Diagnostic>,
) {
    let at = format!("{package}/behaviour.json#state.{scope}");
    let mut total = 0usize;
    let mut largest: Option<(&String, usize)> = None;
    for (name, key) in keys {
        let size = bytes(key);
        total += name.len() + size + 8;
        if largest.is_none_or(|(_, big)| size > big) {
            largest = Some((name, size));
        }
        if size > crate::state::MAX_VALUE_BYTES {
            out.push(
                Diagnostic::error(
                    "state.value_too_big",
                    format!(
                        "state key `{name}` starts at {size} bytes; one value may hold at most {}",
                        crate::state::MAX_VALUE_BYTES
                    ),
                )
                .at(at.clone())
                .hint("split it into several keys, such as one per ammunition type, or keep fewer entries"),
            );
        } else if size >= SMALL_VALUE && size as f64 >= LARGE * crate::state::MAX_VALUE_BYTES as f64
        {
            out.push(
                Diagnostic::warning(
                    "state.value_large",
                    format!(
                        "state key `{name}` starts at {size} of the {} bytes one value may hold, \
                         so a script that adds to it will soon be refused",
                        crate::state::MAX_VALUE_BYTES
                    ),
                )
                .at(at.clone())
                .hint("split it into one key per ammunition type or per weapon"),
            );
        }
    }
    total += 96;
    if total > limit {
        out.push(
            Diagnostic::error(
                "state.scope_too_big",
                format!(
                    "the {scope} state starts at about {total} bytes, over the {limit} one {} may hold; \
                     `{}` is the largest key ({} bytes)",
                    if scope == "player" { "player" } else { "package" },
                    largest.map_or("", |l| l.0.as_str()),
                    largest.map_or(0, |l| l.1),
                ),
            )
            .at(at),
        );
    } else if total as f64 >= LARGE * limit as f64 {
        out.push(
            Diagnostic::warning(
                "state.scope_large",
                format!(
                    "the {scope} state starts at about {total} of {limit} bytes; `{}` is the largest key ({} bytes)",
                    largest.map_or("", |l| l.0.as_str()),
                    largest.map_or(0, |l| l.1),
                ),
            )
            .at(at),
        );
    }
}

/// How the state `behaviour` declares sits against the limits.
pub fn state_budget(package: &str, behaviour: &Behaviour) -> Vec<Diagnostic> {
    let mut out = vec![];
    budget(
        package,
        "player",
        &behaviour.state.player,
        MAX_PLAYER_STATE_BYTES,
        &mut out,
    );
    budget(
        package,
        "global",
        &behaviour.state.global,
        MAX_GLOBAL_STATE_BYTES,
        &mut out,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pack(states: serde_json::Value, commands: serde_json::Value) -> Pack {
        let image = json!({
            "id": "t:image/gun", "name": "gun", "model": "", "states": states, "commands": commands
        });
        Pack::from_json(
            json!({
                "schema_version": bri_weapons::SCHEMA, "id": "t",
                "items": {}, "images": { "t:image/gun": image }
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap()
    }

    fn codes(pack: &Pack) -> Vec<String> {
        weapon_timing(pack, "t/assets/weapons.json")
            .into_iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn a_resting_state_with_nothing_to_leave_by_is_quiet() {
        let p = pack(
            json!([
                {"name": "Ready", "ticks": 0, "allow_change": true, "down": 1},
                {"name": "Fire", "ticks": 12, "script": "onFire", "timeout": 0}
            ]),
            json!({}),
        );
        assert!(codes(&p).is_empty(), "{:?}", weapon_timing(&p, "x"));
    }

    #[test]
    fn full_auto_fire_with_the_trigger_held_is_not_an_idle_loop() {
        let p = pack(
            json!([
                {"name": "Ready", "down": 1},
                {"name": "Fire", "ticks": 6, "script": "onFire", "up": 0, "timeout": 1}
            ]),
            json!({}),
        );
        assert!(codes(&p).is_empty());
    }

    #[test]
    fn an_empty_gun_that_dry_fires_and_reloads_by_itself_is_an_idle_cycle() {
        let p = pack(
            json!([
                {"name": "Ready", "ticks": 0, "no_ammo": 1},
                {"name": "Dry", "ticks": 6, "script": "onDryFire", "timeout": 2},
                {"name": "Reload", "ticks": 60, "script": "onReload", "timeout": 0}
            ]),
            json!({"states": {"ondryfire": "t-rules:dry", "onreload": "t-rules:reload"}}),
        );
        let found = weapon_timing(&p, "t/assets/weapons.json");
        assert_eq!(found.len(), 1, "{found:?}");
        let d = &found[0];
        assert_eq!(d.code, "weapon.idle_cycle");
        assert!(
            d.message.contains("Ready → Dry → Reload → Ready"),
            "{}",
            d.message
        );
        assert!(d.message.contains("every 66 ticks"), "{}", d.message);
        assert!(d.message.contains("no ammunition"), "{}", d.message);
        assert!(
            d.message
                .contains("onDryFire (t-rules:dry), onReload (t-rules:reload)"),
            "{}",
            d.message
        );
        // Taking the edge off the resting state settles it.
        let settled = pack(
            json!([
                {"name": "Ready", "ticks": 0, "down": 1},
                {"name": "Dry", "ticks": 6, "script": "onDryFire", "timeout": 2},
                {"name": "Reload", "ticks": 60, "script": "onReload", "timeout": 0}
            ]),
            json!({"states": {"ondryfire": "t-rules:dry", "onreload": "t-rules:reload"}}),
        );
        assert!(codes(&settled).is_empty());
        // The engine's own callbacks end their loops (a pump shotgun's
        // reload check sets the image loaded), so they are not reported.
        let native = pack(
            json!([
                {"name": "Ready", "ticks": 0, "no_ammo": 1},
                {"name": "Dry", "ticks": 6, "script": "onDryFire", "timeout": 2},
                {"name": "Reload", "ticks": 60, "script": "onReload", "timeout": 0}
            ]),
            json!({}),
        );
        assert!(codes(&native).is_empty());
    }

    #[test]
    fn states_that_chain_without_time_are_a_zero_time_cycle() {
        let p = pack(
            json!([
                {"name": "Ready", "ammo": 1},
                {"name": "Check", "ticks": 0, "script": "onCheck", "no_ammo": 0, "ammo": 0}
            ]),
            json!({}),
        );
        let found = weapon_timing(&p, "x");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, "weapon.zero_time_cycle");
        assert!(found[0].message.contains("Ready → Check → Ready"));
        assert!(found[0].message.contains("unmounts"));
    }

    #[test]
    fn a_state_leading_to_itself_waits_a_tick() {
        let p = pack(
            json!([{"name": "Ready", "ticks": 0, "timeout": 0}]),
            json!({}),
        );
        assert!(codes(&p).is_empty());
    }

    #[test]
    fn a_callback_and_an_ammunition_edge_in_one_step_are_flagged() {
        let commands = json!({"states": {"onpreammocheck": "t-rules:check"}});
        let same_step = pack(
            json!([
                {"name": "Ready", "down": 1},
                {"name": "PreAmmoCheck", "ticks": 0, "script": "onPreAmmoCheck", "ammo": 0, "no_ammo": 0}
            ]),
            commands.clone(),
        );
        assert!(codes(&same_step).contains(&"weapon.callback_timing".to_string()));
        let next_step = pack(
            json!([
                {"name": "Ready", "down": 1},
                {"name": "PreAmmoCheck", "ticks": 1, "script": "onPreAmmoCheck", "ammo": 0, "no_ammo": 0}
            ]),
            commands,
        );
        assert!(!codes(&next_step).contains(&"weapon.callback_timing".to_string()));
    }

    fn behaviour(state: serde_json::Value) -> Behaviour {
        serde_json::from_value(json!({"schema_version": 1, "script": "a.rhai", "state": state}))
            .unwrap()
    }

    #[test]
    fn state_names_the_key_that_nears_a_limit() {
        let big = "x".repeat(3000);
        let small = behaviour(
            json!({"player": {"ammo_9mm": {"default": {"n": 30, "mags": [30, 30], "reserve": 32}}}}),
        );
        assert!(state_budget("t", &small).is_empty());
        let near = behaviour(
            json!({"player": {"ammo_9mm": {"default": {"n": 30}}, "log": {"default": big}}}),
        );
        let found = state_budget("t", &near);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, "state.value_large");
        assert!(found[0].message.contains("`log`"));
        let over = behaviour(json!({"player": {"log": {"default": "x".repeat(5000)}}}));
        let found = state_budget("t", &over);
        assert_eq!(found[0].code, "state.value_too_big");
        assert!(found[0].message.contains("`log`"), "{}", found[0].message);
    }
}
