//! Counted, named fallbacks: the one place a loader says "I could not honour
//! this, so I used that".
//!
//! A loader that meets bad or missing input and carries on with something
//! else (a stand-in texture, a default value, an item left out) calls
//! [`report`] with what kind of thing it was, which Add-On it came from, which
//! item it was, and what it used instead. Reports are counted and kept once
//! per distinct (kind, Add-On, item, fallback); the first of each is also
//! written to the console log as a warning. A checking run (`bri-client
//! --check`) writes the whole list to `fallbacks.json`, and the push gate and
//! the packager refuse a build with a fallback that is not on the stock
//! Add-On allow list.
//!
//! This is not for input that is optional by design; an absent optional key
//! with a documented default is not a fallback. It is for input that was
//! present and wrong, or required and missing, where the game carried on
//! anyway.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Distinct fallbacks kept; past this many the rest are only counted in
/// [`overflow`], so a badly broken Add-On cannot grow the list without bound.
pub const MAX_ENTRIES: usize = 2048;

/// What kinds of thing can fall back. Free text would let spellings drift,
/// so loaders name one of these.
pub mod kind {
    /// An image, shape or material that could not be found or read, replaced
    /// by a stand-in.
    pub const MISSING_TEXTURE: &str = "missing_texture";
    /// A particle group, emitter or light dropped.
    pub const DROPPED_PARTICLE_GROUP: &str = "dropped_particle_group";
    /// An animation role, clip, socket or binding that matched nothing.
    pub const UNKNOWN_ROLE: &str = "unknown_role";
    /// A field that was present but unusable, replaced by a default.
    pub const DEFAULTED_FIELD: &str = "defaulted_field";
    /// A key no reader knows, ignored.
    pub const IGNORED_KEY: &str = "ignored_key";
    /// An object, node, element or layer left out.
    pub const DROPPED_ITEM: &str = "dropped_item";
    /// A sound that could not be found or read.
    pub const MISSING_SOUND: &str = "sound";
    /// A model or shape that could not be found or read.
    pub const MISSING_MODEL: &str = "missing_model";
    /// Something an Add-On names (an item, image, projectile, explosion,
    /// rules part, dependency) that does not exist.
    pub const MISSING_REFERENCE: &str = "missing_reference";
    /// Anything else that carried on with something other than asked.
    pub const OTHER: &str = "other";
}

/// One distinct fallback and how often it happened.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    pub kind: String,
    /// The Add-On's id, or the map or pack it came from when no Add-On is
    /// known.
    pub addon: String,
    /// The asset, field or node that could not be honoured.
    pub item: String,
    /// What was used instead, in plain words.
    pub fallback: String,
    pub count: u64,
}

#[derive(Default)]
struct Store {
    entries: BTreeMap<(String, String, String, String), u64>,
    overflow: u64,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    entries: BTreeMap::new(),
    overflow: 0,
});

fn with<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    let mut store = STORE.lock().unwrap_or_else(|poison| poison.into_inner());
    f(&mut store)
}

thread_local! {
    static OWNER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` with `owner` (a map or Add-On id) standing in for the Add-On of
/// every report made on this thread inside it that names none, so a loader
/// states whose content it is reading once instead of at every site.
pub fn with_owner<R>(owner: &str, f: impl FnOnce() -> R) -> R {
    OWNER.with(|o| o.borrow_mut().push(owner.to_owned()));
    let result = f();
    OWNER.with(|o| o.borrow_mut().pop());
    result
}

/// Record that `item` of `addon` could not be honoured and `fallback` was used
/// instead. The first report of each distinct fallback is a console warning.
pub fn report(kind: &str, addon: &str, item: &str, fallback: &str) {
    let addon = if addon.is_empty() {
        OWNER.with(|o| o.borrow().last().cloned().unwrap_or_default())
    } else {
        addon.to_owned()
    };
    let addon = addon.as_str();
    let first = with(|store| {
        let key = (
            kind.to_owned(),
            addon.to_owned(),
            item.to_owned(),
            fallback.to_owned(),
        );
        if let Some(count) = store.entries.get_mut(&key) {
            *count += 1;
            return false;
        }
        if store.entries.len() >= MAX_ENTRIES {
            store.overflow += 1;
            return false;
        }
        store.entries.insert(key, 1);
        true
    });
    if first {
        let whose = if addon.is_empty() {
            String::new()
        } else {
            format!(" in {addon}")
        };
        crate::log::warn(format!("Fallback ({kind}): {item}{whose} -> {fallback}"));
    }
}

/// Every fallback reported so far, in a stable order.
pub fn snapshot() -> Vec<Entry> {
    with(|store| {
        store
            .entries
            .iter()
            .map(|((kind, addon, item, fallback), count)| Entry {
                kind: kind.clone(),
                addon: addon.clone(),
                item: item.clone(),
                fallback: fallback.clone(),
                count: *count,
            })
            .collect()
    })
}

/// Reports dropped because [`MAX_ENTRIES`] distinct ones were already kept.
pub fn overflow() -> u64 {
    with(|store| store.overflow)
}

/// Forget everything: a new load starts a new count.
pub fn clear() {
    with(|store| {
        store.entries.clear();
        store.overflow = 0;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // The store is process-wide: tests that read it take turns.
    static TURN: Mutex<()> = Mutex::new(());

    #[test]
    fn reports_are_counted_once_per_distinct_fallback() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        report(
            kind::MISSING_TEXTURE,
            "a-pack",
            "smoke.png",
            "a white square",
        );
        report(
            kind::MISSING_TEXTURE,
            "a-pack",
            "smoke.png",
            "a white square",
        );
        report(
            kind::MISSING_TEXTURE,
            "a-pack",
            "smoke.png",
            "a grey square",
        );
        report(kind::DROPPED_ITEM, "", "node 7", "left out");
        let all = snapshot();
        assert_eq!(all.len(), 3);
        let smoke = all.iter().find(|e| e.fallback == "a white square").unwrap();
        assert_eq!(smoke.count, 2);
        assert_eq!(smoke.kind, "missing_texture");
        assert_eq!(smoke.addon, "a-pack");
        clear();
        assert!(snapshot().is_empty());
    }

    #[test]
    fn an_owner_scope_names_the_addon_of_reports_that_name_none() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        with_owner("outer-map", || {
            report(kind::OTHER, "", "one", "x");
            with_owner("inner-map", || report(kind::OTHER, "", "two", "x"));
            report(kind::OTHER, "named", "three", "x");
        });
        report(kind::OTHER, "", "four", "x");
        let by_item = |item: &str| {
            snapshot()
                .into_iter()
                .find(|e| e.item == item)
                .unwrap()
                .addon
        };
        assert_eq!(by_item("one"), "outer-map");
        assert_eq!(by_item("two"), "inner-map");
        assert_eq!(by_item("three"), "named");
        assert_eq!(by_item("four"), "");
        clear();
    }

    #[test]
    fn the_list_is_bounded_and_the_rest_counted() {
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        for n in 0..MAX_ENTRIES + 5 {
            report(kind::OTHER, "x", &format!("item {n}"), "nothing");
        }
        assert_eq!(snapshot().len(), MAX_ENTRIES);
        assert_eq!(overflow(), 5);
        clear();
        assert_eq!(overflow(), 0);
    }
}
