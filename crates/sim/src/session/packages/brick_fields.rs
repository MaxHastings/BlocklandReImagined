//! Values Add-On rules keep on bricks (`set_brick_field`), as a v20 script
//! kept dynamic fields on a brick object: Slayer's `isLocked[color]`, which
//! Capture the Flag reads to refuse a locked flag.
//!
//! Each package writes only its own keys (`package:key`); every package
//! may read any of them, as a v20 script could read any brick field. A
//! brick's values go with it.
use super::*;

/// Values one brick may carry, over every package.
const MAX_FIELDS_PER_BRICK: usize = 32;
/// Values on all bricks together.
const MAX_FIELDS: usize = 16_384;
/// Longest a value may be, as JSON.
const MAX_FIELD_BYTES: usize = 256;

#[derive(Default)]
pub(in crate::session) struct BrickFields {
    by_brick: BTreeMap<u64, BTreeMap<String, serde_json::Value>>,
    count: usize,
}
impl BrickFields {
    pub(in crate::session) fn get(&self, brick: u64, key: &str) -> Option<&serde_json::Value> {
        self.by_brick.get(&brick)?.get(key)
    }
    /// Keep `value` under `key` (already `package:key`), or clear it.
    fn set(&mut self, brick: u64, key: String, value: Option<serde_json::Value>) -> Result<()> {
        let Some(value) = value else {
            if let Some(fields) = self.by_brick.get_mut(&brick)
                && fields.remove(&key).is_some()
            {
                self.count -= 1;
                if fields.is_empty() {
                    self.by_brick.remove(&brick);
                }
            }
            return Ok(());
        };
        state::check_value(&value)?;
        ensure!(
            serde_json::to_vec(&value)?.len() <= MAX_FIELD_BYTES,
            "a brick value is at most {MAX_FIELD_BYTES} bytes"
        );
        let fields = self.by_brick.entry(brick).or_default();
        if !fields.contains_key(&key) {
            ensure!(
                fields.len() < MAX_FIELDS_PER_BRICK,
                "a brick carries at most {MAX_FIELDS_PER_BRICK} values"
            );
            ensure!(
                self.count < MAX_FIELDS,
                "bricks carry at most {MAX_FIELDS} values"
            );
            self.count += 1;
        }
        fields.insert(key, value);
        Ok(())
    }
    /// A brick is gone: so are its values.
    pub(in crate::session) fn forget(&mut self, brick: u64) {
        if let Some(fields) = self.by_brick.remove(&brick) {
            self.count -= fields.len();
        }
    }
}

impl Session {
    /// `set_brick_field`: `package` keeps `value` on `brick` as its `key`.
    pub(in crate::session) fn package_set_brick_field(
        &mut self,
        package: &str,
        brick: u64,
        key: &str,
        value: Option<serde_json::Value>,
    ) -> Result<()> {
        ensure!(
            self.simulation.state().bricks.contains_key(&brick),
            "No brick {brick}"
        );
        let host = self.packages.as_mut().context("No packages are enabled")?;
        host.brick_fields
            .set(brick, format!("{package}:{key}"), value)
    }
    /// `brick_field`: the value kept on `brick` as `key`, the calling
    /// package's own or `namespace:key`.
    pub(in crate::session) fn brick_field(
        &self,
        package: &str,
        brick: u64,
        key: &str,
    ) -> Option<serde_json::Value> {
        let key = if key.contains(':') {
            key.to_owned()
        } else {
            format!("{package}:{key}")
        };
        self.packages
            .as_ref()?
            .brick_fields
            .get(brick, &key)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brick_values_stay_within_their_limits_and_go_with_the_brick() {
        let mut fields = BrickFields::default();
        fields
            .set(1, "a:locked".into(), Some(serde_json::json!({"3": true})))
            .unwrap();
        assert_eq!(
            fields.get(1, "a:locked"),
            Some(&serde_json::json!({"3": true}))
        );
        assert!(
            fields
                .set(1, "a:big".into(), Some(serde_json::json!("x".repeat(300))))
                .is_err(),
            "too long"
        );
        for i in 1..MAX_FIELDS_PER_BRICK {
            fields.set(1, format!("a:{i}"), Some(1.into())).unwrap();
        }
        assert!(fields.set(1, "a:one_more".into(), Some(1.into())).is_err());
        fields.set(1, "a:1".into(), Some(2.into())).unwrap();
        assert_eq!(fields.count, MAX_FIELDS_PER_BRICK, "replacing adds nothing");
        fields.set(1, "a:1".into(), None).unwrap();
        assert_eq!(fields.count, MAX_FIELDS_PER_BRICK - 1);
        fields.forget(1);
        assert_eq!(fields.count, 0);
        assert_eq!(fields.get(1, "a:locked"), None);
    }
}
