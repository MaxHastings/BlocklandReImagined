//! Converter-only inventory schema (`inventory.json`): the complete audit
//! trail behind the runtime manifest. Runtime code never reads this.

use std::collections::BTreeMap;

use bri_audio::schema::{Diagnostic, Evidence};
use serde::Serialize;

pub const INVENTORY_SCHEMA: &str = "bri.audio-inventory";
pub const INVENTORY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub struct Inventory {
    pub schema: &'static str,
    pub schema_version: u32,
    pub pack_id: String,
    pub reference_label: String,
    pub summary: Summary,
    pub archives: Vec<ArchiveRecord>,
    pub scripts: Vec<ScriptRecord>,
    pub audio_files: Vec<AudioFileRecord>,
    pub descriptions: Vec<DescriptionRecord>,
    pub profiles: Vec<ProfileRecord>,
    pub music_rule: MusicRuleRecord,
    pub bindings: Vec<BindingRecord>,
    pub call_sites: Vec<CallSiteRecord>,
    pub engine_bound: Vec<EngineBoundRecord>,
    pub audio_prefs: Vec<PrefRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize, Default)]
pub struct Summary {
    pub audio_files: usize,
    pub wav_files: usize,
    pub ogg_files: usize,
    pub unique_clips: usize,
    pub clips_decoded: usize,
    pub clips_failed: usize,
    pub descriptions: usize,
    pub profiles_static: usize,
    pub profiles_generated_music: usize,
    pub sounds_ready: usize,
    pub sounds_unavailable: usize,
    pub bindings: usize,
    pub call_sites: usize,
    pub engine_bound: usize,
    pub triggers: usize,
    pub unreferenced_audio_files: usize,
    pub diagnostics_by_severity: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
pub struct ArchiveRecord {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub members: usize,
    pub default_enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ScriptRecord {
    /// Evidence label used in every `Evidence.file` pointing into this script.
    pub label: String,
    pub virtual_path: String,
    pub package: String,
    pub layer: String,
    pub side: String,
    pub sha256: String,
    pub bytes: usize,
    pub default_enabled: bool,
    /// For decompiled scripts: the compiled original in the installation.
    pub original_dso: Option<DsoRecord>,
}

#[derive(Debug, Serialize)]
pub struct DsoRecord {
    pub install_path: String,
    pub install_sha256: Option<String>,
    pub research_sha256: Option<String>,
    /// `None` when either compiled file is absent.
    pub identical: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AudioFileRecord {
    pub virtual_path: String,
    pub container: String,
    pub package: String,
    pub sha256: String,
    pub bytes: u64,
    pub clip: String,
    /// Sound ids whose resolved file is this path.
    pub referenced_by: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct DefinitionRecord {
    pub evidence: Evidence,
    pub keyword: String,
    pub layer: String,
    pub package: String,
    pub side: String,
    pub parent: Option<String>,
    pub context: Vec<String>,
    /// Field name as written -> raw value text.
    pub fields: BTreeMap<String, String>,
    pub effective: bool,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DescriptionRecord {
    pub id: String,
    pub name: String,
    pub definitions: Vec<DefinitionRecord>,
    pub resolved_channel_source: String,
}

#[derive(Debug, Serialize)]
pub struct ProfileRecord {
    pub id: String,
    pub name: String,
    pub definitions: Vec<DefinitionRecord>,
    pub filename_raw: Option<String>,
    pub resolved_path: Option<String>,
    pub clip: Option<String>,
    pub description: Option<String>,
    pub lists: Vec<String>,
    pub reachable_by: Vec<String>,
    /// Files named by non-effective definitions (other load orders / guards).
    pub alternate_files: Vec<AlternateFile>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AlternateFile {
    pub evidence: Evidence,
    pub resolved_path: String,
    pub exists: bool,
    pub context: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MusicRuleRecord {
    pub rule: String,
    pub evidence: Vec<Evidence>,
    pub flags_source: Vec<Evidence>,
    pub candidates: Vec<MusicCandidate>,
}

#[derive(Debug, Serialize)]
pub struct MusicCandidate {
    pub file: String,
    pub datablock: String,
    pub ui_name: String,
    pub flag: Option<String>,
    pub bytes: u64,
    pub channels: Option<u16>,
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct BindingRecord {
    pub profile: String,
    pub sound: String,
    pub datablock_class: String,
    pub datablock: String,
    pub field: String,
    /// Extra context, e.g. the image state name for `stateSound[n]`.
    pub detail: Option<String>,
    pub package: String,
    pub layer: String,
    pub evidence: Evidence,
}

#[derive(Debug, Serialize, Clone)]
pub struct CallSiteRecord {
    pub profile: String,
    pub sound: Option<String>,
    /// `alxPlay`, `ServerPlay3D`, `playAudio`, ... or `reference` for other uses.
    pub api: String,
    pub function: Option<String>,
    pub is_string: bool,
    pub package: String,
    pub layer: String,
    pub evidence: Evidence,
}

#[derive(Debug, Serialize, Clone)]
pub struct EngineBoundRecord {
    pub profile: String,
    pub sound: String,
    pub executable_offset: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct PrefRecord {
    pub name: String,
    pub value: String,
    pub evidence: Evidence,
}
