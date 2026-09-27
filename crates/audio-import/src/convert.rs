//! Offline conversion: reference installation + recovered scripts -> native pack.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use bri_audio::decode::{decode_all, peak_rms};
use bri_audio::schema::*;

use crate::model::*;
use crate::tscript::{self, Decl, Token, Value};
use crate::vfs::{Entry, Vfs, resolve_script_path, sha256_hex};

pub struct Options {
    pub v20: PathBuf,
    pub decompiled: PathBuf,
    /// How evidence refers to the decompiled directory, e.g. `.research/v20-dso`.
    pub decompiled_label: String,
    pub pack_id: String,
    pub reference_label: String,
    pub arguments: Vec<String>,
    /// Clips longer than this (seconds) are marked for streaming; music always is.
    pub stream_threshold_seconds: f64,
}

pub struct ClipOut {
    pub file: String,
    pub bytes: Vec<u8>,
}

/// Per-clip validation evidence.
#[derive(Debug, serde::Serialize)]
pub struct ClipValidation {
    pub clip: String,
    pub sha256: String,
    pub sha256_verified_from_source: bool,
    pub format: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: Option<u16>,
    pub frames: u64,
    pub container_frames: Option<u64>,
    pub duration_seconds: f64,
    pub peak: f32,
    pub rms: f32,
    pub packets: u64,
    pub corrupt_packets: u64,
    pub non_finite_samples: u64,
    pub decoded: bool,
    pub error: Option<String>,
}

pub struct Converted {
    pub manifest: PackManifest,
    pub inventory: Inventory,
    pub clips: Vec<ClipOut>,
    pub validation: Vec<ClipValidation>,
    pub coverage_markdown: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    V20Base,
    LauncherPatch,
    AddOn,
}

impl Layer {
    fn as_str(self) -> &'static str {
        match self {
            Layer::V20Base => "v20-base",
            Layer::LauncherPatch => "launcher-patch",
            Layer::AddOn => "add-on",
        }
    }
}

struct Script {
    label: String,
    vpath: String,
    package: String,
    layer: Layer,
    side: &'static str,
    sha256: String,
    bytes: usize,
    default_enabled: bool,
    dso: Option<DsoRecord>,
    tokens: Vec<Token>,
    scan: tscript::ScanResult,
    rank: (u8, bool, String, bool, String),
}

impl Script {
    fn ev(&self, line: u32) -> Evidence {
        Evidence {
            file: self.label.clone(),
            line,
        }
    }
}

/// Audio-playing console functions/methods (lower-case -> canonical spelling).
const AUDIO_APIS: &[(&str, &str)] = &[
    ("alxplay", "alxPlay"),
    ("alxcreatesource", "alxCreateSource"),
    ("serverplay3d", "ServerPlay3D"),
    ("serverplay2d", "ServerPlay2D"),
    ("playaudio", "playAudio"),
    ("stopaudio", "stopAudio"),
    ("play2d", "play2D"),
    ("play3d", "play3D"),
    ("playsound", "playSound"),
];

fn canonical_api(callee: &str) -> Option<&'static str> {
    let l = lower(callee);
    AUDIO_APIS.iter().find(|(k, _)| *k == l).map(|(_, v)| *v)
}

fn side_of(vpath: &str) -> &'static str {
    let l = vpath.to_ascii_lowercase();
    if l.starts_with("base/client/") {
        "client"
    } else if l.starts_with("base/server/") {
        "server"
    } else if l.starts_with("base/") {
        "shared"
    } else {
        let file = l.rsplit('/').next().unwrap_or("");
        if file == "client.cs" || l.contains("/rtbc_") || l.ends_with(".gui") {
            "client"
        } else {
            "server"
        }
    }
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            walk_files(&e.path(), out)?;
        } else {
            out.push(e.path());
        }
    }
    Ok(())
}

