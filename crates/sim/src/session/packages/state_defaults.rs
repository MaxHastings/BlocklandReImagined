//! Native defaults use the same state-size accounting as script commits.
//! Admission is atomic for one package/global or package/player map and retains
//! authored data. Only that existing bounded value map is cloned for admission.
use bri_package_runtime::{
    Diagnostic, PlayerKey, Store,
    content::StateKey,
    state::{self, stored_size},
};
use std::collections::BTreeMap;

pub(super) fn initialize(
    store: &mut Store,
    total: &mut usize,
    package: &str,
    key: &PlayerKey,
    declarations: &BTreeMap<String, StateKey>,
) -> Result<bool, Diagnostic> {
    if declarations.is_empty() {
        return Ok(false);
    }
    let namespace = store.namespace(package);
    let old = namespace.and_then(|ns| ns.players.get(key));
    if old.is_some_and(|values| declarations.keys().all(|name| values.contains_key(name))) {
        return Ok(false);
    }
    let mut values = old.cloned().unwrap_or_default();
    let mut changed = false;
    for (name, declaration) in declarations {
        if !values.contains_key(name) {
            values.insert(name.clone(), declaration.default.clone());
            changed = true;
        }
    }
    if !changed {
        return Ok(false);
    }
    let before = old.map_or(0, stored_size);
    let after = stored_size(&values);
    // Store::stored_size charges this framing once for a new namespace;
    // empty declarations above do not create a namespace or player entry.
    let framing = if namespace.is_none() {
        package.len() + 64
    } else {
        0
    };
    let new_total = admitted_total(
        *total,
        before,
        after,
        framing,
        package,
        (state::MAX_PLAYER_STATE_BYTES, "a player's"),
    )?;
    store
        .namespace_mut(package)
        .players
        .insert(key.clone(), values);
    *total = new_total;
    Ok(true)
}

pub(super) fn initialize_global(
    store: &mut Store,
    total: &mut usize,
    package: &str,
    declarations: &BTreeMap<String, StateKey>,
) -> Result<bool, Diagnostic> {
    let namespace = store.namespace(package);
    let old = namespace.map(|ns| &ns.global);
    if old.is_some_and(|values| declarations.keys().all(|name| values.contains_key(name))) {
        return Ok(false);
    }
    let mut values = old.cloned().unwrap_or_default();
    let mut changed = namespace.is_none();
    for (name, declaration) in declarations {
        if !values.contains_key(name) {
            values.insert(name.clone(), declaration.default.clone());
            changed = true;
        }
    }
    if !changed {
        return Ok(false);
    }
    let before = old.map_or(0, stored_size);
    let after = stored_size(&values);
    let framing = if namespace.is_none() {
        package.len() + 64
    } else {
        0
    };
    let new_total = admitted_total(
        *total,
        before,
        after,
        framing,
        package,
        (state::MAX_GLOBAL_STATE_BYTES, "server-wide"),
    )?;
    // Installation historically creates even an empty global namespace.
    // That namespace framing is admitted and charged before creation.
    store.namespace_mut(package).global = values;
    *total = new_total;
    Ok(true)
}

