//! Original brick surface images and stock prints in a native, versioned bundle.
//! Conversion/provenance readers live in the offline converter crate.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: u32 = 1;
pub const SURFACES: [&str; 5] = ["top", "side", "bottom_edge", "bottom_loop", "ramp"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Installation-relative file or archive member. Evidence paths are informational.
    pub path: String,
    pub archive: Option<String>,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    /// Bundle-relative PNG; bytes are identical to the original source.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    pub source: Source,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Print {
    pub id: String,
    /// Original filename stem, including letter case and punctuation names.
    pub name: String,
    /// Legacy compatibility class (not the image's numeric aspect ratio).
    pub aspect: String,
    pub package: String,
    /// Original BLS/printNameTable tokens, e.g. `Letters/A` or `2x2r/monitor3`.
    pub aliases: Vec<String>,
    pub diffuse: Image,
    pub icon: Image,
}
/// The print class every printable brick takes (v20's letters).
pub const UNIVERSAL_PRINT_ASPECT: &str = "Letters";
/// Whether a print of class `print_aspect` fits a brick of class
/// `brick_aspect`: v20 serverCmdSetPrint accepts the brick's class and
/// [`UNIVERSAL_PRINT_ASPECT`] for every printable brick. An empty class
/// identifies a non-printable brick.
pub fn print_fits(print_aspect: &str, brick_aspect: &str) -> bool {
    !brick_aspect.is_empty()
        && (print_aspect.eq_ignore_ascii_case(brick_aspect)
            || print_aspect.eq_ignore_ascii_case(UNIVERSAL_PRINT_ASPECT))
}
impl Print {
    /// Whether this print fits a brick of class `aspect` ([`print_fits`]).
    pub fn compatible(&self, aspect: &str) -> bool {
        print_fits(&self.aspect, aspect)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub archive: String,
    pub archive_sha256: String,
    pub default_list_line: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub schema_version: u32,
    pub surfaces: BTreeMap<String, Image>,
    pub prints: Vec<Print>,
    pub packages: Vec<Package>,
    pub evidence: Vec<Source>,
    /// Present packages lacking the stock evidence required by this conversion.
    pub excluded_installed_packages: Vec<String>,
    pub warnings: Vec<String>,
}
/// Stock print classes renamed since older Blockland versions. Each old
/// class held the same image names as its v20 package: `2x2` the
/// `Print_2x2f_Default` set (arrow, blank, circle, ...), `2x1` the
/// `Print_1x2f_Default` set (letter1, keyboard, vent, ...), `1x1r` the
/// `Print_2x2r_Default` set (monitor1, medical1, radar1, vent).
const RENAMED_CLASSES: [(&str, &str); 3] = [("2x2", "2x2f"), ("2x1", "1x2f"), ("1x1r", "2x2r")];

/// The v20 `<class>/<name>` alias an older save's print token means, when it
/// differs from the token. Older saves store the texture path
/// (`base/data/prints/Letters/A.png`), v20 Add-On paths
/// (`Add-Ons/Print_2x2f_Default/prints/arrow.png`) or a renamed class
/// (`2x1/vent`). Anything else (`NOPRINT`, an Add-On's own print) is `None`.
pub fn legacy_print_alias(token: &str) -> Option<String> {
    let path = token.trim().replace('\\', "/");
    let parts: Vec<&str> = path.split('/').collect();
    let starts = |prefix: &[&str]| {
        parts.len() > prefix.len()
            && prefix
                .iter()
                .zip(&parts)
                .all(|(p, s)| p.eq_ignore_ascii_case(s))
    };
    let (class, name) = match parts.as_slice() {
        // `base/data/prints/<class>/<name>.png`
        [_, _, _, class, file] if starts(&["base", "data", "prints"]) => (*class, strip_png(file)),
        // `Add-Ons/Print_<class>_<package>/prints/<name>.png`
        [addons, package, prints, file]
            if addons.eq_ignore_ascii_case("add-ons") && prints.eq_ignore_ascii_case("prints") =>
        {
            let mut words = package.split('_');
            let print = words.next()?;
            if !print.eq_ignore_ascii_case("print") {
                return None;
            }
            (words.next()?, strip_png(file))
        }
        [class, name] => (*class, *name),
        _ => return None,
    };
    if class.is_empty() || name.is_empty() {
        return None;
    }
    let class = RENAMED_CLASSES
        .iter()
        .find(|(old, _)| old.eq_ignore_ascii_case(class))
        .map_or(class, |(_, new)| new);
    let alias = format!("{class}/{name}");
    (alias != token).then_some(alias)
}
fn strip_png(file: &str) -> &str {
    file.len()
        .checked_sub(4)
        .and_then(|cut| Some((file.get(..cut)?, file.get(cut..)?)))
        .filter(|(_, ext)| ext.eq_ignore_ascii_case(".png"))
        .map_or(file, |(stem, _)| stem)
}
pub fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
impl Bundle {
    pub fn compatible_prints<'a>(&'a self, aspect: &'a str) -> impl Iterator<Item = &'a Print> {
        self.prints
            .iter()
            .filter(move |print| print.compatible(aspect))
    }
    /// Numeric original print indices are session-local and never stable IDs.
    /// Older saves' names ([`legacy_print_alias`]) resolve to the stock
    /// print they name.
    pub fn resolve(&self, name: &str) -> Option<&Print> {
        let find = |name: &str| {
            self.prints
                .iter()
                .find(|p| p.id == name || p.aliases.iter().any(|a| a.eq_ignore_ascii_case(name)))
        };
        find(name).or_else(|| find(&legacy_print_alias(name)?))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA,
            "Unsupported brick materials schema"
        );
        ensure!(
            self.surfaces.len() == 5 && SURFACES.iter().all(|s| self.surfaces.contains_key(*s)),
            "Missing or unknown brick surface"
        );
        ensure!(
            self.prints.len() <= 4096 && self.packages.len() <= 256,
            "Brick materials catalog exceeds bounds"
        );
        let mut packages = BTreeSet::new();
        for p in &self.packages {
            ensure!(
                p.name.starts_with("Print_")
                    && packages.insert(&p.name)
                    && safe_relative(&p.archive)
                    && hash(&p.archive_sha256)
                    && p.default_list_line > 0,
                "Invalid/duplicate print package"
            );
        }
        let mut ids = BTreeSet::new();
        let mut aliases = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for p in &self.prints {
            ensure!(
                p.id.starts_with("print/")
                    && safe_relative(&p.id)
                    && ids.insert(&p.id)
                    && packages.contains(&p.package),
                "Invalid/duplicate print identity or package"
            );
            ensure!(
                !p.name.is_empty()
                    && !p.name.contains(['/', '\\', ' '])
                    && !p.aspect.is_empty()
                    && !p.aliases.is_empty(),
                "Invalid print name/aspect"
            );
            for a in &p.aliases {
                ensure!(
                    safe_relative(a) && aliases.insert(a.to_ascii_lowercase()),
                    "Invalid/ambiguous print alias"
                );
            }
        }
        for image in self.images() {
            ensure!(
                safe_relative(&image.path)
                    && image.path.ends_with(".png")
                    && paths.insert(&image.path),
                "Invalid/duplicate image output path"
            );
            ensure!(
                (1..=8192).contains(&image.width)
                    && (1..=8192).contains(&image.height)
                    && u64::from(image.width) * u64::from(image.height) <= 16_777_216,
                "Invalid image dimensions"
            );
            ensure!(
                hash(&image.sha256)
                    && image.sha256 == image.source.sha256
                    && safe_relative(&image.source.path)
                    && image
                        .source
                        .archive
                        .as_ref()
                        .is_none_or(|a| safe_relative(a)),
                "Invalid image provenance or byte preservation"
            );
        }
        ensure!(
            self.evidence.iter().all(|s| hash(&s.sha256)),
            "Invalid evidence hash"
        );
        Ok(())
    }
    pub fn images(&self) -> impl Iterator<Item = &Image> {
        self.surfaces
            .values()
            .chain(self.prints.iter().flat_map(|p| [&p.diffuse, &p.icon]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(path: &str) -> Image {
        Image {
            path: path.into(),
            width: 1,
            height: 1,
            sha256: "a".repeat(64),
            source: Source {
                path: path.into(),
                archive: None,
                sha256: "a".repeat(64),
            },
        }
    }
    fn bundle() -> Bundle {
        Bundle {
            schema_version: SCHEMA,
            surfaces: SURFACES
                .into_iter()
                .map(|s| (s.into(), image(&format!("{s}.png"))))
                .collect(),
            prints: vec![Print {
                id: "print/print_letters_default/a".into(),
                name: "A".into(),
                aspect: "Letters".into(),
                package: "Print_Letters_Default".into(),
                aliases: vec!["Letters/A".into()],
                diffuse: image("a.png"),
                icon: image("a-icon.png"),
            }],
            packages: vec![Package {
                name: "Print_Letters_Default".into(),
                archive: "Add-Ons/Print_Letters_Default.zip".into(),
                archive_sha256: "b".repeat(64),
                default_list_line: 45,
            }],
            evidence: vec![],
            excluded_installed_packages: vec![],
            warnings: vec![],
        }
    }
    #[test]
    fn letters_are_compatible_with_every_printable_class_and_aliases_ignore_case() {
        let b = bundle();
        b.validate().unwrap();
        assert_eq!(b.compatible_prints("1x1f").count(), 1);
        assert_eq!(b.compatible_prints("").count(), 0);
        assert_eq!(b.resolve("letters/a").unwrap().name, "A");
        assert!(b.resolve("0").is_none());
        let mut p = b.prints[0].clone();
        p.aspect = "2x2f".into();
        assert!(p.compatible("2x2f"));
        assert!(!p.compatible("2x2r"));
    }
    /// Older saves name stock prints by texture path or by a class since
    /// renamed; they resolve to the v20 print. Anything else stays unknown.
    #[test]
    fn older_saves_print_paths_and_renamed_classes_resolve_to_stock_prints() {
        for (token, alias) in [
            ("base/data/prints/Letters/A.png", Some("Letters/A")),
            (
                "Base/Data/Prints/letters/-space.PNG",
                Some("letters/-space"),
            ),
            ("base\\data\\prints\\Letters\\A.png", Some("Letters/A")),
            ("base/data/prints/2x2/blank.png", Some("2x2f/blank")),
            ("base/data/prints/2x1/letter1.png", Some("1x2f/letter1")),
            ("base/data/prints/1x1r/monitor1.png", Some("2x2r/monitor1")),
            ("2x1/vent", Some("1x2f/vent")),
            (
                "Add-Ons/Print_2x2f_Default/prints/arrow.png",
                Some("2x2f/arrow"),
            ),
            (
                "Add-Ons/Print_Letters_Default/prints/A.png",
                Some("Letters/A"),
            ),
            ("Letters/A", None),
            ("2x2f/arrow", None),
            ("NOPRINT", None),
            ("", None),
            ("base/data/prints/A.png", None),
            ("base/data/prints/Letters/sub/A.png", None),
            ("Add-Ons/Script_Thing/prints/A.png", None),
            (
                "base/data/prints/Letters/\u{e9}.png",
                Some("Letters/\u{e9}"),
            ),
        ] {
            assert_eq!(legacy_print_alias(token).as_deref(), alias, "{token}");
        }
        let b = bundle();
        let a = Some("print/print_letters_default/a");
        for token in [
            "base/data/prints/Letters/A.png",
            "Add-Ons/Print_Letters_Default/prints/a.png",
            "print/print_letters_default/a",
        ] {
            assert_eq!(b.resolve(token).map(|p| p.id.as_str()), a, "{token}");
        }
        for token in [
            "NOPRINT",
            "base/data/prints/Letters/B.png",
            "ModTer/brickRAMP",
        ] {
            assert!(b.resolve(token).is_none(), "{token}");
        }
    }
    #[test]
    fn rejects_paths_duplicate_aliases_and_changed_bytes() {
        for path in ["../a.png", "C:/a.png", "/a.png", "a\\b.png", "a/./b.png"] {
            assert!(!safe_relative(path));
        }
        let mut b = bundle();
        b.prints[0].aliases.push("letters/a".into());
        assert!(b.validate().is_err());
        let mut b = bundle();
        b.prints[0].diffuse.sha256 = "c".repeat(64);
        assert!(b.validate().is_err());
        let mut b = bundle();
        b.surfaces.remove("top");
        assert!(b.validate().is_err());
    }
}