fn text_of(bytes: &[u8]) -> String {
    // Latin-1: every byte maps to one char, so line numbers stay exact.
    bytes.iter().map(|&b| b as char).collect()
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Engine defaults for `AudioDescription` (TGE-family `AudioDescription` ctor).
struct DescValues {
    volume: f32,
    is_looping: bool,
    is_streaming: bool,
    is_3d: bool,
    reference_distance: f32,
    max_distance: f32,
    cone_inside_angle: f32,
    cone_outside_angle: f32,
    cone_outside_volume: f32,
    environment_level: f32,
    channel: u8,
}

impl Default for DescValues {
    fn default() -> Self {
        Self {
            volume: 1.0,
            is_looping: false,
            is_streaming: false,
            is_3d: false,
            reference_distance: 1.0,
            max_distance: 100.0,
            cone_inside_angle: 360.0,
            cone_outside_angle: 360.0,
            cone_outside_volume: 1.0,
            environment_level: 0.0,
            channel: 0,
        }
    }
}

struct DefRef {
    script: usize,
    decl: usize,
}

struct Resolved {
    name: String,
    /// lower key -> (name as written, value, evidence)
    fields: BTreeMap<String, (String, Value, Evidence)>,
    effective: usize,
    defs: Vec<DefRef>,
    skipped_guarded: Vec<usize>,
}

pub fn convert(opts: &Options) -> Result<Converted, String> {
    let vfs = Vfs::open(&opts.v20)?;
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for p in &vfs.problems {
        diagnostics.push(diag(Severity::Warning, "vfs-problem", p.clone(), vec![]));
    }

    // ---------------------------------------------------------------- defaults
    let default_addons = read_flag_list(&vfs, "base/server/defaultAddOnList.cs", "$addon__");

    // ---------------------------------------------------------------- scripts
    let mut scripts: Vec<Script> = Vec::new();
    let mut decompiled_vpaths: HashSet<String> = HashSet::new();
    let mut files = Vec::new();
    walk_files(&opts.decompiled, &mut files)
        .map_err(|e| format!("cannot list {}: {e}", opts.decompiled.display()))?;
    files.sort();
    for f in &files {
        let rel = f
            .strip_prefix(&opts.decompiled)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        let l = lower(&rel);
        if !(l.ends_with(".cs") || l.ends_with(".gui")) {
            continue;
        }
        let bytes = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let vpath = format!("base/{rel}");
        let install_dso = vfs.get(&format!("{vpath}.dso"));
        let install_sha = install_dso
            .and_then(|e| vfs.read(e).ok())
            .map(|b| sha256_hex(&b));
        let research_dso = f.with_file_name(format!(
            "{}.dso",
            f.file_name().unwrap_or_default().to_string_lossy()
        ));
        let research_sha = std::fs::read(&research_dso).ok().map(|b| sha256_hex(&b));
        let identical = match (&install_sha, &research_sha) {
            (Some(a), Some(b)) => Some(a == b),
            _ => None,
        };
        if identical == Some(false) {
            diagnostics.push(diag(
                Severity::Error,
                "dso-mismatch",
                format!("decompiled {rel} was produced from a DSO that differs from the reference installation"),
                vec![],
            ));
        }
        decompiled_vpaths.insert(lower(&vpath));
        scripts.push(new_script(
            format!("{}/{}", opts.decompiled_label.trim_end_matches('/'), rel),
            vpath.clone(),
            "base".into(),
            Layer::V20Base,
            true,
            &bytes,
            Some(DsoRecord {
                install_path: format!("{vpath}.dso"),
                install_sha256: install_sha,
                research_sha256: research_sha,
                identical,
            }),
        ));
    }
    for e in vfs.entries() {
        let ext = e.extension();
        if !(ext == "cs" || ext == "gui") {
            continue;
        }
        let lp = lower(&e.path);
        if lp.starts_with("base/") {
            if decompiled_vpaths.contains(&lp) {
                continue;
            }
            let bytes = vfs.read(e)?;
            let layer = if text_of(&bytes).contains("B4v21") {
                Layer::LauncherPatch
            } else {
                Layer::V20Base
            };
            scripts.push(new_script(
                e.path.clone(),
                e.path.clone(),
                "base".into(),
                layer,
                true,
                &bytes,
                None,
            ));
        } else if lp.starts_with("add-ons/") {
            let bytes = vfs.read(e)?;
            let enabled = default_addons
                .get(&lower(&e.package))
                .map(|(v, _)| *v == "1")
                .unwrap_or(false);
            scripts.push(new_script(
                e.evidence_label(),
                e.path.clone(),
                e.package.clone(),
                Layer::AddOn,
                enabled,
                &bytes,
                None,
            ));
        }
    }
    scripts.sort_by(|a, b| a.rank.cmp(&b.rank));

    // ---------------------------------------------------------------- globals
    let mut globals: HashMap<String, (Value, Evidence)> = HashMap::new();
    for s in &scripts {
        for g in &s.scan.globals {
            globals.insert(g.name.clone(), (g.value.clone(), s.ev(g.line)));
        }
    }

    // ---------------------------------------------------------------- audio clips
    let mut by_sha: BTreeMap<String, (Vec<&Entry>, Vec<u8>)> = BTreeMap::new();
    let (mut wav_count, mut ogg_count) = (0usize, 0usize);
    for e in vfs.entries() {
        let ext = e.extension();
        if ext != "wav" && ext != "ogg" {
            continue;
        }
        if ext == "wav" {
            wav_count += 1
        } else {
            ogg_count += 1
        }
        let bytes = vfs.read(e)?;
        let sha = sha256_hex(&bytes);
        by_sha
            .entry(sha)
            .or_insert_with(|| (Vec::new(), bytes))
            .0
            .push(e);
    }
    let mut clips: Vec<ClipEntry> = Vec::new();
    let mut clip_out: Vec<ClipOut> = Vec::new();
    let mut validation: Vec<ClipValidation> = Vec::new();
    let mut path_to_clip: HashMap<String, String> = HashMap::new();
    let mut clip_by_id: HashMap<String, usize> = HashMap::new();
    for (sha, (entries, bytes)) in &by_sha {
        let mut sources: Vec<&Entry> = entries.clone();
        sources.sort_by_key(|e| (e.package != "base", lower(&e.path)));
        let canonical = sources[0];
        let id = format!("v20/clip/{}", lower(&canonical.path));
        let ext = canonical.extension();
        let format = if ext == "ogg" {
            ClipFormat::OggVorbis
        } else {
            ClipFormat::Wav
        };
        let file = format!("clips/{sha}.{ext}");
        let recheck = sha256_hex(bytes) == *sha;
        let (decoded, err) = match decode_all(bytes, Some(&ext)) {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        let mut v = ClipValidation {
            clip: id.clone(),
            sha256: sha.clone(),
            sha256_verified_from_source: recheck,
            format: format!("{format:?}"),
            sample_rate: 0,
            channels: 0,
            bits_per_sample: None,
            frames: 0,
            container_frames: None,
            duration_seconds: 0.0,
            peak: 0.0,
            rms: 0.0,
            packets: 0,
            corrupt_packets: 0,
            non_finite_samples: 0,
            decoded: decoded.is_some(),
            error: err.clone(),
        };
        if let Some((pcm, rep)) = &decoded {
            let (peak, rms) = peak_rms(&pcm.samples);
            v.sample_rate = pcm.sample_rate;
            v.channels = pcm.channels;
            v.bits_per_sample = rep.bits_per_sample;
            v.frames = pcm.frames() as u64;
            v.container_frames = rep.container_frames;
            v.duration_seconds = pcm.duration_seconds();
            v.peak = peak;
            v.rms = rms;
            v.packets = rep.packets;
            v.corrupt_packets = rep.corrupt_packets;
            v.non_finite_samples = rep.non_finite_samples;
            if rep.corrupt_packets > 0 || rep.non_finite_samples > 0 {
                diagnostics.push(diag(
                    Severity::Warning,
                    "clip-decode-irregular",
                    format!(
                        "{id}: {} corrupt packets, {} non-finite samples",
                        rep.corrupt_packets, rep.non_finite_samples
                    ),
                    vec![],
                ));
            }
        } else {
            diagnostics.push(diag(
                Severity::Error,
                "clip-undecodable",
                format!("{id}: {}", err.unwrap_or_default()),
                vec![],
            ));
        }
        for e in &sources {
            path_to_clip.insert(lower(&e.path), id.clone());
        }
        let is_music_file = sources
            .iter()
            .any(|e| e.package.eq_ignore_ascii_case("Music"));
        let stream = decoded.is_some()
            && (is_music_file || v.duration_seconds > opts.stream_threshold_seconds);
        if decoded.is_some() {
            clip_by_id.insert(id.clone(), clips.len());
            clips.push(ClipEntry {
                id: id.clone(),
                file: file.clone(),
                sha256: sha.clone(),
                bytes: bytes.len() as u64,
                format,
                channels: v.channels,
                sample_rate: v.sample_rate,
                bits_per_sample: v.bits_per_sample,
                frames: v.frames,
                duration_seconds: v.duration_seconds,
                peak: v.peak,
                rms: v.rms,
                stream,
                sources: sources
                    .iter()
                    .map(|e| ClipSource {
                        virtual_path: e.path.clone(),
                        container: e.container_label(),
                        archive_sha256: match &e.container {
                            crate::vfs::Container::Zip { archive, .. } => vfs
                                .archives
                                .iter()
                                .find(|a| &a.path == archive)
                                .map(|a| a.sha256.clone()),
                            _ => None,
                        },
                        package: e.package.clone(),
                    })
                    .collect(),
            });
            clip_out.push(ClipOut {
                file,
                bytes: bytes.clone(),
            });
        }
        validation.push(v);
    }

    // ---------------------------------------------------------------- descriptions
    let desc_res = resolve_objects(&scripts, "audiodescription", &mut diagnostics);
    let mut descriptions: Vec<DescriptionEntry> = Vec::new();
    let mut desc_values: HashMap<String, (String, DescValues)> = HashMap::new();
    let mut desc_records = Vec::new();
    for r in desc_res.values() {
        let mut d = DescValues::default();
        let mut authored = Vec::new();
        let mut channel_src = "engine default 0".to_string();
        for (key, (name, value, ev)) in &r.fields {
            authored.push(name.clone());
            let num = || value.as_f64();
            match key.as_str() {
                "volume" => d.volume = num().unwrap_or(1.0) as f32,
                "islooping" => d.is_looping = num().unwrap_or(0.0) != 0.0,
                "isstreaming" => d.is_streaming = num().unwrap_or(0.0) != 0.0,
                "is3d" => d.is_3d = num().unwrap_or(0.0) != 0.0,
                "referencedistance" => d.reference_distance = num().unwrap_or(1.0) as f32,
                "maxdistance" => d.max_distance = num().unwrap_or(100.0) as f32,
                "coneinsideangle" => d.cone_inside_angle = num().unwrap_or(360.0) as f32,
                "coneoutsideangle" => d.cone_outside_angle = num().unwrap_or(360.0) as f32,
                "coneoutsidevolume" => d.cone_outside_volume = num().unwrap_or(1.0) as f32,
                "environmentlevel" => d.environment_level = num().unwrap_or(0.0) as f32,
                "type" => {
                    let (v, src) = match value {
                        Value::Var(g) => match globals.get(&lower(g)) {
                            Some((gv, gev)) => (
                                gv.as_f64(),
                                format!("{g} = {} ({}:{})", gv.raw(), gev.file, gev.line),
                            ),
                            None => {
                                diagnostics.push(diag(
                                    Severity::Warning,
                                    "undefined-global",
                                    format!(
                                        "{} type uses undefined {g}; engine would read 0",
                                        r.name
                                    ),
                                    vec![ev.clone()],
                                ));
                                (Some(0.0), format!("{g} undefined -> 0"))
                            }
                        },
                        other => (other.as_f64(), format!("literal {}", other.raw())),
                    };
                    d.channel = v.unwrap_or(0.0).clamp(0.0, 8.0) as u8;
                    channel_src = src;
                }
                _ => {}
            }
        }
        let def = &scripts[r.defs[r.effective].script].scan.decls[r.defs[r.effective].decl];
        let id = format!("v20/audio-description/{}", lower(&r.name));
        descriptions.push(DescriptionEntry {
            id: id.clone(),
            name: r.name.clone(),
            volume: d.volume,
            is_looping: d.is_looping,
            is_streaming: d.is_streaming,
            is_3d: d.is_3d,
            reference_distance: d.reference_distance,
            max_distance: d.max_distance,
            channel: d.channel,
            cone_inside_angle: d.cone_inside_angle,
            cone_outside_angle: d.cone_outside_angle,
            cone_outside_volume: d.cone_outside_volume,
            environment_level: d.environment_level,
            authored_fields: authored,
            defined_at: scripts[r.defs[r.effective].script].ev(def.line),
        });
        desc_records.push(DescriptionRecord {
            id: id.clone(),
            name: r.name.clone(),
            definitions: def_records(&scripts, r),
            resolved_channel_source: channel_src,
        });
        desc_values.insert(lower(&r.name), (id, d));
    }
    descriptions.sort_by(|a, b| a.id.cmp(&b.id));
    desc_records.sort_by(|a, b| a.id.cmp(&b.id));

    // ---------------------------------------------------------------- profiles
    let prof_res = resolve_objects(&scripts, "audioprofile", &mut diagnostics);
    let mut sounds: Vec<SoundEntry> = Vec::new();
    let mut profile_records: Vec<ProfileRecord> = Vec::new();
    let mut profile_ids: HashMap<String, String> = HashMap::new(); // lower name -> id
    let all_audio: Vec<&Entry> = vfs
        .entries()
        .filter(|e| {
            let x = e.extension();
            x == "wav" || x == "ogg"
        })
        .collect();
    for r in prof_res.values() {
        let eff = &r.defs[r.effective];
        let script = &scripts[eff.script];
        let decl = &script.scan.decls[eff.decl];
        let id = format!("v20/sound/{}", lower(&r.name));
        profile_ids.insert(lower(&r.name), id.clone());
        let get = |k: &str| r.fields.get(k);
        let filename_raw = get("filename").and_then(|(_, v, _)| string_value(v, &globals));
        // `./` resolves relative to the script declaring the filename field.
        let (resolved_path, clip) = match get("filename") {
            Some((_, v, ev)) => match string_value(v, &globals) {
                Some(raw) => {
                    let base_script = scripts
                        .iter()
                        .find(|s| s.label == ev.file)
                        .map(|s| s.vpath.as_str())
                        .unwrap_or(&script.vpath);
                    let p = resolve_script_path(base_script, &raw);
                    let c = path_to_clip.get(&lower(&p)).cloned();
                    (Some(p), c)
                }
                None => (None, None),
            },
            None => (None, None),
        };
        let desc_name = get("description").and_then(|(_, v, _)| v.as_name().map(str::to_string));
        let desc = desc_name
            .as_deref()
            .and_then(|n| desc_values.get(&lower(n)));
        let ui_name = get("uiname")
            .and_then(|(_, v, _)| v.as_name().map(str::to_string))
            .filter(|s| !s.is_empty());
        let preload = get("preload")
            .and_then(|(_, v, _)| v.as_f64())
            .unwrap_or(0.0)
            != 0.0;
        let status = if let Some(c) = clip.as_ref().filter(|c| !clip_by_id.contains_key(*c)) {
            SoundStatus::Undecodable {
                file: resolved_path.clone().unwrap_or_else(|| c.clone()),
            }
        } else if resolved_path.is_some() && clip.is_none() {
            let requested = filename_raw.clone().unwrap_or_default();
            let cands = candidates(&all_audio, resolved_path.as_deref().unwrap_or(""));
            SoundStatus::MissingClip {
                requested,
                candidates: cands,
            }
        } else if clip.is_none() {
            SoundStatus::MissingClip {
                requested: filename_raw.clone().unwrap_or_default(),
                candidates: vec![],
            }
        } else if desc.is_none() {
            SoundStatus::MissingDescription {
                requested: desc_name.clone().unwrap_or_default(),
            }
        } else {
            SoundStatus::Ready
        };
        let is_music = lower(&r.name).starts_with("musicdata_")
            || desc_name
                .as_deref()
                .is_some_and(|d| lower(d).contains("music"));
        let playback = playback_of(desc.map(|(_, d)| d), is_music);
        let lists = lists_of(ui_name.is_some(), desc.map(|(_, d)| d));
        let package = script.package.clone();
        let layer = script.layer;
        let default_enabled = script.default_enabled;
        sounds.push(SoundEntry {
            id: id.clone(),
            name: r.name.clone(),
            clip: clip.clone().filter(|c| clip_by_id.contains_key(c)),
            description: desc.map(|(id, _)| id.clone()),
            playback,
            preload,
            ui_name: ui_name.clone(),
            family: family_of(&r.name, &package, is_music).into(),
            package: package.clone(),
            default_enabled,
            layer: layer.as_str().into(),
            lists: lists.clone(),
            status: status.clone(),
            defined_at: script.ev(decl.line),
        });
        if let SoundStatus::MissingClip {
            requested,
            candidates,
        } = &status
        {
            diagnostics.push(diag(
                if default_enabled { Severity::Error } else { Severity::Warning },
                "missing-clip",
                format!(
                    "{} ({package}) requests {requested:?} which does not exist in the reference installation{}",
                    r.name,
                    if candidates.is_empty() { String::new() } else { format!("; similarly named: {}", candidates.join(", ")) }
                ),
                vec![script.ev(decl.line)],
            ));
        }
        if let (Some(raw), Some(p)) = (&filename_raw, &resolved_path)
            && let Some(e) = vfs.get(p)
            && !e.path.ends_with(raw.rsplit('/').next().unwrap_or(raw))
        {
            diagnostics.push(diag(
                Severity::Info,
                "filename-case",
                format!("{} names {raw:?}; the file on disk is {:?} (resolved case-insensitively as on Windows)", r.name, e.path),
                vec![script.ev(decl.line)],
            ));
        }
        let mut alternate_files = Vec::new();
        for (i, dr) in r.defs.iter().enumerate() {
            if i == r.effective {
                continue;
            }
            let ds = &scripts[dr.script];
            let dd = &ds.scan.decls[dr.decl];
            let Some(f) = dd.fields.iter().rev().find(|f| f.key == "filename") else {
                continue;
            };
            let Some(raw) = string_value(&f.value, &globals) else {
                continue;
            };
            let p = resolve_script_path(&ds.vpath, &raw);
            let exists = path_to_clip.contains_key(&lower(&p));
            if !exists {
                diagnostics.push(diag(
                    Severity::Warning,
                    "missing-clip-alternate",
                    format!(
                        "{} alternative definition ({}) names {raw:?}, which does not exist; it only takes effect under a different load order{}",
                        r.name,
                        ds.package,
                        if dd.context.is_empty() { String::new() } else { format!(" / when {}", dd.context.join(", ")) }
                    ),
                    vec![ds.ev(dd.line)],
                ));
            }
            alternate_files.push(AlternateFile {
                evidence: ds.ev(dd.line),
                resolved_path: p,
                exists,
                context: dd.context.clone(),
            });
        }
        profile_records.push(ProfileRecord {
            alternate_files,
            id,
            name: r.name.clone(),
            definitions: def_records(&scripts, r),
            filename_raw,
            resolved_path,
            clip,
            description: desc.map(|(id, _)| id.clone()),
            lists,
            reachable_by: vec![],
        });
    }

    // ---------------------------------------------------------------- music rule
    let (music_rule, mut music_sounds) = music_rule(
        &vfs,
        &scripts,
        &clips,
        &clip_by_id,
        &path_to_clip,
        &desc_values,
        &mut diagnostics,
    );
    for s in &music_sounds {
        profile_ids.insert(lower(&s.name), s.id.clone());
    }
    sounds.append(&mut music_sounds);
    sounds.sort_by(|a, b| a.id.cmp(&b.id));
    profile_records.sort_by(|a, b| a.id.cmp(&b.id));

    // ---------------------------------------------------------------- references
    let mut names: HashSet<String> = profile_ids.keys().cloned().collect();
    names.extend(desc_values.keys().cloned());
    let mut bindings: Vec<BindingRecord> = Vec::new();
    let mut call_sites: Vec<CallSiteRecord> = Vec::new();
    for s in &scripts {
        for d in &s.scan.decls {
            let class = lower(&d.class);
            if class == "audioprofile" || class == "audiodescription" {
                continue;
            }
            for f in &d.fields {
                let Some(v) = f.value.as_name() else { continue };
                if let Some(sid) = profile_ids.get(&lower(v)) {
                    let detail = f.key.strip_prefix("statesound").and_then(|idx| {
                        d.field(&format!("statename{idx}"))
                            .and_then(|g| g.value.as_name().map(|n| format!("state {idx} \"{n}\"")))
                    });
                    bindings.push(BindingRecord {
                        detail,
                        profile: canonical_name(&sounds, sid),
                        sound: sid.clone(),
                        datablock_class: d.class.clone(),
                        datablock: d.name.clone().unwrap_or_else(|| "<unnamed>".into()),
                        field: f.name.clone(),
                        package: s.package.clone(),
                        layer: s.layer.as_str().into(),
                        evidence: s.ev(f.line),
                    });
                } else if (looks_like_sound_field(&f.key)
                    || (class == "audioemitter" && f.key == "profile"))
                    && !v.is_empty()
                    && v != "0"
                    && !desc_values.contains_key(&lower(v))
                {
                    diagnostics.push(diag(
                        Severity::Warning,
                        "unknown-profile-binding",
                        format!(
                            "{} {}.{} = {v}: no AudioProfile of that name exists",
                            d.class,
                            d.name.as_deref().unwrap_or("?"),
                            f.name
                        ),
                        vec![s.ev(f.line)],
                    ));
                }
            }
        }
        let occ = tscript::scan(&s.tokens, Some(&names));
        for o in occ.occurrences {
            let lname = lower(&o.text);
            let Some(sid) = profile_ids.get(&lname) else {
                continue;
            }; // description refs are recorded via profiles
            let api = o
                .callee
                .as_deref()
                .and_then(canonical_api)
                .unwrap_or("reference")
                .to_string();
            call_sites.push(CallSiteRecord {
                profile: canonical_name(&sounds, sid),
                sound: Some(sid.clone()),
                api,
                function: o.function,
                is_string: o.is_string,
                package: s.package.clone(),
                layer: s.layer.as_str().into(),
                evidence: s.ev(o.line),
            });
        }
        unknown_api_refs(s, &profile_ids, &desc_values, &mut diagnostics);
    }

    // ---------------------------------------------------------------- engine-bound
    let mut engine_bound = Vec::new();
    let mut exe_sha = None;
    if let Some(exe) = vfs
        .get("blocklandv20.exe")
        .or_else(|| vfs.get("Blockland.exe"))
    {
        let bytes = vfs.read(exe)?;
        exe_sha = Some(sha256_hex(&bytes));
        // Only client-local profiles (`new AudioProfile` in client scripts) can be
        // looked up by name from engine code; server datablock names that match
        // engine field names (e.g. `JumpSound`) are not evidence of binding.
        let client_side: HashSet<String> = prof_res
            .values()
            .filter(|r| scripts[r.defs[r.effective].script].side == "client")
            .map(|r| lower(&r.name))
            .collect();
        let script_played: HashSet<&str> = call_sites
            .iter()
            .filter(|c| c.api != "reference")
            .map(|c| c.profile.as_str())
            .collect();
        for s in &sounds {
            if !client_side.contains(&lower(&s.name)) || script_played.contains(s.name.as_str()) {
                continue;
            }
            let mut pat = vec![0u8];
            pat.extend_from_slice(s.name.as_bytes());
            pat.push(0);
            if let Some(off) = bytes.windows(pat.len()).position(|w| w == pat.as_slice()) {
                engine_bound.push(EngineBoundRecord {
                    profile: s.name.clone(),
                    sound: s.id.clone(),
                    executable_offset: off + 1,
                });
            }
        }
    }

    // ---------------------------------------------------------------- reachability
    for rec in profile_records.iter_mut() {
        let mut by = BTreeSet::new();
        for b in bindings.iter().filter(|b| b.sound == rec.id) {
            by.insert(format!("binding:{}.{}", b.datablock, b.field));
        }
        for c in call_sites
            .iter()
            .filter(|c| c.sound.as_deref() == Some(&rec.id) && c.api != "reference")
        {
            by.insert(format!("call:{}", c.api));
        }
        for e in engine_bound.iter().filter(|e| e.sound == rec.id) {
            by.insert(format!("engine:{}", e.profile));
        }
        for l in &rec.lists {
            by.insert(format!("list:{l}"));
        }
        rec.reachable_by = by.into_iter().collect();
        if rec.reachable_by.is_empty() {
            let s = sounds.iter().find(|s| s.id == rec.id);
            diagnostics.push(diag(
                Severity::Info,
                "unbound-profile",
                format!("{} is defined but never bound, played by script, engine-bound or listed; kept in the pack for completeness", rec.name),
                s.map(|s| vec![s.defined_at.clone()]).unwrap_or_default(),
            ));
        }
    }

    // ---------------------------------------------------------------- unreferenced files
    let mut referenced_paths: HashMap<String, Vec<String>> = HashMap::new();
    for r in &profile_records {
        if let Some(p) = &r.resolved_path {
            referenced_paths
                .entry(lower(p))
                .or_default()
                .push(r.id.clone());
        }
        for a in r.alternate_files.iter().filter(|a| a.exists) {
            referenced_paths
                .entry(lower(&a.resolved_path))
                .or_default()
                .push(format!("{} (alternate definition)", r.id));
        }
    }
    for s in sounds.iter().filter(|s| s.layer == "rule") {
        if let Some(c) = s.clip.as_ref().and_then(|c| clip_by_id.get(c)) {
            for src in &clips[*c].sources {
                referenced_paths
                    .entry(lower(&src.virtual_path))
                    .or_default()
                    .push(s.id.clone());
            }
        }
    }
    let mut audio_files = Vec::new();
    let mut unreferenced = 0;
    for e in &all_audio {
        let refs = referenced_paths
            .get(&lower(&e.path))
            .cloned()
            .unwrap_or_default();
        let clip = path_to_clip
            .get(&lower(&e.path))
            .cloned()
            .unwrap_or_default();
        if refs.is_empty() {
            unreferenced += 1;
            let shared = clips
                .iter()
                .find(|c| c.id == clip)
                .map(|c| {
                    c.sources
                        .iter()
                        .any(|s| referenced_paths.contains_key(&lower(&s.virtual_path)))
                })
                .unwrap_or(false);
            diagnostics.push(diag(
                Severity::Info,
                "unreferenced-audio-file",
                format!(
                    "{} ({}) is not named by any profile{}; its bytes are preserved in the pack as {clip}",
                    e.path,
                    e.package,
                    if shared { " (identical bytes are used under another path)" } else { "" }
                ),
                vec![],
            ));
        }
        audio_files.push(AudioFileRecord {
            virtual_path: e.path.clone(),
            container: e.container_label(),
            package: e.package.clone(),
            sha256: clips
                .iter()
                .find(|c| c.id == clip)
                .map(|c| c.sha256.clone())
                .unwrap_or_default(),
            bytes: e.size,
            clip,
            referenced_by: refs,
        });
    }

    // ---------------------------------------------------------------- prefs/defaults
    let (defaults, audio_prefs) = read_defaults(&vfs)?;

    // ---------------------------------------------------------------- triggers
    let triggers = build_triggers(
        &sounds,
        &bindings,
        &call_sites,
        &engine_bound,
        &mut diagnostics,
    );

    // ---------------------------------------------------------------- manifest
    let channels = vec![
        ChannelInfo {
            channel: 0,
            name: "AudioChannel0 (options test tone only)".into(),
            bus: Bus::Other,
            ui_control: None,
        },
        ChannelInfo {
            channel: 1,
            name: "$GuiAudioType".into(),
            bus: Bus::Interface,
            ui_control: Some("OptAudioVolumeShell ($pref::Audio::channelVolume1)".into()),
        },
        ChannelInfo {
            channel: 2,
            name: "$SimAudioType".into(),
            bus: Bus::Effects,
            ui_control: Some("OptAudioVolumeSim ($pref::Audio::channelVolume2)".into()),
        },
        ChannelInfo {
            channel: 3,
            name: "$MessageAudioType".into(),
            bus: Bus::Message,
            ui_control: None,
        },
    ];
    diagnostics
        .sort_by(|a, b| (b.severity, &a.code, &a.message).cmp(&(a.severity, &b.code, &b.message)));
    let manifest = PackManifest {
        schema: PACK_SCHEMA.into(),
        schema_version: PACK_SCHEMA_VERSION,
        pack_id: opts.pack_id.clone(),
        generator: Generator {
            name: env!("CARGO_PKG_NAME").into(),
            version: env!("CARGO_PKG_VERSION").into(),
            arguments: opts.arguments.clone(),
        },
        source: SourceSummary {
            reference_label: opts.reference_label.clone(),
            executable_sha256: exe_sha,
            audio_files_found: wav_count + ogg_count,
            wav_files_found: wav_count,
            ogg_files_found: ogg_count,
            scripts_scanned: scripts.len(),
        },
        defaults,
        channels,
        clips,
        descriptions,
        sounds,
        triggers,
        diagnostics: diagnostics.clone(),
    };
    manifest.validate()?;

    let mut by_sev = BTreeMap::new();
    for d in &diagnostics {
        *by_sev
            .entry(format!("{:?}", d.severity).to_ascii_lowercase())
            .or_insert(0usize) += 1;
    }
    let summary = Summary {
        audio_files: wav_count + ogg_count,
        wav_files: wav_count,
        ogg_files: ogg_count,
        unique_clips: by_sha.len(),
        clips_decoded: validation.iter().filter(|v| v.decoded).count(),
        clips_failed: validation.iter().filter(|v| !v.decoded).count(),
        descriptions: manifest.descriptions.len(),
        profiles_static: profile_records.len(),
        profiles_generated_music: manifest.sounds.iter().filter(|s| s.layer == "rule").count(),
        sounds_ready: manifest.sounds.iter().filter(|s| s.is_ready()).count(),
        sounds_unavailable: manifest.sounds.iter().filter(|s| !s.is_ready()).count(),
        bindings: bindings.len(),
        call_sites: call_sites.iter().filter(|c| c.api != "reference").count(),
        engine_bound: engine_bound.len(),
        triggers: manifest.triggers.len(),
        unreferenced_audio_files: unreferenced,
        diagnostics_by_severity: by_sev,
    };
    let inventory = Inventory {
        schema: INVENTORY_SCHEMA,
        schema_version: INVENTORY_SCHEMA_VERSION,
        pack_id: opts.pack_id.clone(),
        reference_label: opts.reference_label.clone(),
        summary,
        archives: vfs
            .archives
            .iter()
            .map(|a| {
                let stem = a
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".zip");
                ArchiveRecord {
                    path: a.path.clone(),
                    sha256: a.sha256.clone(),
                    bytes: a.bytes,
                    members: a.members,
                    default_enabled: default_addons.get(&lower(stem)).map(|(v, _)| v == "1"),
                }
            })
            .collect(),
        scripts: scripts
            .iter()
            .map(|s| ScriptRecord {
                label: s.label.clone(),
                virtual_path: s.vpath.clone(),
                package: s.package.clone(),
                layer: s.layer.as_str().into(),
                side: s.side.into(),
                sha256: s.sha256.clone(),
                bytes: s.bytes,
                default_enabled: s.default_enabled,
                original_dso: s.dso.as_ref().map(|d| DsoRecord {
                    install_path: d.install_path.clone(),
                    install_sha256: d.install_sha256.clone(),
                    research_sha256: d.research_sha256.clone(),
                    identical: d.identical,
                }),
            })
            .collect(),
        audio_files,
        descriptions: desc_records,
        profiles: profile_records,
        music_rule,
        bindings,
        call_sites,
        engine_bound,
        audio_prefs,
        diagnostics,
    };
    let coverage_markdown = crate::report::coverage_markdown(&manifest, &inventory);
    Ok(Converted {
        manifest,
        inventory,
        clips: clip_out,
        validation,
        coverage_markdown,
    })
}

/// Evaluate a literal string value, expanding `$global @ "text"` concatenations.
fn string_value(v: &Value, globals: &HashMap<String, (Value, Evidence)>) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Num(n) => Some(format!("{n}")),
        Value::Var(g) => globals.get(&lower(g)).and_then(|(gv, _)| match gv {
            Value::Var(_) | Value::Concat(_) => None, // no chains
            other => string_value(other, globals),
        }),
        Value::Concat(parts) => parts
            .iter()
            .map(|p| string_value(p, globals))
            .collect::<Option<Vec<_>>>()
            .map(|v| v.concat()),
        _ => None,
    }
}

