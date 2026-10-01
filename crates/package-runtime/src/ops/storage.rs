//! Operations behind the `storage` capability.
use super::*;

/// Keep `value` on the host as the package's `key` between games and
/// restarts, or forget it with `None` (Slayer's configs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetHostData {
    pub key: String,
    pub value: Option<serde_json::Value>,
}
impl ScriptOp for SetHostData {
    const CAPABILITY: &str = "storage";
    const NAME: &str = "set_host_data";
    fn bounded(&self) -> bool {
        let SetHostData { key, value } = self;
        valid_host_key(key)
            && value
                .as_ref()
                .is_none_or(|v| serde_json::to_vec(v).is_ok_and(|b| b.len() <= MAX_HOST_VALUE))
    }
}
