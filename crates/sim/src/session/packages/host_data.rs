//! What an Add-On's rules keep on the host between games and restarts:
//! Slayer's saved configs, the last game's settings for Auto Start With
//! Server and its Bonus Kills texts. The host keeps one small JSON map per
//! Add-On ([`AddOnData`]); rules read and write it
//! with `host_data` and `set_host_data`.
use super::*;
use std::sync::Mutex;

pub use bri_package_runtime::ops::{MAX_HOST_KEYS, MAX_HOST_VALUE};

/// Where a host keeps Add-Ons' data.
pub trait AddOnData: Send + Sync {
    /// Everything kept for `package`, by key.
    fn load(&self, package: &str) -> BTreeMap<String, serde_json::Value>;
    /// Keep `data` as everything for `package`.
    fn save(&self, package: &str, data: &BTreeMap<String, serde_json::Value>);
}

/// A store in memory: for tests, and hosts that keep nothing on disk.
#[derive(Default)]
pub struct MemoryAddOnData {
    data: Mutex<BTreeMap<String, BTreeMap<String, serde_json::Value>>>,
}
impl AddOnData for MemoryAddOnData {
    fn load(&self, package: &str) -> BTreeMap<String, serde_json::Value> {
        self.data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(package)
            .cloned()
            .unwrap_or_default()
    }
    fn save(&self, package: &str, data: &BTreeMap<String, serde_json::Value>) {
        self.data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(package.to_owned(), data.clone());
    }
}

pub(in crate::session) use bri_package_runtime::ops::valid_host_key as rules_key;

impl Session {
    /// Where Add-Ons keep data on this host. Set before packages are
    /// enabled; without one, nothing outlives the session.
    pub fn set_addon_data(&mut self, store: Arc<dyn AddOnData>) {
        self.addon_data = Some(store);
    }

    /// Read every enabled Add-On's kept data.
    pub(in crate::session) fn load_host_data(&mut self) {
        let Some(store) = self.addon_data.clone() else {
            return;
        };
        let tape = self.tape.clone();
        let Some(host) = self.packages.as_mut() else {
            return;
        };
        let ids: Vec<String> = host
            .catalog
            .behaviours()
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let mut data =
                crate::replay::outside(tape.as_ref(), || crate::replay::HostData(store.load(&id)))
                    .0;
            data.retain(|k, v| {
                rules_key(k) && serde_json::to_vec(v).is_ok_and(|b| b.len() <= MAX_HOST_VALUE)
            });
            while data.len() > MAX_HOST_KEYS {
                data.pop_last();
            }
            host.host_data.insert(id, data);
        }
    }

    /// What `package` keeps as `key`.
    pub(in crate::session) fn host_data(
        &self,
        package: &str,
        key: &str,
    ) -> Option<&serde_json::Value> {
        self.packages.as_ref()?.host_data.get(package)?.get(key)
    }

    /// Keep `value` as `package`'s `key`, or forget it with `None`.
    pub(in crate::session) fn set_host_data(
        &mut self,
        package: &str,
        key: &str,
        value: Option<serde_json::Value>,
    ) -> Result<()> {
        if let Some(v) = &value {
            ensure!(
                serde_json::to_vec(v)?.len() <= MAX_HOST_VALUE,
                "A kept value is at most {MAX_HOST_VALUE} bytes"
            );
        }
        let host = self.packages.as_mut().context("No Add-Ons are running")?;
        let data = host.host_data.entry(package.to_owned()).or_default();
        match value {
            Some(v) => {
                ensure!(
                    data.contains_key(key) || data.len() < MAX_HOST_KEYS,
                    "An Add-On keeps at most {MAX_HOST_KEYS} values"
                );
                data.insert(key.to_owned(), v);
            }
            None => {
                data.remove(key);
            }
        }
        if let Some(store) = &self.addon_data {
            store.save(package, data);
        }
        Ok(())
    }
}
