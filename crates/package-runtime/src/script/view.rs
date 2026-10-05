//! Host views: what the host hands a script about one player, bot, brick,
//! entity, object, drop or mini-game, read like a map (`p.name`,
//! `"team" in p`, `p.keys()`).
//!
//! Rhai holds every script value to a size limit counted over everything
//! inside it: an array of maps counts all their entries and all their text
//! together. A list of the build's spawn bricks, the players with their
//! tools, or a team's members gathered by a script grew with the build and
//! the server, and past a few dozen of them the call threw (Slayer's spawn
//! picker, its team lists). A view is the host's value, not the script's:
//! Rhai does not count inside it, so what the host lists never meets a
//! script's own limits however big the build or the server. Each call
//! instead makes at most [`MAX_VIEWS`] distinct views (the same player or
//! brick asked again is the same view) and may write at most
//! [`MAX_VIEW_WRITES`] bytes into views, so a call's memory stays bounded.
use super::{Fallible, Invocation, fail, with};
use rhai::{Array, Dynamic, Engine, ImmutableString, Map};
use std::sync::Arc;

/// Most distinct views one call makes.
pub const MAX_VIEWS: usize = 16_384;
/// Most bytes one call writes into views (a field set on a view, and the
/// copy a view shared with another value takes before its first change).
pub const MAX_VIEW_WRITES: usize = 1 << 20;

/// One thing in the world, as a script reads it.
#[derive(Clone)]
pub struct View(Arc<Map>);

/// What a view is of, with its id, so the same one asked twice in a call
/// is the same view.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Of {
    Player(u64),
    Brick(u64),
    Entity(u64),
    Drop(u64),
    Minigame(u64),
}

/// The view of `of` (or an uncached one, for `None`), built by `build` the
/// first time this call asks for it. `build` gives a map.
pub(super) fn view_in(
    i: &mut Invocation,
    of: Option<Of>,
    build: impl FnOnce() -> Dynamic,
) -> Fallible<Dynamic> {
    if let Some(of) = of
        && let Some(v) = i.views.get(&of)
    {
        return Ok(Dynamic::from(v.clone()));
    }
    if i.views_made >= MAX_VIEWS {
        return fail(format!(
            "a call reads at most {MAX_VIEWS} players, bricks and other things"
        ));
    }
    i.views_made += 1;
    let map = build().try_cast::<Map>().unwrap_or_default();
    let v = View(Arc::new(map));
    if let Some(of) = of {
        i.views.insert(of, v.clone());
    }
    Ok(Dynamic::from(v))
}

/// Roughly how many bytes a value holds.
fn weight(value: &Dynamic) -> usize {
    if let Some(s) = value.read_lock::<ImmutableString>() {
        return 16 + s.len();
    }
    if let Some(a) = value.read_lock::<Array>() {
        return 16 + a.iter().map(weight).sum::<usize>();
    }
    if let Some(m) = value.read_lock::<Map>() {
        return 16 + m.iter().map(|(k, v)| k.len() + weight(v)).sum::<usize>();
    }
    if let Some(v) = value.read_lock::<View>() {
        return 16 + v.0.iter().map(|(k, v)| k.len() + weight(v)).sum::<usize>();
    }
    16
}

impl View {
    /// The view's own map to change, charged to the call's writes: `adding`
    /// bytes, and a copy of the map when another value shares it.
    fn make_mut(&mut self, adding: usize) -> Fallible<&mut Map> {
        let copy = if Arc::strong_count(&self.0) > 1 {
            self.0
                .iter()
                .map(|(k, v)| k.len() + weight(v))
                .sum::<usize>()
        } else {
            0
        };
        with(|i| {
            i.view_writes += copy + adding;
            if i.view_writes > MAX_VIEW_WRITES {
                return fail(format!(
                    "a call changes at most {MAX_VIEW_WRITES} bytes of what it reads; copy what it keeps into its own map"
                ));
            }
            Ok(())
        })?;
        Ok(Arc::make_mut(&mut self.0))
    }
    fn map(&self) -> Dynamic {
        Dynamic::from_map(self.0.as_ref().clone())
    }
}

