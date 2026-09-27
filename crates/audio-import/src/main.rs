//! `bri-audio-import` — build a native audio pack from the vanilla reference.
//!
//! ```text
//! bri-audio-import --v20 <install root> --decompiled .research/v20-dso
//!                  (--out content/audio-pack-001 | --out-root content)
//!                  [--evidence artifacts/native-audio] [--coverage docs/research/audio/coverage.md]
//!                  [--label "text"] [--decompiled-label .research/v20-dso]
//! ```
//! The installation is only read. Scripts are scanned as text, never executed.
//! An existing output directory is never overwritten.

mod convert;
mod model;
mod report;
mod tscript;
mod vfs;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

struct Args {
    v20: PathBuf,
    decompiled: PathBuf,
    decompiled_label: String,
    out: Option<PathBuf>,
    out_root: Option<PathBuf>,
    evidence: Option<PathBuf>,
    coverage: Option<PathBuf>,
    label: String,
    raw: Vec<String>,
}

fn parse() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut it = raw.iter();
    let (
        mut v20,
        mut dec,
        mut dec_label,
        mut out,
        mut out_root,
        mut evidence,
        mut coverage,
        mut label,
    ) = (None, None, None, None, None, None, None, None);
    while let Some(a) = it.next() {
        let mut val = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--v20" => v20 = Some(PathBuf::from(val()?)),
            "--decompiled" => dec = Some(val()?),
            "--decompiled-label" => dec_label = Some(val()?),
            "--out" => out = Some(PathBuf::from(val()?)),
            "--out-root" => out_root = Some(PathBuf::from(val()?)),
            "--evidence" => evidence = Some(PathBuf::from(val()?)),
            "--coverage" => coverage = Some(PathBuf::from(val()?)),
            "--label" => label = Some(val()?),
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let dec = dec.ok_or_else(|| format!("--decompiled is required\n{USAGE}"))?;
    if out.is_some() == out_root.is_some() {
        return Err(format!("give exactly one of --out or --out-root\n{USAGE}"));
    }
    Ok(Args {
        v20: v20.ok_or_else(|| format!("--v20 is required\n{USAGE}"))?,
        decompiled_label: dec_label.unwrap_or_else(|| dec.replace('\\', "/")),
        decompiled: PathBuf::from(dec),
        out,
        out_root,
        evidence,
        coverage,
        label: label.unwrap_or_else(|| "Blockland v20 reference installation (read-only)".into()),
        raw,
    })
}

const USAGE: &str = "usage: bri-audio-import --v20 <install> --decompiled <dir> (--out <new dir> | --out-root <dir>) \
[--evidence <dir>] [--coverage <file.md>] [--label <text>] [--decompiled-label <text>]";

