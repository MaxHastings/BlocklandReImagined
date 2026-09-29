//! Structure-aware damage for fuzzing anything that travels as JSON (saves,
//! commands): swap values for extremes, drop keys, cut or repeat lists. A
//! damaged value either fails to decode or must be handled cleanly.
use proptest::prelude::*;
use serde_json::Value;

/// One change to a JSON value; indices wrap around what the value holds.
#[derive(Debug, Clone)]
pub enum Change {
    Number(usize, f64),
    Integer(usize, i64),
    Text(usize, String),
    Flip(usize),
    Drop(usize),
    Repeat(usize, usize),
    Cut(usize, usize),
}

/// Changes weighted towards values that break code: extremes, empties,
/// control characters, huge strings and lists.
pub fn change() -> impl Strategy<Value = Change> {
    let numbers = prop_oneof![
        Just(1e39),
        Just(-1e39),
        Just(3.4e38),
        Just(1e-45),
        Just(0.0),
        Just(-0.0),
        Just(-1.0),
        Just(0.5),
        Just(1e9),
        -1e6f64..1e6,
    ];
    let integers = prop_oneof![
        Just(-1i64),
        Just(0),
        Just(255),
        Just(256),
        Just(65536),
        Just(u32::MAX as i64 + 1),
        Just(i64::MAX),
        Just(i64::MIN),
    ];
    let texts = prop_oneof![
        Just(String::new()),
        Just("x".repeat(70_000)),
        Just("../../../etc/passwd".to_string()),
        Just("\u{0}\u{7}\u{202e}\u{fffd}".to_string()),
        Just("v20.brick.nonexistent".to_string()),
        Just("chaos/brick/1x1".to_string()),
        "[a-zA-Z0-9 ]{0,12}",
    ];
    prop_oneof![
        3 => (any::<usize>(), numbers).prop_map(|(at, v)| Change::Number(at, v)),
        3 => (any::<usize>(), integers).prop_map(|(at, v)| Change::Integer(at, v)),
        2 => (any::<usize>(), texts).prop_map(|(at, v)| Change::Text(at, v)),
        1 => any::<usize>().prop_map(Change::Flip),
        2 => any::<usize>().prop_map(Change::Drop),
        1 => (any::<usize>(), 1usize..300).prop_map(|(at, n)| Change::Repeat(at, n)),
        1 => (any::<usize>(), any::<usize>()).prop_map(|(at, n)| Change::Cut(at, n)),
    ]
}

/// Every node's JSON pointer, parents before children.
pub fn pointers(value: &Value, here: String, out: &mut Vec<String>) {
    out.push(here.clone());
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                pointers(
                    v,
                    format!("{here}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                pointers(v, format!("{here}/{i}"), out);
            }
        }
        _ => {}
    }
}

/// Apply `change` to `value` in place.
pub fn apply(value: &mut Value, change: &Change) {
    let mut all = Vec::new();
    pointers(value, String::new(), &mut all);
    let of = |kind: fn(&Value) -> bool, at: usize, value: &Value| {
        let matching: Vec<_> = all
            .iter()
            .filter(|p| kind(value.pointer(p).unwrap()))
            .collect();
        (!matching.is_empty()).then(|| matching[at % matching.len()].clone())
    };
    match change {
        Change::Number(at, v) => {
            if let Some(p) =
                of(Value::is_f64, *at, value).or_else(|| of(Value::is_number, *at, value))
            {
                *value.pointer_mut(&p).unwrap() = serde_json::json!(v);
            }
        }
        Change::Integer(at, v) => {
            if let Some(p) = of(Value::is_number, *at, value) {
                *value.pointer_mut(&p).unwrap() = serde_json::json!(v);
            }
        }
        Change::Text(at, v) => {
            if let Some(p) = of(Value::is_string, *at, value) {
                *value.pointer_mut(&p).unwrap() = Value::String(v.clone());
            }
        }
        Change::Flip(at) => {
            if let Some(p) = of(Value::is_boolean, *at, value) {
                let slot = value.pointer_mut(&p).unwrap();
                *slot = Value::Bool(!slot.as_bool().unwrap());
            }
        }
        Change::Drop(at) => {
            // Any key of any object, or any element of any list; a bare
            // value has none.
            if all.len() < 2 {
                return;
            }
            let p = &all[1 + at % (all.len() - 1)];
            let (parent, last) = p.rsplit_once('/').unwrap();
            match value.pointer_mut(parent) {
                Some(Value::Object(map)) => {
                    map.remove(&last.replace("~1", "/").replace("~0", "~"));
                }
                Some(Value::Array(items)) => {
                    items.remove(last.parse::<usize>().unwrap());
                }
                _ => {}
            }
        }
        Change::Repeat(at, n) => {
            if let Some(p) = of(|v| v.as_array().is_some_and(|a| !a.is_empty()), *at, value) {
                let items = value.pointer_mut(&p).unwrap().as_array_mut().unwrap();
                let first = items[0].clone();
                items.extend(std::iter::repeat_n(first, *n));
            }
        }
        Change::Cut(at, n) => {
            if let Some(p) = of(Value::is_array, *at, value) {
                let items = value.pointer_mut(&p).unwrap().as_array_mut().unwrap();
                let keep = n % (items.len() + 1);
                items.truncate(keep);
            }
        }
    }
}