fn new_script(
    label: String,
    vpath: String,
    package: String,
    layer: Layer,
    default_enabled: bool,
    bytes: &[u8],
    dso: Option<DsoRecord>,
) -> Script {
    let text = text_of(bytes);
    let tokens = tscript::lex(&text);
    let scan = tscript::scan(&tokens, None);
    let lp = lower(&vpath);
    let file = lp.rsplit('/').next().unwrap_or("").to_string();
    let tier = match layer {
        Layer::V20Base => 0,
        Layer::LauncherPatch => 1,
        Layer::AddOn => 2,
    };
    // Assumed load order: base, launcher patch, then add-ons with RTB first and
    // the rest alphabetically (findFirstFile order is filesystem-dependent);
    // within an add-on, server.cs before the files it execs.
    let rank = (
        tier,
        !package.eq_ignore_ascii_case("System_ReturnToBlockland"),
        lower(&package),
        file != "server.cs",
        lp,
    );
    Script {
        side: side_of(&vpath),
        sha256: sha256_hex(bytes),
        bytes: bytes.len(),
        label,
        vpath,
        package,
        layer,
        default_enabled,
        dso,
        tokens,
        scan,
        rank,
    }
}

fn diag(severity: Severity, code: &str, message: String, evidence: Vec<Evidence>) -> Diagnostic {
    Diagnostic {
        severity,
        code: code.into(),
        message,
        evidence,
    }
}