fn next_pack(root: &Path) -> PathBuf {
    (1..1000)
        .map(|n| root.join(format!("audio-pack-{n:03}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| root.join("audio-pack-overflow"))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let args = parse()?;
    let out = match (&args.out, &args.out_root) {
        (Some(o), _) => {
            if o.exists() {
                let root = o.parent().unwrap_or(Path::new("."));
                return Err(format!(
                    "{} already exists; use a fresh pack such as {}",
                    o.display(),
                    next_pack(root).display()
                ));
            }
            o.clone()
        }
        (None, Some(root)) => next_pack(root),
        _ => unreachable!(),
    };
    let pack_id = out
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "audio-pack".into());
    let opts = convert::Options {
        v20: args.v20.clone(),
        decompiled: args.decompiled.clone(),
        decompiled_label: args.decompiled_label.clone(),
        pack_id: pack_id.clone(),
        reference_label: args.label.clone(),
        arguments: args.raw.clone(),
        stream_threshold_seconds: 20.0,
    };
    let conv = convert::convert(&opts)?;

    // Stage into a temporary sibling, then rename, so a failed run leaves nothing half-written.
    let tmp = out.with_file_name(format!(".{pack_id}.partial"));
    if tmp.exists() {
        return Err(format!(
            "{} exists from an interrupted run; remove it first",
            tmp.display()
        ));
    }
    for c in &conv.clips {
        write(&tmp.join(&c.file), &c.bytes)?;
    }
    let json = |v: &dyn erased::Json| v.to_pretty();
    write(&tmp.join("manifest.json"), json(&conv.manifest).as_bytes())?;
    write(
        &tmp.join("inventory.json"),
        json(&conv.inventory).as_bytes(),
    )?;
    write(&tmp.join("coverage.md"), conv.coverage_markdown.as_bytes())?;
    write(
        &tmp.join("README.md"),
        pack_readme(&conv.manifest).as_bytes(),
    )?;

    // Self-check: the runtime must load the pack with hash verification.
    let bank = bri_audio::SoundBank::load(
        &tmp,
        &bri_audio::BankOptions {
            verify_hashes: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("self-check failed: {e}"))?;
    let ready = conv.manifest.sounds.iter().filter(|s| s.is_ready()).count();
    if bank.ready_count() != ready {
        return Err(format!(
            "self-check: runtime loaded {} of {ready} ready sounds",
            bank.ready_count()
        ));
    }
    std::fs::rename(&tmp, &out)
        .map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), out.display()))?;

    if let Some(ev) = &args.evidence {
        write(
            &ev.join(format!("{pack_id}-clip-validation.json")),
            json(&conv.validation).as_bytes(),
        )?;
        let ready: std::collections::HashSet<&str> = conv
            .manifest
            .sounds
            .iter()
            .filter(|s| s.is_ready())
            .map(|s| s.id.as_str())
            .collect();
        let unready_bindings: Vec<&str> = conv
            .inventory
            .bindings
            .iter()
            .filter(|b| !ready.contains(b.sound.as_str()))
            .map(|b| b.profile.as_str())
            .collect();
        let mut unready_calls: Vec<&str> = conv
            .inventory
            .call_sites
            .iter()
            .filter(|c| {
                c.api != "reference" && !c.sound.as_deref().is_some_and(|s| ready.contains(s))
            })
            .map(|c| c.profile.as_str())
            .collect();
        unready_calls.sort_unstable();
        unready_calls.dedup();
        let summary = serde_json::json!({
            "pack": out.display().to_string(),
            "summary": conv.inventory.summary,
            "binding_check": {
                "datablock_bindings": conv.inventory.bindings.len(),
                "bindings_to_unavailable_sounds": unready_bindings,
                "script_calls_to_unavailable_sounds": unready_calls,
                "triggers_to_unavailable_sounds": conv.manifest.triggers.iter().filter(|t| !ready.contains(t.sound.as_str())).map(|t| t.key.as_str()).collect::<Vec<_>>(),
            },
            "runtime_self_check": {
                "sounds_ready": bank.ready_count(),
                "resident_bytes": bank.resident_bytes(),
                "streamed_clips": bank.streamed_clips(),
                "hashes_verified": true,
            },
        });
        write(
            &ev.join(format!("{pack_id}-import-summary.json")),
            serde_json::to_string_pretty(&summary)
                .unwrap_or_default()
                .as_bytes(),
        )?;
    }
    if let Some(c) = &args.coverage {
        write(c, conv.coverage_markdown.as_bytes())?;
    }
    let s = &conv.inventory.summary;
    println!(
        "{}: {} audio files ({} wav, {} ogg) -> {} unique clips ({} decoded, {} failed); {} descriptions; {} sounds ({} ready, {} unavailable); {} triggers; diagnostics {:?}",
        out.display(),
        s.audio_files,
        s.wav_files,
        s.ogg_files,
        s.unique_clips,
        s.clips_decoded,
        s.clips_failed,
        s.descriptions,
        conv.manifest.sounds.len(),
        s.sounds_ready,
        s.sounds_unavailable,
        s.triggers,
        s.diagnostics_by_severity
    );
    Ok(())
}

fn pack_readme(m: &bri_audio::schema::PackManifest) -> String {
    format!(
        "# {}\n\nGenerated native audio pack (schema `{}` v{}). Original clip bytes are stored unmodified \
         under `clips/<sha256>.<ext>`. `manifest.json` is the runtime input; `inventory.json` is the \
         complete audit trail; `coverage.md` is the generated coverage table.\n\n\
         Generated content: do not edit. Re-run `bri-audio-import` into a fresh numbered pack instead.\n\
         Contains original game audio: never commit this directory.\n",
        m.pack_id, m.schema, m.schema_version
    )
}

mod erased {
    pub trait Json {
        fn to_pretty(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn to_pretty(&self) -> String {
            serde_json::to_string_pretty(self).unwrap_or_default()
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bri-audio-import: {e}");
            ExitCode::FAILURE
        }
    }
}