fn admitted_total(
    total: usize,
    before: usize,
    after: usize,
    framing: usize,
    package: &str,
    limit: (usize, &str),
) -> Result<usize, Diagnostic> {
    let rejected = |message: String| {
        Diagnostic::error("state.budget", message)
            .at(package.to_string())
            .hint("keep state small; store counts, not logs")
    };
    if after > before && after > limit.0 {
        return Err(rejected(format!(
            "defaults would grow {} state to {after} bytes; the limit is {}",
            limit.1, limit.0
        )));
    }
    let growth = after.saturating_sub(before).checked_add(framing);
    let new_total = growth.and_then(|growth| total.checked_add(growth));
    new_total
        .filter(|n| growth == Some(0) || *n <= state::MAX_STATE_BYTES)
        .ok_or_else(|| {
            rejected(format!(
                "defaults would grow package state past the server's {} bytes",
                state::MAX_STATE_BYTES
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bri_package_runtime::content::Visible;
    use serde_json::json;

    fn declaration(value: serde_json::Value) -> StateKey {
        StateKey {
            default: value,
            visible: Visible::default(),
            persist: false,
            per_minigame: false,
        }
    }

    #[test]
    fn preserves_authored_values_is_idempotent_and_cleanup_recovers_accounting() {
        let mut store = Store::default();
        let player = PlayerKey::session(7);
        let ns = store.namespace_mut("parcel");
        ns.global.insert("clock".into(), json!(12));
        ns.players.insert(
            player.clone(),
            BTreeMap::from([("returns".into(), json!(9))]),
        );
        let original = store.clone();
        let mut total = store.stored_size();
        let declarations = BTreeMap::from([
            ("returns".into(), declaration(json!(0))),
            ("carrying".into(), declaration(json!(null))),
        ]);
        assert!(initialize(&mut store, &mut total, "parcel", &player, &declarations).unwrap());
        assert_eq!(store.player("parcel", &player, "returns"), Some(&json!(9)));
        assert_eq!(
            store.player("parcel", &player, "carrying"),
            Some(&json!(null))
        );
        assert_eq!(total, store.stored_size());
        let initialized = store.clone();
        let initialized_total = total;
        assert!(!initialize(&mut store, &mut total, "parcel", &player, &declarations).unwrap());
        assert_eq!(store, initialized);
        assert_eq!(total, initialized_total);
        // Same subtraction as native bot-state cleanup. Original authored
        // player values also go with that bot; the namespace/global remains.
        let removed = store
            .namespace_mut("parcel")
            .players
            .remove(&player)
            .unwrap();
        total -= stored_size(&removed);
        let mut original_without_player = original;
        original_without_player
            .namespace_mut("parcel")
            .players
            .remove(&player);
        assert_eq!(store, original_without_player);
        assert_eq!(total, original_without_player.stored_size());
    }

    #[test]
    fn empty_declarations_create_nothing_and_new_namespace_framing_is_counted() {
        let mut store = Store::default();
        let mut total = 0;
        let player = PlayerKey::session(1);
        assert!(!initialize(&mut store, &mut total, "fresh", &player, &BTreeMap::new()).unwrap());
        assert_eq!(store, Store::default());
        assert_eq!(total, 0);
        let declarations = BTreeMap::from([("returns".into(), declaration(json!(0)))]);
        assert!(initialize(&mut store, &mut total, "fresh", &player, &declarations).unwrap());
        assert_eq!(total, store.stored_size());
        assert_eq!(
            total,
            "fresh".len() + 64 + stored_size(&store.namespace("fresh").unwrap().players[&player])
        );
    }

    #[test]
    fn per_player_growth_is_rejected_without_partial_default_or_count_changes() {
        let mut store = Store::default();
        let player = PlayerKey::session(1);
        store.namespace_mut("large").players.insert(
            player.clone(),
            BTreeMap::from([("authored".into(), json!(13))]),
        );
        let mut total = store.stored_size();
        let original = store.clone();
        let original_total = total;
        // Individually valid values; aggregate player defaults exceed 64KiB.
        let declarations = (0..20)
            .map(|n| (format!("value_{n}"), declaration(json!("x".repeat(3500)))))
            .collect();
        let error =
            initialize(&mut store, &mut total, "large", &player, &declarations).unwrap_err();
        assert_eq!(error.code, "state.budget");
        assert_eq!(store, original);
        assert_eq!(total, original_total);
    }

    #[test]
    fn aggregate_growth_and_namespace_framing_are_atomic_at_the_budget_edge() {
        let player = PlayerKey::session(1);
        let declarations = BTreeMap::from([("count".into(), declaration(json!(0)))]);
        for existing_namespace in [false, true] {
            let mut store = Store::default();
            if existing_namespace {
                store.namespace_mut("edge");
            }
            let original = store.clone();
            let mut total = state::MAX_STATE_BYTES - 1;
            let original_total = total;
            assert!(initialize(&mut store, &mut total, "edge", &player, &declarations).is_err());
            assert_eq!(store, original);
            assert_eq!(total, original_total);
            let value = BTreeMap::from([("count".into(), json!(0))]);
            let growth = stored_size(&value)
                + if existing_namespace {
                    0
                } else {
                    "edge".len() + 64
                };
            total = state::MAX_STATE_BYTES - growth;
            assert!(initialize(&mut store, &mut total, "edge", &player, &declarations).unwrap());
            assert_eq!(total, state::MAX_STATE_BYTES);
            // Repeating initialization admits no growth even at the limit.
            assert!(!initialize(&mut store, &mut total, "edge", &player, &declarations).unwrap());
            assert_eq!(total, state::MAX_STATE_BYTES);
        }
    }

    #[test]
    fn global_defaults_preserve_saved_values_and_count_empty_namespace_framing() {
        let mut store = Store::default();
        let mut total = 0;
        assert!(initialize_global(&mut store, &mut total, "saved", &BTreeMap::new()).unwrap());
        assert_eq!(total, store.stored_size());
        assert_eq!(total, "saved".len() + 64);
        assert!(store.namespace("saved").unwrap().players.is_empty());
        assert!(!initialize_global(&mut store, &mut total, "saved", &BTreeMap::new()).unwrap());
        store
            .namespace_mut("saved")
            .global
            .insert("epoch".into(), json!(17));
        total = store.stored_size();
        let declarations = BTreeMap::from([
            ("epoch".into(), declaration(json!(0))),
            ("sources".into(), declaration(json!({}))),
        ]);
        assert!(initialize_global(&mut store, &mut total, "saved", &declarations).unwrap());
        assert_eq!(store.global("saved", "epoch"), Some(&json!(17)));
        assert_eq!(store.global("saved", "sources"), Some(&json!({})));
        assert_eq!(total, store.stored_size());
        let initialized = store.clone();
        let initialized_total = total;
        assert!(!initialize_global(&mut store, &mut total, "saved", &declarations).unwrap());
        assert_eq!(store, initialized);
        assert_eq!(total, initialized_total);
    }

    #[test]
    fn global_and_aggregate_limits_reject_atomic_growth_and_admit_the_exact_edge() {
        let mut store = Store::default();
        store
            .namespace_mut("saved")
            .global
            .insert("epoch".into(), json!(17));
        let mut total = store.stored_size();
        let original = store.clone();
        let original_total = total;
        let oversized = (0..80)
            .map(|n| (format!("value_{n}"), declaration(json!("x".repeat(3500)))))
            .collect();
        assert!(initialize_global(&mut store, &mut total, "saved", &oversized).is_err());
        assert_eq!(store, original);
        assert_eq!(total, original_total);
        // Aggregate near-limit setup avoids allocating a 63MiB fixture.
        let declarations = BTreeMap::from([("count".into(), declaration(json!(0)))]);
        let mut missing = Store::default();
        total = state::MAX_STATE_BYTES - 1;
        assert!(initialize_global(&mut missing, &mut total, "fresh", &declarations).is_err());
        assert_eq!(missing, Store::default());
        assert_eq!(total, state::MAX_STATE_BYTES - 1);
        let values = BTreeMap::from([("count".into(), json!(0))]);
        let growth = stored_size(&values) + "fresh".len() + 64;
        total = state::MAX_STATE_BYTES - growth;
        assert!(initialize_global(&mut missing, &mut total, "fresh", &declarations).unwrap());
        assert_eq!(total, state::MAX_STATE_BYTES);
    }

    #[test]
    fn unchanged_legacy_oversized_maps_do_not_fail_growth_only_admission() {
        let player = PlayerKey::session(1);
        let mut store = Store::default();
        let values: BTreeMap<_, _> = (0..80)
            .map(|n| (format!("value_{n}"), json!("x".repeat(3500))))
            .collect();
        let ns = store.namespace_mut("legacy");
        ns.global = values.clone();
        ns.players.insert(player.clone(), values);
        let declarations = BTreeMap::from([("value_0".into(), declaration(json!("different")))]);
        let original = store.clone();
        let mut total = state::MAX_STATE_BYTES + 1;
        assert!(!initialize(&mut store, &mut total, "legacy", &player, &declarations).unwrap());
        assert!(!initialize_global(&mut store, &mut total, "legacy", &declarations).unwrap());
        assert_eq!(store, original);
        assert_eq!(total, state::MAX_STATE_BYTES + 1);
    }
}