fn read_flag_list(vfs: &Vfs, path: &str, prefix: &str) -> HashMap<String, (String, Evidence)> {
    let mut out = HashMap::new();
    if let Some(e) = vfs.get(path)
        && let Ok(bytes) = vfs.read(e)
    {
        let toks = tscript::lex(&text_of(&bytes));
        for g in tscript::scan(&toks, None).globals {
            if let Some(rest) = g.name.strip_prefix(prefix) {
                let v = match &g.value {
                    Value::Num(n) => format!("{n}"),
                    other => other.as_name().unwrap_or("").to_string(),
                };
                out.insert(
                    rest.to_string(),
                    (
                        v,
                        Evidence {
                            file: e.path.clone(),
                            line: g.line,
                        },
                    ),
                );
            }
        }
    }
    out
}

/// Group declarations of one class by name and resolve inheritance/effective
/// definition in the assumed load order.
fn resolve_objects(
    scripts: &[Script],
    class: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, Resolved> {
    let mut groups: BTreeMap<String, Vec<DefRef>> = BTreeMap::new();
    for (si, s) in scripts.iter().enumerate() {
        for (di, d) in s.scan.decls.iter().enumerate() {
            if lower(&d.class) != class {
                continue;
            }
            let Some(name) = d
                .name
                .as_ref()
                .filter(|n| n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            else {
                continue; // dynamic names (e.g. eval'd music) are modelled explicitly
            };
            groups.entry(lower(name)).or_default().push(DefRef {
                script: si,
                decl: di,
            });
        }
    }
    let mut out: BTreeMap<String, Resolved> = BTreeMap::new();
    let keys: Vec<String> = groups.keys().cloned().collect();
    // Resolve parents first (bounded depth).
    fn resolve_one(
        key: &str,
        groups: &BTreeMap<String, Vec<DefRef>>,
        scripts: &[Script],
        out: &mut BTreeMap<String, Resolved>,
        diagnostics: &mut Vec<Diagnostic>,
        depth: usize,
    ) {
        if out.contains_key(key) || depth > 8 {
            return;
        }
        let Some(defs) = groups.get(key) else { return };
        let decl = |r: &DefRef| -> &Decl { &scripts[r.script].scan.decls[r.decl] };
        // Effective: last in load order, except `if(!isObject(Name))`-guarded
        // definitions when an earlier definition exists.
        let mut effective = 0;
        let mut skipped = Vec::new();
        for (i, r) in defs.iter().enumerate() {
            let d = decl(r);
            let guarded = d.context.iter().any(|c| {
                let c = lower(c);
                c.contains("isobject") && c.contains(key) && c.contains('!')
            });
            if guarded && i > 0 {
                skipped.push(i);
                continue;
            }
            effective = i;
        }
        let eff = decl(&defs[effective]);
        let mut fields: BTreeMap<String, (String, Value, Evidence)> = BTreeMap::new();
        if let Some(parent) = &eff.parent {
            let pk = lower(parent);
            resolve_one(&pk, groups, scripts, out, diagnostics, depth + 1);
            match out.get(&pk) {
                Some(p) => fields.extend(p.fields.clone()),
                None => diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: "unknown-parent".into(),
                    message: format!(
                        "{} inherits from undefined {parent}",
                        eff.name.clone().unwrap_or_default()
                    ),
                    evidence: vec![scripts[defs[effective].script].ev(eff.line)],
                }),
            }
        }
        for f in &eff.fields {
            fields.insert(
                f.key.clone(),
                (
                    f.name.clone(),
                    f.value.clone(),
                    scripts[defs[effective].script].ev(f.line),
                ),
            );
        }
        if defs.len() > 1 {
            let evs = defs
                .iter()
                .map(|r| scripts[r.script].ev(decl(r).line))
                .collect();
            diagnostics.push(Diagnostic {
                severity: Severity::Info,
                code: "duplicate-definition".into(),
                message: format!(
                    "{} is declared {} times; using the one at {}:{} (assumed load order{})",
                    eff.name.clone().unwrap_or_default(),
                    defs.len(),
                    scripts[defs[effective].script].label,
                    eff.line,
                    if skipped.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; {} isObject-guarded declaration(s) skipped",
                            skipped.len()
                        )
                    }
                ),
                evidence: evs,
            });
        }
        let name = eff.name.clone().unwrap_or_default();
        out.insert(
            key.to_string(),
            Resolved {
                name,
                fields,
                effective,
                defs: defs
                    .iter()
                    .map(|r| DefRef {
                        script: r.script,
                        decl: r.decl,
                    })
                    .collect(),
                skipped_guarded: skipped,
            },
        );
    }
    for k in keys {
        resolve_one(&k, &groups, scripts, &mut out, diagnostics, 0);
    }
    out
}

