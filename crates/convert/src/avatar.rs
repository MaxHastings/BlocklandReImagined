//! Read declarative TSShapeConstructor aliases without executing TorqueScript.
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct Constructor {
    pub shape: (String, usize),
    pub sequences: Vec<(String, String, usize)>,
}

pub fn constructor(text: &str, name: &str) -> Result<Constructor> {
    let header = format!("datablock TSShapeConstructor({name})");
    let mut lines = text.lines().enumerate();
    lines
        .find(|(_, s)| s.trim() == header)
        .context("Missing avatar constructor")?;
    ensure!(
        lines.next().is_some_and(|(_, s)| s.trim() == "{"),
        "Missing constructor brace"
    );
    let mut shape = None;
    let mut sequences = BTreeMap::new();
    let mut aliases = BTreeSet::new();
    let mut closed = false;
    for (line, raw) in lines {
        let s = raw.trim();
        if s == "};" {
            closed = true;
            break;
        }
        if s.is_empty() || s.starts_with("//") {
            continue;
        }
        let (field, value) = s.split_once('=').context("Invalid constructor field")?;
        let value = value
            .trim()
            .strip_suffix(';')
            .context("Missing field terminator")?
            .trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .context("Expected literal constructor value")?;
        ensure!(
            !value.contains(['"', '\\', '$', '@']),
            "Nonliteral constructor value"
        );
        if field.trim() == "baseShape" {
            ensure!(
                shape.replace((value.to_string(), line + 1)).is_none(),
                "Duplicate base shape"
            );
        } else {
            let index: usize = field
                .trim()
                .strip_prefix("sequence")
                .context("Unsupported constructor field")?
                .parse()?;
            let words: Vec<_> = value.split_whitespace().collect();
            ensure!(
                words.len() == 2 && index < 256,
                "Invalid sequence declaration"
            );
            let alias = words[1].to_ascii_lowercase();
            ensure!(aliases.insert(alias.clone()), "Duplicate sequence alias");
            ensure!(
                sequences
                    .insert(index, (words[0].to_string(), alias, line + 1))
                    .is_none(),
                "Duplicate sequence index"
            );
        }
    }
    ensure!(closed && !sequences.is_empty(), "Incomplete constructor");
    ensure!(
        sequences.keys().copied().eq(0..sequences.len()),
        "Sequence indices must be contiguous"
    );
    Ok(Constructor {
        shape: shape.context("Missing base shape")?,
        sequences: sequences.into_values().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "datablock TSShapeConstructor(mDts)\n{\nbaseShape = \"m.dts\";\nsequence0 = \"run.dsq run\";\nsequence1 = \"run.dsq walk\";\n};";
    #[test]
    fn aliases_preserve_reused_sources_and_evidence_lines() {
        let c = constructor(SOURCE, "mDts").unwrap();
        assert_eq!(c.shape, ("m.dts".into(), 3));
        assert_eq!(c.sequences[1], ("run.dsq".into(), "walk".into(), 5));
    }
    #[test]
    fn malformed_or_executable_fields_do_not_get_silently_accepted() {
        for source in [
            SOURCE.replace("sequence1", "sequence3"),
            SOURCE.replace("run.dsq walk", "run.dsq RUN"),
            SOURCE.replace("\"m.dts\"", "getShape()"),
            SOURCE.replace("};", ""),
            SOURCE.replace("sequence1", "other"),
        ] {
            assert!(constructor(&source, "mDts").is_err());
        }
    }
}
