//! `$pref::` values with TorqueScript's case-insensitive names. Stock
//! defaults come from the UI pack; user overrides live in `Settings::prefs`.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prefs {
    defaults: BTreeMap<String, (String, String)>,
    values: BTreeMap<String, (String, String)>,
}

fn norm(key: &str) -> String {
    let k = key.trim_start_matches('$').to_ascii_lowercase();
    format!("${k}")
}

impl Prefs {
    pub fn new(defaults: &BTreeMap<String, String>, overrides: &BTreeMap<String, String>) -> Self {
        let mut p = Prefs::default();
        for (k, v) in defaults {
            p.defaults.insert(norm(k), (k.clone(), v.clone()));
        }
        for (k, v) in overrides {
            p.values.insert(norm(k), (k.clone(), v.clone()));
        }
        p
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        let n = norm(key);
        self.values
            .get(&n)
            .or_else(|| self.defaults.get(&n))
            .map(|(_, v)| v.as_str())
    }
    pub fn str_or<'a>(&'a self, key: &str, d: &'a str) -> &'a str {
        self.get(key).unwrap_or(d)
    }
    /// TorqueScript truthiness: non-zero number or non-empty non-numeric.
    pub fn bool_or(&self, key: &str, d: bool) -> bool {
        match self.get(key) {
            None => d,
            Some(v) => match v.trim().parse::<f64>() {
                Ok(n) => n != 0.0,
                Err(_) => !v.trim().is_empty(),
            },
        }
    }
    pub fn f32_or(&self, key: &str, d: f32) -> f32 {
        self.get(key)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(d)
    }
    pub fn i64_or(&self, key: &str, d: i64) -> i64 {
        self.get(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .map_or(d, |v| v as i64)
    }
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.values
            .insert(norm(key), (key.to_string(), value.into()));
    }
    pub fn set_bool(&mut self, key: &str, v: bool) {
        self.set(key, if v { "1" } else { "0" });
    }
    /// User overrides (for `Settings::prefs`).
    pub fn overrides(&self) -> BTreeMap<String, String> {
        self.values
            .values()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
    pub fn reset(&mut self, key: &str) {
        self.values.remove(&norm(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_with_defaults() {
        let d = BTreeMap::from([("$pref::HUD::HideBrickBox".to_string(), "1".to_string())]);
        let mut p = Prefs::new(&d, &BTreeMap::new());
        assert!(p.bool_or("$Pref::Hud::hidebrickbox", false));
        p.set_bool("$pref::hud::HideBrickBox", false);
        assert!(!p.bool_or("$pref::HUD::HideBrickBox", true));
        assert_eq!(p.overrides().len(), 1);
        assert_eq!(p.f32_or("$missing", 0.5), 0.5);
    }
}