/// `value` with each view in it turned back into a plain map, for what
/// leaves the script (state, operations, a hook's answer).
pub fn plain(value: &Dynamic) -> Dynamic {
    if let Some(v) = value.read_lock::<View>() {
        return Dynamic::from_map(v.0.iter().map(|(k, x)| (k.clone(), plain(x))).collect());
    }
    if let Some(a) = value.read_lock::<Array>() {
        return Dynamic::from_array(a.iter().map(plain).collect());
    }
    if let Some(m) = value.read_lock::<Map>() {
        return Dynamic::from_map(m.iter().map(|(k, x)| (k.clone(), plain(x))).collect());
    }
    value.clone()
}

/// Two views are the same thing when they share a map, or name the same
/// id with the same fields.
fn same(a: &View, b: &View) -> bool {
    if Arc::ptr_eq(&a.0, &b.0) {
        return true;
    }
    let id = |v: &View| v.0.get("id").and_then(|i| i.as_int().ok());
    id(a).is_some()
        && id(a) == id(b)
        && a.0.len() == b.0.len()
        && a.0.keys().zip(b.0.keys()).all(|(x, y)| x == y)
}

pub(super) fn register(engine: &mut Engine) {
    // `type_of(p)` is "map", as before views.
    engine.register_type_with_name::<View>("map");
    // `p.name` and `p["name"]` (a property a view lacks is `()`, as a
    // map's is), and `p.name = ...` on the script's own copy.
    engine.register_indexer_get(|v: &mut View, key: &str| {
        v.0.get(key).cloned().unwrap_or(Dynamic::UNIT)
    });
    engine.register_indexer_set(|v: &mut View, key: &str, value: Dynamic| -> Fallible<()> {
        let adding = key.len() + weight(&value);
        v.make_mut(adding)?.insert(key.into(), value);
        Ok(())
    });
    engine.register_fn("contains", |v: &mut View, key: &str| v.0.contains_key(key));
    engine.register_fn("keys", |v: &mut View| {
        v.0.keys()
            .map(|k| Dynamic::from(ImmutableString::from(k.as_str())))
            .collect::<Array>()
    });
    engine.register_fn("values", |v: &mut View| {
        v.0.values().cloned().collect::<Array>()
    });
    engine.register_fn("len", |v: &mut View| v.0.len() as i64);
    engine.register_fn("is_empty", |v: &mut View| v.0.is_empty());
    engine.register_fn("remove", |v: &mut View, key: &str| -> Fallible<Dynamic> {
        Ok(v.make_mut(0)?.remove(key).unwrap_or(Dynamic::UNIT))
    });
    // The script's own map of it, counted against its limits like any.
    engine.register_fn("to_map", |v: &mut View| v.map());
    engine.register_fn("+", |a: View, b: Map| {
        let mut m = a.0.as_ref().clone();
        m.extend(b);
        m
    });
    engine.register_fn("+", |a: Map, b: View| {
        let mut m = a;
        m.extend(b.0.as_ref().clone());
        m
    });
    engine.register_fn("==", |a: View, b: View| same(&a, &b));
    engine.register_fn("!=", |a: View, b: View| !same(&a, &b));
    engine.register_fn("to_string", |v: &mut View| v.map().to_string());
    engine.register_fn("to_debug", |v: &mut View| format!("{:?}", v.map()));
    let mut iter = rhai::Module::new();
    iter.set_iter(std::any::TypeId::of::<View>(), |v: Dynamic| {
        let keys: Vec<Dynamic> = v
            .try_cast::<View>()
            .map(|v| {
                v.0.keys()
                    .map(|k| Dynamic::from(ImmutableString::from(k.as_str())))
                    .collect()
            })
            .unwrap_or_default();
        Box::new(keys.into_iter())
    });
    engine.register_global_module(iter.into());
}