fn def_records(scripts: &[Script], r: &Resolved) -> Vec<DefinitionRecord> {
    r.defs
        .iter()
        .enumerate()
        .map(|(i, dr)| {
            let s = &scripts[dr.script];
            let d = &s.scan.decls[dr.decl];
            DefinitionRecord {
                evidence: s.ev(d.line),
                keyword: d.keyword.clone(),
                layer: s.layer.as_str().into(),
                package: s.package.clone(),
                side: s.side.into(),
                parent: d.parent.clone(),
                context: d.context.clone(),
                fields: d
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.value.raw()))
                    .collect(),
                effective: i == r.effective,
                note: r
                    .skipped_guarded
                    .contains(&i)
                    .then(|| "skipped: isObject guard and an earlier definition exists".into()),
            }
        })
        .collect()
}

fn playback_of(d: Option<&DescValues>, is_music: bool) -> Playback {
    let d_default = DescValues::default();
    let d = d.unwrap_or(&d_default);
    let bus = if is_music {
        Bus::Music
    } else {
        match d.channel {
            1 => Bus::Interface,
            2 => Bus::Effects,
            3 => Bus::Message,
            _ => Bus::Other,
        }
    };
    Playback {
        gain: d.volume.max(0.0),
        pitch: 1.0,
        looping: d.is_looping,
        spatial: d.is_3d.then_some(Spatial {
            reference_distance: d.reference_distance,
            max_distance: d.max_distance,
        }),
        channel: d.channel,
        bus,
    }
}

/// Stock menus/lists that include a profile (client/server evidence in docs).
fn lists_of(has_ui_name: bool, d: Option<&DescValues>) -> Vec<String> {
    let mut v = Vec::new();
    let Some(d) = d else { return v };
    if has_ui_name {
        v.push("event-param:Music".into()); // fxDTSBrick.setMusic parameter menu
        if d.is_looping {
            v.push("wrench:Sound".into()); // sound-brick wrench list / $uiNameTable_Music
        } else {
            v.push("uiNameTable:Sounds".into());
        }
    } else if !d.is_looping && d.is_3d {
        v.push("event-param:Sound".into()); // fxDTSBrick.playSound / GameConnection.playSound
    }
    v
}

fn family_of(name: &str, package: &str, is_music: bool) -> &'static str {
    if is_music {
        return "music";
    }
    let p = lower(package);
    for (prefix, fam) in [
        ("weapon_", "weapons"),
        ("projectile_", "projectiles"),
        ("vehicle_", "vehicles"),
        ("sound_", "event-sounds"),
        ("emote_", "emotes"),
        ("item_", "items"),
        ("system_", "legacy-service"),
        ("map_", "maps"),
    ] {
        if p.starts_with(prefix) {
            return fam;
        }
    }
    let n = lower(name);
    let n = n.as_str();
    const UI: &[&str] = &[
        "audiobuttonover",
        "audioerror",
        "itempickup",
        "adminsound",
        "brickclearsound",
        "clientjoinsound",
        "clientdropsound",
        "uploadstartsound",
        "uploadendsound",
        "processcompletesound",
    ];
    const BUILDING: &[&str] = &[
        "brickbreak",
        "brickmove",
        "brickplant",
        "brickrotate",
        "brickchange",
        "glassexplosionsound",
    ];
    const TOOLS: &[&str] = &[
        "hammerhitsound",
        "wrenchhitsound",
        "wrenchmisssound",
        "wandhitsound",
        "sprayfiresound",
        "sprayactivatesound",
        "printfiresound",
        "weaponswitchsound",
    ];
    const PLAYER: &[&str] = &[
        "jumpsound",
        "deathcrysound",
        "paincrysound",
        "armormovebubblessound",
        "waterbreathmalesound",
        "splash1sound",
        "exitwatersound",
        "playermountsound",
        "deathexplosionsound",
        "spawnexplosionsound",
        "lightonsound",
        "lightoffsound",
    ];
    if UI.contains(&n) || (n.starts_with("note") && n.ends_with("sound")) {
        "ui"
    } else if BUILDING.contains(&n) {
        "building"
    } else if TOOLS.contains(&n) {
        "tools"
    } else if PLAYER.contains(&n) {
        "player"
    } else if [
        "vehicleexplosionsound",
        "fastimpactsound",
        "slowimpactsound",
    ]
    .contains(&n)
    {
        "vehicles"
    } else if ["errorsound", "rewardsound"].contains(&n) {
        "gameplay"
    } else {
        "other"
    }
}

fn looks_like_sound_field(key: &str) -> bool {
    let k = key.split('[').next().unwrap_or(key);
    (k.contains("sound")
        && !k.contains("velocity")
        && !k.contains("volume")
        && !k.contains("brick"))
        || matches!(
            k,
            "impactwatereasy"
                | "impactwatermedium"
                | "impactwaterhard"
                | "exitingwater"
                | "soundprofile"
        )
}

fn candidates(all_audio: &[&Entry], resolved: &str) -> Vec<String> {
    let norm = |s: &str| lower(s).replace([' ', '_'], "");
    let stem = |p: &str| {
        let f = p.rsplit('/').next().unwrap_or(p);
        norm(f.rsplit_once('.').map(|(a, _)| a).unwrap_or(f))
    };
    let want = stem(resolved);
    let mut v: Vec<String> = all_audio
        .iter()
        .filter(|e| stem(&e.path) == want)
        .map(|e| e.path.clone())
        .collect();
    v.sort();
    v
}

