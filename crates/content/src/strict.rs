//! JSON that reports the keys nothing reads.
//!
//! The structs content is read into mostly accept unknown keys, so a
//! misspelled optional key (`alpha_cutoff`, `looping`, `muzzle`) is ignored
//! and the field keeps its default. Reading through [`from_slice`] reports each
//! ignored key as a counted fallback (`bri_console::fallback`, kind
//! `ignored_key`) naming the struct and the key's path, under the owner the
//! loader set with `bri_console::fallback::with_owner`.
use serde::de::DeserializeOwned;

/// `bytes` as a `T`, reporting every key in it that `T` did not read.
pub fn from_slice<T: DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = serde_ignored::deserialize(&mut de, report::<T>)?;
    de.end()?;
    Ok(value)
}

/// `value` as a `T`, reporting every key in it that `T` did not read.
pub fn from_value<T: DeserializeOwned>(value: &serde_json::Value) -> serde_json::Result<T> {
    serde_ignored::deserialize(value, report::<T>)
}

fn report<T>(path: serde_ignored::Path<'_>) {
    bri_console::fallback::report(
        bri_console::fallback::kind::IGNORED_KEY,
        "",
        &format!("{}: {path}", short(std::any::type_name::<T>())),
        "ignored (nothing reads this key)",
    );
}

/// `a::b::Shape` as `Shape`, and `Vec<a::b::Shape>` kept readable.
fn short(name: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            out.push_str(word.rsplit("::").next().unwrap_or(""));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(word.rsplit("::").next().unwrap_or(""));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Light {
        #[serde(default)]
        radius: f32,
        #[serde(default)]
        inner: Vec<Spot>,
    }
    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Spot {
        #[serde(default)]
        angle: f32,
    }

    #[test]
    fn an_ignored_key_is_reported_with_its_struct_and_path_and_the_value_still_reads() {
        let text = br#"{"radius": 2.0, "radiuss": 3.0, "inner": [{"angle": 1.0, "angel": 2.0}]}"#;
        let light: Light =
            bri_console::fallback::with_owner("strict-test-owner", || from_slice(text).unwrap());
        assert_eq!(light.radius, 2.0);
        assert_eq!(light.inner[0].angle, 1.0);
        let mine: Vec<_> = bri_console::fallback::snapshot()
            .into_iter()
            .filter(|e| e.addon == "strict-test-owner")
            .collect();
        let items: Vec<&str> = mine.iter().map(|e| e.item.as_str()).collect();
        assert!(items.contains(&"Light: radiuss"), "{items:?}");
        assert!(items.contains(&"Light: inner.0.angel"), "{items:?}");
        assert!(mine.iter().all(|e| e.kind == "ignored_key"));
        // Every key read: nothing reported.
        let before = bri_console::fallback::snapshot().len();
        let ok: Light = from_slice(br#"{"radius": 1.0}"#).unwrap();
        assert_eq!(ok.radius, 1.0);
        assert_eq!(bri_console::fallback::snapshot().len(), before);
    }
}
