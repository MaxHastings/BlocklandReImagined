//! Bounded, data-only presentation bindings. Adapters supply named facts;
//! these types know nothing about vehicles, weapons, lights or game rules.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// A cone in a source's local frame, usable by any attached light.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightCone {
    pub inner: f32,
    pub outer: f32,
    #[serde(default = "LightCone::forward")]
    pub axis: [f32; 3],
}
impl LightCone {
    fn forward() -> [f32; 3] {
        [0., 0., -1.]
    }
    pub fn valid(&self) -> bool {
        (0.0..=180.0).contains(&self.inner)
            && self.outer.is_finite()
            && self.outer > self.inner
            && self.outer <= 180.
            && self.axis.iter().all(|v| v.is_finite())
            && self.axis.iter().any(|v| *v != 0.)
    }
    /// The same linear angular falloff used by every native light consumer.
    pub fn words(&self, rotation: glam::Quat) -> [f32; 4] {
        cone_words(
            rotation * glam::Vec3::from(self.axis),
            self.inner,
            self.outer,
        )
    }
}
pub const ALL_ROUND: [f32; 4] = [0., 0., 0., -1.];
pub fn cone_words(direction: glam::Vec3, inner: f32, outer: f32) -> [f32; 4] {
    let direction = direction.normalize_or_zero();
    let (inner, outer) = (inner.to_radians().cos(), outer.to_radians().cos());
    if direction == glam::Vec3::ZERO || !(inner.is_finite() && outer.is_finite()) {
        return ALL_ROUND;
    }
    let k = 1. / (inner - outer).max(0.001);
    [direction.x * k, direction.y * k, direction.z * k, outer * k]
}

/// All required facts, at least one alternative (when present), and no excluded
/// facts. Missing facts are errors, including under `none`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub none: Vec<String>,
}
impl Condition {
    pub fn signals(&self) -> impl Iterator<Item = &str> {
        self.all
            .iter()
            .chain(&self.any)
            .chain(&self.none)
            .map(String::as_str)
    }
    pub fn validate(&self, supports: impl Fn(&str) -> bool) -> Result<()> {
        ensure!(
            self.all.len() + self.any.len() + self.none.len() <= 32,
            "Too many presentation condition inputs"
        );
        for name in self.signals() {
            ensure!(
                !name.is_empty() && name.len() <= 64 && !name.chars().any(char::is_control),
                "Invalid presentation signal name"
            );
            ensure!(supports(name), "Unsupported presentation signal {name}");
        }
        Ok(())
    }
    pub fn evaluate(&self, value: impl Fn(&str) -> bool) -> bool {
        self.all.iter().all(|s| value(s))
            && (self.any.is_empty() || self.any.iter().any(|s| value(s)))
            && !self.none.iter().any(|s| value(s))
    }
}

/// A prebuilt object's appearance. None preserves its authored textures;
/// a color replaces the object's texture without affecting shared materials
/// on other objects. Visibility can select alternate authored geometry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectAppearance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 4]>,
    #[serde(default = "visible")]
    pub visible: bool,
}
fn visible() -> bool {
    true
}
impl Default for ObjectAppearance {
    fn default() -> Self {
        Self {
            color: None,
            visible: true,
        }
    }
}

/// A source-independent two-state model binding: indicators, alternate parts,
/// lenses, power displays, or any other authored object set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectVariant {
    pub objects: Vec<String>,
    pub when: Condition,
    #[serde(default)]
    pub on: ObjectAppearance,
    #[serde(default)]
    pub off: ObjectAppearance,
}
impl ObjectVariant {
    pub fn validate(&self, supports: impl Fn(&str) -> bool) -> Result<()> {
        ensure!(
            !self.objects.is_empty() && self.objects.len() <= 64,
            "Invalid presentation object count"
        );
        ensure!(
            self.objects
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 64 && !s.chars().any(char::is_control)),
            "Invalid presentation object name"
        );
        self.when.validate(supports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_compose_source_facts_without_assigning_gameplay_meaning() -> Result<()> {
        // The same condition mechanism drives unrelated source adapters.
        for (required, alternatives, excluded) in [
            ("occupied", ["brake", "reverse"], "destroyed"),
            ("equipped", ["trigger", "charging"], "empty"),
        ] {
            let condition = Condition {
                all: vec![required.into()],
                any: alternatives.map(String::from).into(),
                none: vec![excluded.into()],
            };
            condition.validate(|s| {
                [required, alternatives[0], alternatives[1], excluded].contains(&s)
            })?;
            for bits in 0..16 {
                let values = [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0];
                let names = [required, alternatives[0], alternatives[1], excluded];
                assert_eq!(
                    condition.evaluate(|s| values[names.iter().position(|n| *n == s).unwrap()]),
                    values[0] && (values[1] || values[2]) && !values[3]
                );
            }
        }
        let unknown: Condition = serde_json::from_str(r#"{"none":["unknown"]}"#)?;
        assert!(unknown.validate(|_| false).is_err());
        Ok(())
    }
}