fn canonical_name(sounds: &[SoundEntry], id: &str) -> String {
    sounds
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// Calls like `playAudio(0, lightOff)` naming a profile that does not exist.
fn unknown_api_refs(
    s: &Script,
    profiles: &HashMap<String, String>,
    descs: &HashMap<String, (String, DescValues)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let t = &s.tokens;
    for i in 0..t.len().saturating_sub(2) {
        let tscript::Tok::Ident(callee) = &t[i].tok else {
            continue;
        };
        let lc = lower(callee);
        let arg = match lc.as_str() {
            "alxplay" | "serverplay3d" | "serverplay2d" | "play2d" | "play3d" => 0,
            "playaudio" => 1,
            _ => continue,
        };
        if !matches!(t[i + 1].tok, tscript::Tok::Punct("(")) {
            continue;
        }
        // Walk to the requested argument at depth 1.
        let (mut k, mut depth, mut idx) = (i + 2, 1i32, 0usize);
        let mut arg_tokens = Vec::new();
        while k < t.len() && depth > 0 {
            match &t[k].tok {
                tscript::Tok::Punct("(") => depth += 1,
                tscript::Tok::Punct(")") => depth -= 1,
                tscript::Tok::Punct(",") if depth == 1 => {
                    idx += 1;
                    k += 1;
                    continue;
                }
                _ => {}
            }
            if depth > 0 && idx == arg {
                arg_tokens.push(&t[k]);
            }
            k += 1;
        }
        if let [tok] = arg_tokens.as_slice() {
            let name = match &tok.tok {
                tscript::Tok::Ident(n) | tscript::Tok::Str(n) => n,
                _ => continue,
            };
            let ln = lower(name);
            if !profiles.contains_key(&ln) && !descs.contains_key(&ln) && !ln.starts_with('$') {
                diagnostics.push(diag(
                    Severity::Warning,
                    "unknown-profile-reference",
                    format!(
                        "{callee}(...{name}...) names no AudioProfile; vanilla plays nothing here"
                    ),
                    vec![s.ev(tok.line)],
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn music_rule(
    vfs: &Vfs,
    scripts: &[Script],
    clips: &[ClipEntry],
    clip_by_id: &HashMap<String, usize>,
    path_to_clip: &HashMap<String, String>,
    descs: &HashMap<String, (String, DescValues)>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (MusicRuleRecord, Vec<SoundEntry>) {
    let mut evidence = Vec::new();
    let mut effective_script: Option<&Script> = None;
    for s in scripts {
        for (name, line) in &s.scan.functions {
            if lower(name) == "createmusicdatablocks"
                || lower(name) == "ismusicfilename"
                || lower(name) == "isvalidmusicfilename"
            {
                evidence.push(s.ev(*line));
                if lower(name) == "createmusicdatablocks" {
                    effective_script = Some(s);
                }
            }
        }
    }
    // Flags: base/server/defaultMusicList.cs (decompiled) — config/ is user state and ignored.
    let mut flags: HashMap<String, (String, Evidence)> = HashMap::new();
    for s in scripts
        .iter()
        .filter(|s| lower(&s.vpath) == "base/server/defaultmusiclist.cs")
    {
        for g in &s.scan.globals {
            if let Some(rest) = g.name.strip_prefix("$music__") {
                let v = match &g.value {
                    Value::Num(n) => format!("{n}"),
                    other => other.as_name().unwrap_or("").to_string(),
                };
                flags.insert(rest.to_string(), (v, s.ev(g.line)));
            }
        }
    }
    let flags_source: Vec<Evidence> = {
        let mut v: Vec<Evidence> = flags.values().map(|(_, e)| e.clone()).collect();
        v.sort();
        v.dedup_by(|a, b| a.file == b.file);
        v
    };
    let desc = descs.get("audiomusiclooping3d");
    if desc.is_none() {
        diagnostics.push(diag(
            Severity::Error,
            "music-description-missing",
            "AudioMusicLooping3d is not defined".into(),
            evidence.clone(),
        ));
    }
    let mut candidates_out = Vec::new();
    let mut sounds = Vec::new();
    let music_entries: Vec<&Entry> = vfs
        .entries()
        .filter(|e| {
            let l = lower(&e.path);
            l.starts_with("add-ons/music/") && l.ends_with(".ogg")
        })
        .collect();
    for e in music_entries {
        let file_rel = &e.path;
        let base = file_rel
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".ogg")
            .trim_end_matches(".OGG")
            .to_string();
        let ui_name = base.replace('_', " ");
        let safe: String = base
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let db = format!("musicData_{safe}");
        let flag = flags.get(&lower(&safe)).map(|(v, _)| v.clone());
        let clip_id = path_to_clip.get(&lower(file_rel)).cloned();
        let channels = clip_id
            .as_ref()
            .and_then(|c| clip_by_id.get(c))
            .map(|i| clips[*i].channels);
        let first_word = ui_name.split(' ').next().unwrap_or("");
        let reason = if first_word.parse::<f64>().is_ok() {
            Some("first word is a number (isValidMusicFilename)".to_string())
        } else if file_rel["Add-Ons/Music/".len()..].contains('/') {
            Some("inside a subdirectory".to_string())
        } else if [
            "Copy of", "Copy_of", "- Copy", "-_Copy", "(", ")", "[", "]", "+", " ",
        ]
        .iter()
        .any(|b| file_rel.contains(b))
        {
            Some("filename contains a rejected pattern".to_string())
        } else if e.size > 1_048_576 {
            Some("larger than 1 MiB".to_string())
        } else if flag.as_deref() != Some("1") {
            Some(format!("$Music__{safe} is {:?}, not 1", flag))
        } else if channels.is_none() {
            Some("clip could not be decoded".to_string())
        } else if channels != Some(1) {
            Some(format!(
                "not mono ({channels:?} channels): vanilla deletes stereo music datablocks"
            ))
        } else {
            None
        };
        let accepted = reason.is_none();
        candidates_out.push(MusicCandidate {
            file: file_rel.clone(),
            datablock: db.clone(),
            ui_name: ui_name.clone(),
            flag: flag.clone(),
            bytes: e.size,
            channels,
            accepted,
            reason: reason.clone(),
        });
        let playback = playback_of(desc.map(|(_, d)| d), true);
        let decodable = clip_id.as_ref().is_some_and(|c| clip_by_id.contains_key(c));
        let status = match (&reason, decodable) {
            (_, false) => SoundStatus::Undecodable {
                file: file_rel.clone(),
            },
            (None, true) => SoundStatus::Ready,
            (Some(r), true) => SoundStatus::RejectedByVanilla { reason: r.clone() },
        };
        let clip_id = clip_id.filter(|_| decodable);
        let ev = effective_script.map(|s| {
            let line = s
                .scan
                .functions
                .iter()
                .find(|(n, _)| lower(n) == "createmusicdatablocks")
                .map(|(_, l)| *l)
                .unwrap_or(1);
            s.ev(line)
        });
        sounds.push(SoundEntry {
            id: format!("v20/music/{}", lower(&safe)),
            name: db,
            clip: clip_id,
            description: desc.map(|(id, _)| id.clone()),
            playback,
            preload: true,
            lists: lists_of(true, desc.map(|(_, d)| d)),
            ui_name: Some(ui_name),
            family: "music".into(),
            package: "Music".into(),
            default_enabled: flag.as_deref() == Some("1"),
            layer: "rule".into(),
            status,
            defined_at: ev.unwrap_or(Evidence {
                file: "createMusicDatablocks".into(),
                line: 0,
            }),
        });
    }
    let rule = "For each Add-Ons/Music/*.ogg passing isValidMusicFilename, <= 1 MiB, with $Music__<name> = 1 \
                in base/server/defaultMusicList.cs and a mono clip: datablock AudioProfile(musicData_<name>) \
                { filename = <file>; description = AudioMusicLooping3d; preload = true; uiName = <name with _ as spaces>; }"
        .to_string();
    (
        MusicRuleRecord {
            rule,
            evidence,
            flags_source,
            candidates: candidates_out,
        },
        sounds,
    )
}

fn read_defaults(vfs: &Vfs) -> Result<(MixDefaults, Vec<PrefRecord>), String> {
    let mut d = MixDefaults {
        plant_error_sound: true,
        ..MixDefaults::default()
    };
    let mut prefs = Vec::new();
    let Some(e) = vfs.get("base/client/defaults.cs") else {
        return Ok((d, prefs));
    };
    let toks = tscript::lex(&text_of(&vfs.read(e)?));
    for g in tscript::scan(&toks, None).globals {
        let Some(key) = g.name.strip_prefix("$pref::audio::") else {
            continue;
        };
        let ev = Evidence {
            file: e.path.clone(),
            line: g.line,
        };
        prefs.push(PrefRecord {
            name: format!("$pref::Audio::{key}"),
            value: g.value.raw(),
            evidence: ev.clone(),
        });
        let v = g.value.as_f64();
        let b = v.map(|x| x != 0.0);
        match key {
            "mastervolume" => d.master_volume = v.unwrap_or(1.0) as f32,
            "playmusic" => d.play_music = b.unwrap_or(true),
            "menusounds" => d.menu_sounds = b.unwrap_or(true),
            "planterrorsound" => d.plant_error_sound = b.unwrap_or(false),
            "playbrickmovesound" => d.play_brick_move_sound = b.unwrap_or(true),
            "playbrickplantsound" => d.play_brick_plant_sound = b.unwrap_or(true),
            k if k.starts_with("channelvolume") => {
                if let Ok(n) = k["channelvolume".len()..].parse::<usize>()
                    && n < 9
                {
                    d.channel_volumes[n] = v.unwrap_or(1.0) as f32;
                }
            }
            _ => continue,
        }
        d.evidence.push(ev);
    }
    Ok((d, prefs))
}

struct Curated {
    key: &'static str,
    profile: &'static str,
    label: &'static str,
    placement: PlacementKind,
    note: Option<&'static str>,
}

const fn c(
    key: &'static str,
    profile: &'static str,
    label: &'static str,
    placement: PlacementKind,
) -> Curated {
    Curated {
        key,
        profile,
        label,
        placement,
        note: None,
    }
}
const fn cn(
    key: &'static str,
    profile: &'static str,
    label: &'static str,
    placement: PlacementKind,
    note: &'static str,
) -> Curated {
    Curated {
        key,
        profile,
        label,
        placement,
        note: Some(note),
    }
}

use PlacementKind::{Attached as A, Listener as L, World as W};

const CURATED: &[Curated] = &[
    cn(
        "ui.error",
        "AudioError",
        "client error beep (plant errors only when $Pref::Audio::PlantErrorSound)",
        L,
        "also used by clientCmd handlers for denied actions",
    ),
    c(
        "ui.admin",
        "AdminSound",
        "player became admin/super admin",
        L,
    ),
    c("ui.brick_clear", "BrickClearSound", "bricks cleared", L),
    c(
        "ui.client_join",
        "ClientJoinSound",
        "another client joined (MsgClientJoin)",
        L,
    ),
    c(
        "ui.client_drop",
        "ClientDropSound",
        "a client left (clientCmd ClientDrop)",
        L,
    ),
    c(
        "ui.upload_start",
        "UploadStartSound",
        "save upload / load started",
        L,
    ),
    c(
        "ui.upload_end",
        "UploadEndSound",
        "save upload / load finished",
        L,
    ),
    c(
        "ui.process_complete",
        "ProcessCompleteSound",
        "long process completed",
        L,
    ),
    c("ui.item_pickup", "ItemPickup", "item picked up (client)", L),
    cn(
        "ui.button_over",
        "AudioButtonOver",
        "GUI button hover",
        L,
        "authored file base/data/sound/buttonOver.wav is absent from the reference",
    ),
    cn(
        "ui.title_music",
        "TitleMusic",
        "main menu music (MainMenuGui)",
        L,
        "authored ~/data/sound/music/Ambient Deep.ogg is absent; Add-Ons/Music/Ambient_Deep.ogg is only a candidate",
    ),
    cn(
        "ui.menu_note.0",
        "Note0Sound",
        "menu note 0 ($Pref::Audio::MenuSounds)",
        L,
        "menu hover/click notes; see call-site triggers for the exact controls",
    ),
    c("ui.menu_note.1", "Note1Sound", "menu note 1", L),
    c("ui.menu_note.2", "Note2Sound", "menu note 2", L),
    c("ui.menu_note.3", "Note3Sound", "menu note 3", L),
    c("ui.menu_note.4", "Note4Sound", "menu note 4", L),
    c("ui.menu_note.5", "Note5Sound", "menu note 5", L),
    c("ui.menu_note.6", "Note6Sound", "menu note 6", L),
    c("ui.menu_note.7", "Note7Sound", "menu note 7", L),
    c("ui.menu_note.8", "Note8Sound", "menu note 8", L),
    c("ui.menu_note.9", "Note9Sound", "menu note 9", L),
    c("ui.menu_note.10", "Note10Sound", "menu note 10", L),
    c("ui.menu_note.11", "Note11Sound", "menu note 11", L),
    cn(
        "brick.plant",
        "BrickPlant",
        "brick planted (engine-bound client profile; $Pref::Audio::PlayBrickPlantSound)",
        W,
        "played by the engine; exact emit position (ghost brick) inferred",
    ),
    cn(
        "brick.move",
        "BrickMove",
        "ghost brick moved ($Pref::Audio::PlayBrickMoveSound)",
        W,
        "engine-bound; position inferred",
    ),
    cn(
        "brick.rotate",
        "BrickRotate",
        "ghost brick rotated",
        W,
        "engine-bound; position inferred",
    ),
    cn(
        "brick.change",
        "BrickChange",
        "ghost brick datablock changed",
        W,
        "engine-bound; position inferred",
    ),
    cn(
        "brick.break",
        "BrickBreak",
        "brick destroyed (BrickBreakSoundEvent)",
        W,
        "engine-bound network event; position inferred",
    ),
    c(
        "brick.glass_break",
        "glassExplosionSound",
        "static shape explosionSound (glass)",
        W,
    ),
    c("player.jump", "JumpSound", "PlayerData jumpSound", A),
    c(
        "player.water.impact_easy",
        "Splash1Sound",
        "entering water slowly (impactWaterEasy)",
        A,
    ),
    c(
        "player.water.impact_medium",
        "Splash1Sound",
        "entering water (impactWaterMedium, >= mediumSplashSoundVelocity 10)",
        A,
    ),
    c(
        "player.water.impact_hard",
        "Splash1Sound",
        "entering water fast (impactWaterHard, >= hardSplashSoundVelocity 20)",
        A,
    ),
    c(
        "player.water.exit",
        "exitWaterSound",
        "leaving water (exitingWater)",
        A,
    ),
    c(
        "player.death_cry",
        "DeathCrySound",
        "death cry (playAudio slot 0 unless PlayerData.deathSound)",
        A,
    ),
    c(
        "player.pain_cry",
        "PainCrySound",
        "pain cry (playAudio slot 0 unless PlayerData.painSound)",
        A,
    ),
    c(
        "player.mount",
        "playerMountSound",
        "player mounted a vehicle",
        W,
    ),
    c(
        "player.light_on",
        "lightOnSound",
        "player light toggled on",
        W,
    ),
    c(
        "player.light_off",
        "lightOffSound",
        "player light toggled off",
        W,
    ),
    c("player.spawn", "spawnExplosionSound", "spawn explosion", W),
    c(
        "player.body_remove",
        "deathExplosionSound",
        "corpse removal explosion",
        W,
    ),
    c("tool.hammer.hit", "hammerHitSound", "hammer hit", W),
    c("tool.wrench.hit", "wrenchHitSound", "wrench hit", W),
    c(
        "tool.wrench.miss",
        "wrenchMissSound",
        "wrench miss / denied",
        W,
    ),
    c(
        "tool.wand.hit",
        "wandHitSound",
        "wand/destructo wand hit (explosion soundProfile)",
        W,
    ),
    c(
        "tool.spray.activate",
        "sprayActivateSound",
        "spray can image activate (stateSound[0])",
        A,
    ),
    c(
        "tool.spray.fire",
        "sprayFireSound",
        "spray can firing loop (stateSound[2])",
        A,
    ),
    c(
        "tool.printer.fire",
        "printFireSound",
        "printer fire (stateSound[2])",
        A,
    ),
    c(
        "item.weapon_switch",
        "weaponSwitchSound",
        "weapon image activate (stateSound[0])",
        A,
    ),
    c(
        "vehicle.explosion",
        "vehicleExplosionSound",
        "vehicle explosion",
        W,
    ),
    cn(
        "vehicle.impact_soft",
        "slowImpactSound",
        "vehicle soft impact",
        A,
        "launcher-patch profile ('made default in v21')",
    ),
    cn(
        "vehicle.impact_hard",
        "fastImpactSound",
        "vehicle hard impact",
        A,
        "launcher-patch profile ('made default in v21')",
    ),
    c(
        "game.error",
        "errorSound",
        "server-side error sound at object",
        W,
    ),
    cn(
        "game.reward",
        "rewardSound",
        "reward sound: tutorial tips/win, minigame end, kill reward (client.play2D)",
        L,
        "also ServerPlay3D at the catcher in Item_Sports football",
    ),
    c("weapon.gun.fire", "gunShot1Sound", "gun fire", A),
    c("weapon.gun.hit", "bulletHitSound", "bullet impact", W),
    c("weapon.bow.fire", "bowFireSound", "bow fire", A),
    c("weapon.bow.hit", "arrowHitSound", "arrow impact", W),
    c(
        "weapon.rocket.fire",
        "rocketFireSound",
        "rocket launcher fire",
        A,
    ),
    c(
        "weapon.rocket.loop",
        "rocketLoopSound",
        "rocket projectile loop",
        A,
    ),
    c(
        "weapon.rocket.explode",
        "rocketExplodeSound",
        "rocket explosion",
        W,
    ),
    c("weapon.spear.fire", "spearFireSound", "spear throw", A),
    c("weapon.spear.hit", "spearExplosionSound", "spear impact", W),
    c("weapon.sword.draw", "swordDrawSound", "sword draw", A),
    c("weapon.sword.hit", "swordHitSound", "sword hit", W),
    c(
        "weapon.broom.swing",
        "pushBroomSwingSound",
        "push broom swing loop",
        A,
    ),
    c("weapon.broom.hit", "pushBroomHitSound", "push broom hit", W),
    c("vehicle.tank.fire", "TankShotSound", "tank cannon fire", W),
    c("vehicle.horse.jump", "HorseJumpSound", "horse jump", A),
    c(
        "vehicle.cannon.whistle",
        "WhistleLoopSound",
        "pirate cannonball whistle loop",
        A,
    ),
    c("emote.alarm", "AlarmSound", "alarm emote", A),
];

fn placement_for_binding(class: &str, field: &str) -> PlacementKind {
    let c = lower(class);
    let f = lower(field);
    if c == "explosiondata" || f == "explosionsound" {
        W
    } else if c == "guicontrolprofile" {
        L
    } else {
        A
    }
}

fn placement_for_api(api: &str) -> PlacementKind {
    match lower(api).as_str() {
        "alxplay" | "play2d" | "alxcreatesource" => L,
        "playaudio" => A,
        _ => W,
    }
}

fn build_triggers(
    sounds: &[SoundEntry],
    bindings: &[BindingRecord],
    calls: &[CallSiteRecord],
    engine: &[EngineBoundRecord],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<TriggerEntry> {
    let by_name: HashMap<String, &SoundEntry> =
        sounds.iter().map(|s| (lower(&s.name), s)).collect();
    let mut out: BTreeMap<String, TriggerEntry> = BTreeMap::new();
    let evidence_for = |sid: &str| -> Vec<Evidence> {
        let mut v: Vec<Evidence> = bindings
            .iter()
            .filter(|b| b.sound == sid)
            .map(|b| b.evidence.clone())
            .collect();
        v.extend(
            calls
                .iter()
                .filter(|c| c.sound.as_deref() == Some(sid) && c.api != "reference")
                .map(|c| c.evidence.clone()),
        );
        v.sort();
        v.dedup();
        v
    };
    for cur in CURATED {
        let Some(s) = by_name.get(&lower(cur.profile)) else {
            diagnostics.push(diag(
                Severity::Warning,
                "curated-trigger-stale",
                format!(
                    "curated trigger {} names missing profile {}",
                    cur.key, cur.profile
                ),
                vec![],
            ));
            continue;
        };
        let mut ev = evidence_for(&s.id);
        if ev.is_empty() {
            ev.push(s.defined_at.clone());
        }
        let source = if engine.iter().any(|e| e.sound == s.id) {
            "engine"
        } else {
            "curated"
        };
        out.insert(
            cur.key.to_string(),
            TriggerEntry {
                key: cur.key.into(),
                label: cur.label.into(),
                sound: s.id.clone(),
                placement: cur.placement,
                source: source.into(),
                package: s.package.clone(),
                evidence: ev,
                note: cur.note.map(Into::into),
            },
        );
    }
    for b in bindings {
        let key = format!("datablock:{}.{}", b.datablock, b.field);
        out.entry(key.clone()).or_insert_with(|| TriggerEntry {
            key,
            label: match &b.detail {
                Some(d) => format!(
                    "{} {} field {} ({d})",
                    b.datablock_class, b.datablock, b.field
                ),
                None => format!("{} {} field {}", b.datablock_class, b.datablock, b.field),
            },
            sound: b.sound.clone(),
            placement: placement_for_binding(&b.datablock_class, &b.field),
            source: "datablock-field".into(),
            package: b.package.clone(),
            evidence: vec![b.evidence.clone()],
            note: None,
        });
    }
    for c in calls.iter().filter(|c| c.api != "reference") {
        let Some(sid) = &c.sound else { continue };
        let site = c
            .function
            .clone()
            .unwrap_or_else(|| format!("{}:{}", c.evidence.file, c.evidence.line));
        let key = format!("call:{site}:{}", c.profile);
        let e = out.entry(key.clone()).or_insert_with(|| TriggerEntry {
            key,
            label: format!("{} in {site}", c.api),
            sound: sid.clone(),
            placement: placement_for_api(&c.api),
            source: "script-call".into(),
            package: c.package.clone(),
            evidence: vec![],
            note: None,
        });
        if !e.evidence.contains(&c.evidence) {
            e.evidence.push(c.evidence.clone());
        }
    }
    for e in engine {
        let key = format!("engine:{}", e.profile);
        out.entry(key.clone()).or_insert_with(|| TriggerEntry {
            key,
            label: format!(
                "engine-bound client profile {} (loadBrickSounds)",
                e.profile
            ),
            sound: e.sound.clone(),
            placement: W,
            source: "engine".into(),
            package: "base".into(),
            evidence: by_name
                .get(&lower(&e.profile))
                .map(|s| vec![s.defined_at.clone()])
                .unwrap_or_default(),
            note: Some(format!(
                "name found in the engine executable at offset {}",
                e.executable_offset
            )),
        });
    }
    for s in sounds.iter().filter(|s| s.layer == "rule") {
        let key = format!("music-brick:{}", s.name.trim_start_matches("musicData_"));
        out.insert(
            key.clone(),
            TriggerEntry {
                key,
                label: format!("music brick / setMusic event playing \"{}\" (looping AudioEmitter at the brick)", s.ui_name.clone().unwrap_or_default()),
                sound: s.id.clone(),
                placement: A,
                source: "rule".into(),
                package: s.package.clone(),
                evidence: vec![s.defined_at.clone()],
                note: None,
            },
        );
    }
    out.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn wav(seconds: f32, channels: u16) -> Vec<u8> {
        let n = (22_050.0 * seconds) as usize * channels as usize;
        let s: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect();
        bri_audio::wav::encode_pcm16(22_050, channels, &s)
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    /// A miniature installation exercising every resolution rule.
    fn fixture() -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("bri-audio-import-fixture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let install = root.join("install");
        let dec = root.join("dec");
        put(&install, "base/data/sound/jump.wav", &wav(0.2, 1));
        put(
            &install,
            "base/data/sound/Notes/Synth4_00.WAV",
            &wav(0.2, 1),
        );
        put(&install, "base/data/sound/unused.wav", &wav(0.1, 1));
        put(
            &install,
            "base/server/defaultAddOnList.cs",
            b"$AddOn__Weapon_Test = 1;\n$AddOn__Weapon_Other = 0;\n",
        );
        put(
            &install,
            "base/client/defaults.cs",
            b"$pref::Audio::masterVolume = 0.9;\n$Pref::Audio::PlayMusic = 1;\n",
        );
        put(
            &install,
            "base/client/scripts/allClientScripts-Vanilla.cs.dso",
            b"dso-bytes",
        );
        put(&install, "base/server/scripts/allGameScripts.cs", b"exec(\"./allGameScripts-Vanilla.cs\");\n// B4v21 patch\ndatablock AudioProfile(slowImpactSound) { fileName = \"base/data/sound/jump.wav\"; description = AudioClose3d; };\n");
        put(&install, "Add-Ons/Music/Calm_Loop.ogg", &[]); // not a valid ogg: must be reported, not fatal
        put(&install, "Add-Ons/Music/1_Bad.ogg", b"x");
        put(
            &install,
            "blocklandv20.exe",
            b"xx\0BrickPlant\0JumpSound\0yy",
        );
        // Add-on archive.
        let mut zbuf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut zbuf));
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("server.cs", o).unwrap();
            z.write_all(
                br#"datablock AudioProfile(testShotSound) { filename = "./shot.wav"; description = AudioClose3d; preload = true; };
datablock AudioProfile(testShot2Sound : testShotSound) { filename = "./shot2.wav"; };
if(!isObject(testShotSound)) { datablock AudioProfile(testShotSound) { filename = "./nope/shot.wav"; }; }
datablock ShapeBaseImageData(testImage) { stateName[2] = "Fire"; stateSound[2] = testShotSound; stateSound[3] = noSuchSound; };
function testImage::onFire(%this, %obj) { ServerPlay3D(testShot2Sound, %obj.getPosition()); %obj.playAudio(0, missingThing); }
"#,
            )
            .unwrap();
            z.start_file("shot.wav", o).unwrap();
            z.write_all(&wav(0.3, 1)).unwrap();
            z.start_file("shot2.wav", o).unwrap();
            z.write_all(&wav(0.3, 2)).unwrap();
            z.finish().unwrap();
        }
        put(&install, "Add-Ons/Weapon_Test.zip", &zbuf);
        // "Decompiled" base scripts.
        put(
            &dec,
            "client/scripts/allClientScripts-Vanilla.cs",
            br#"$GuiAudioType = 1;
$SimAudioType = 2;
new AudioDescription(AudioGui) { volume = 1; isLooping = 0; is3D = 0; type = $GuiAudioType; };
new AudioProfile(Note0Sound) { fileName = "~/data/sound/notes/Synth 4/Synth4_00.wav"; description = "AudioGui"; preload = 1; };
new AudioProfile(Note1Sound) { fileName = "~/data/sound/Notes/Synth4_00.wav"; description = "AudioGui"; preload = 1; };
new AudioProfile(BrickPlant) { fileName = "~/data/sound/jump.wav"; description = "AudioGui"; };
function handleX() { alxPlay(Note1Sound); }
"#,
        );
        put(
            &dec,
            "client/scripts/allClientScripts-Vanilla.cs.dso",
            b"dso-bytes",
        );
        put(
            &dec,
            "server/scripts/allGameScripts-Vanilla.cs",
            br#"datablock AudioDescription(AudioClose3d) { volume = 1; isLooping = 0; is3D = 1; ReferenceDistance = 10; maxDistance = 60; type = $SimAudioType; };
datablock AudioDescription(AudioMusicLooping3d) { volume = 1; isLooping = 1; is3D = 1; ReferenceDistance = 10; maxDistance = 30; type = $SimAudioType; };
datablock AudioProfile(JumpSound) { fileName = "base/data/sound/JUMP.wav"; description = AudioClose3d; preload = 1; };
datablock PlayerData(PlayerStandardArmor) { JumpSound = JumpSound; };
function createMusicDatablocks() { }
"#,
        );
        put(
            &dec,
            "server/defaultMusicList.cs",
            b"$Music__Calm_Loop = 1;\n$Music__1_Bad = 1;\n",
        );
        (install, dec)
    }

    #[test]
    fn converts_fixture_with_every_rule_and_diagnostic() {
        let (install, dec) = fixture();
        let opts = Options {
            v20: install.clone(),
            decompiled: dec.clone(),
            decompiled_label: ".research/v20-dso".into(),
            pack_id: "audio-pack-test".into(),
            reference_label: "fixture".into(),
            arguments: vec![],
            stream_threshold_seconds: 20.0,
        };
        let c = convert(&opts).unwrap();
        let m = &c.manifest;
        let dso = c
            .inventory
            .scripts
            .iter()
            .find_map(|s| {
                s.original_dso
                    .as_ref()
                    .filter(|d| d.install_path.ends_with("allClientScripts-Vanilla.cs.dso"))
            })
            .unwrap();
        assert_eq!(dso.identical, Some(true));
        let snd = |name: &str| {
            m.sounds
                .iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let has = |code: &str, needle: &str| {
            m.diagnostics
                .iter()
                .any(|d| d.code == code && d.message.contains(needle))
        };

        // ~/ and case-insensitive resolution; "Synth 4" path is missing -> diagnostic.
        assert!(snd("Note1Sound").is_ready());
        assert!(matches!(
            snd("Note0Sound").status,
            SoundStatus::MissingClip { .. }
        ));
        assert!(has("missing-clip", "Note0Sound"));
        assert!(
            snd("JumpSound").is_ready(),
            "JUMP.wav resolves case-insensitively"
        );
        assert!(has("filename-case", "JumpSound"));
        // Globals resolve channel; description values.
        let j = snd("JumpSound");
        assert_eq!(j.playback.channel, 2);
        assert_eq!(j.playback.spatial.unwrap().max_distance, 60.0);
        assert_eq!(snd("Note1Sound").playback.channel, 1);
        assert!(snd("Note1Sound").playback.spatial.is_none());
        // ./ inside a zip, inheritance, guarded duplicate skipped with alternate warning.
        let shot = snd("testShotSound");
        assert!(shot.is_ready() && shot.package == "Weapon_Test" && shot.default_enabled);
        assert_eq!(
            snd("testShot2Sound").playback.spatial,
            shot.playback.spatial,
            "inherits description"
        );
        assert!(has("duplicate-definition", "testShotSound"));
        assert!(has("missing-clip-alternate", "testShotSound"));
        // Launcher-patch layer.
        assert_eq!(snd("slowImpactSound").layer, "launcher-patch");
        // Bindings, calls, unknown references.
        assert!(
            c.inventory
                .bindings
                .iter()
                .any(|b| b.profile == "testShotSound"
                    && b.detail.as_deref() == Some("state [2] \"Fire\""))
        );
        assert!(
            c.inventory
                .bindings
                .iter()
                .any(|b| b.profile == "JumpSound" && b.datablock == "PlayerStandardArmor")
        );
        assert!(has("unknown-profile-binding", "noSuchSound"));
        assert!(has("unknown-profile-reference", "missingThing"));
        assert!(
            c.inventory
                .call_sites
                .iter()
                .any(|cs| cs.profile == "testShot2Sound"
                    && cs.api == "ServerPlay3D"
                    && cs.function.as_deref() == Some("testImage::onFire"))
        );
        // Engine binding: only the client-local profile, not the JumpSound field name.
        let eng: Vec<_> = c
            .inventory
            .engine_bound
            .iter()
            .map(|e| e.profile.as_str())
            .collect();
        assert_eq!(eng, vec!["BrickPlant"]);
        // Music rule: invalid ogg -> undecodable clip, numeric first word rejected.
        assert!(has("clip-undecodable", "calm_loop"));
        let bad = c
            .inventory
            .music_rule
            .candidates
            .iter()
            .find(|x| x.file.ends_with("1_Bad.ogg"))
            .unwrap();
        assert!(!bad.accepted && bad.reason.as_deref().unwrap().contains("number"));
        assert!(
            m.sounds
                .iter()
                .any(|s| s.name == "musicData_Calm_Loop" && !s.is_ready())
        );
        // Unreferenced files and stereo clip facts.
        assert!(has("unreferenced-audio-file", "unused.wav"));
        let shot2 = m
            .clips
            .iter()
            .find(|x| x.id.ends_with("shot2.wav"))
            .unwrap();
        assert_eq!(shot2.channels, 2);
        // Stable, deterministic ids and a valid manifest.
        assert!(m.sounds.iter().all(|s| s.id.starts_with("v20/")));
        m.validate().unwrap();
        let again = convert(&opts).unwrap();
        assert_eq!(
            serde_json::to_string(&again.manifest).unwrap(),
            serde_json::to_string(m).unwrap(),
            "deterministic"
        );
        assert_eq!(m.defaults.master_volume, 0.9);
        let _ = std::fs::remove_dir_all(install.parent().unwrap());
    }
}
