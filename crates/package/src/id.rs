//! Package namespaces, namespaced content ids and versions.
//!
//! A package's id is also the namespace of everything it declares:
//! package `creeper` owns `creeper:creature/creeper`. Content ids follow the
//! platform grammar `namespace:kind/name`. Namespaces the engine or the base
//! game own are reserved, so a package can neither masquerade as vanilla nor
//! overwrite it by declaring the same id.
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Namespaces no package may take: the base game, the engine, and names that
/// would read as either.
pub const RESERVED_NAMESPACES: &[&str] = &[
    "v20",
    "bri",
    "base",
    "core",
    "engine",
    "game",
    "vanilla",
    "blockland",
    "server",
    "client",
    "local",
    "system",
    "admin",
    "package",
    "packages",
];

pub const MAX_NAMESPACE: usize = 32;
pub const MAX_CONTENT_NAME: usize = 64;

/// Why a namespace is not acceptable, or None when it is.
pub fn namespace_problem(namespace: &str) -> Option<String> {
    if namespace.is_empty() || namespace.len() > MAX_NAMESPACE {
        return Some(format!("must be 1 to {MAX_NAMESPACE} characters"));
    }
    let mut chars = namespace.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase()) {
        return Some("must start with a lowercase letter a-z".into());
    }
    if !namespace
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Some("may contain only a-z, 0-9, '_' and '-'".into());
    }
    if namespace.ends_with('-') || namespace.ends_with('_') {
        return Some("must not end with '-' or '_'".into());
    }
    None
}

/// Reserved namespaces, and package ids built on one (`v20-weapons`), which
/// only the game itself ships.
pub fn is_reserved(namespace: &str) -> bool {
    RESERVED_NAMESPACES.contains(&namespace)
        || namespace
            .split_once('-')
            .is_some_and(|(head, _)| RESERVED_NAMESPACES.contains(&head))
}

/// The base game's content namespace.
pub const BASE_NAMESPACE: &str = "v20";

/// The namespace a package's content ids use. Every package owns its own id
/// as its namespace, except the base game's packages (`v20-weapons`,
/// `v20-map-bundle`, ...), which all declare into the shared `v20` namespace.
pub fn content_namespace(package_id: &str) -> &str {
    match package_id.split_once('-') {
        Some((BASE_NAMESPACE, _)) => BASE_NAMESPACE,
        _ => package_id,
    }
}

/// A parsed `namespace:kind/name` id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentId {
    pub namespace: String,
    pub kind: String,
    pub name: String,
}

impl ContentId {
    pub fn parse(id: &str) -> Result<Self, String> {
        let (namespace, rest) = id
            .split_once(':')
            .ok_or_else(|| format!("`{id}` is not `namespace:kind/name`"))?;
        let (kind, name) = rest
            .split_once('/')
            .ok_or_else(|| format!("`{id}` is not `namespace:kind/name`"))?;
        if let Some(problem) = namespace_problem(namespace) {
            return Err(format!("namespace `{namespace}` {problem}"));
        }
        if kind.is_empty()
            || !kind
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(format!(
                "kind `{kind}` may contain only a-z, 0-9, '_' and '-'"
            ));
        }
        if name.is_empty()
            || name.len() > MAX_CONTENT_NAME
            || !name.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.' | '/')
            })
            || name.starts_with('/')
            || name.ends_with('/')
            || name
                .split('/')
                .any(|segment| segment.is_empty() || segment.chars().all(|c| c == '.'))
        {
            return Err(format!(
                "name `{name}` must be 1 to {MAX_CONTENT_NAME} characters of a-z, 0-9, '_', '-', '.' and inner '/'"
            ));
        }
        Ok(Self {
            namespace: namespace.into(),
            kind: kind.into(),
            name: name.into(),
        })
    }
}

/// Mint an id for content converted from another format, whose names
/// (Torque datablock names, virtual file paths) do not follow the grammar:
/// lowercase, `\` becomes `/`, and every other character outside
/// `a-z 0-9 _ - . /` becomes `_` (`64x cube.blb` is `64x_cube.blb`,
/// `horsearmor::activate` is `horsearmor__activate`). Files referenced by
/// path use the kind `file` (`v20:file/add-ons/brick_large_cubes/64x_cube.blb`).
/// Use [`Minter`] when minting many ids, so two sources that map to the same
/// id are refused instead of silently merged.
pub fn native(namespace: &str, kind: &str, source: &str) -> Result<ContentId, String> {
    let name: String = source
        .trim()
        .chars()
        .map(|c| match c.to_ascii_lowercase() {
            '\\' => '/',
            c @ ('a'..='z' | '0'..='9' | '_' | '-' | '.' | '/') => c,
            _ => '_',
        })
        .collect();
    ContentId::parse(&format!("{namespace}:{kind}/{}", name.trim_matches('/')))
        .map_err(|problem| format!("cannot name `{source}` as {namespace}:{kind}: {problem}"))
}

/// Mints ids with [`native`] and refuses two different sources that map to
/// the same id (`Brick 2x2` and `brick_2x2`).
#[derive(Debug, Default)]
pub struct Minter {
    sources: std::collections::BTreeMap<ContentId, String>,
}

impl Minter {
    pub fn mint(&mut self, namespace: &str, kind: &str, source: &str) -> Result<ContentId, String> {
        let id = native(namespace, kind, source)?;
        match self.sources.get(&id) {
            Some(first) if first != source => Err(format!(
                "`{source}` and `{first}` would both be named {id}; rename one"
            )),
            Some(_) => Ok(id),
            None => {
                self.sources.insert(id.clone(), source.to_string());
                Ok(id)
            }
        }
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}/{}", self.namespace, self.kind, self.name)
    }
}

/// `major.minor.patch`, compared numerically. Pre-release tags are not
/// supported in v0; a package bumps its patch instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<&str> = text.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("`{text}` is not `major.minor.patch` (e.g. 1.0.0)"));
        }
        let number = |part: &str| -> Result<u32, String> {
            if part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.chars().all(|c| c.is_ascii_digit())
            {
                return Err(format!("`{text}` is not `major.minor.patch` (e.g. 1.0.0)"));
            }
            part.parse()
                .map_err(|_| format!("`{text}` has a component that is too large"))
        };
        Ok(Self {
            major: number(parts[0])?,
            minor: number(parts[1])?,
            patch: number(parts[2])?,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A dependency requirement: `*`, `=1.2.3`, `>=1.2.0`, or `^1.2.0` (same
/// major, at least this version). A bare `1.2.0` means `^1.2.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    Any,
    Exact(Version),
    AtLeast(Version),
    Compatible(Version),
}

impl Requirement {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text == "*" {
            return Ok(Self::Any);
        }
        let partial = |v: &str| -> Result<Version, String> {
            // Accept `2` and `2.1` as shorthand for `2.0.0` and `2.1.0`.
            let dots = v.matches('.').count();
            let full = match dots {
                0 => format!("{v}.0.0"),
                1 => format!("{v}.0"),
                _ => v.to_string(),
            };
            Version::parse(&full)
        };
        if let Some(v) = text.strip_prefix(">=") {
            Ok(Self::AtLeast(partial(v.trim())?))
        } else if let Some(v) = text.strip_prefix('=') {
            Ok(Self::Exact(partial(v.trim())?))
        } else if let Some(v) = text.strip_prefix('^') {
            Ok(Self::Compatible(partial(v.trim())?))
        } else {
            Ok(Self::Compatible(partial(text).map_err(|_| {
                format!("`{text}` is not a version requirement (`*`, `=1.0.0`, `>=1.0`, `^1.0`)")
            })?))
        }
    }

    pub fn matches(&self, version: Version) -> bool {
        match *self {
            Self::Any => true,
            Self::Exact(v) => version == v,
            Self::AtLeast(v) => version >= v,
            Self::Compatible(v) => version.major == v.major && version >= v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_are_lowercase_identifiers() {
        assert!(namespace_problem("creeper").is_none());
        assert!(namespace_problem("my-mod_2").is_none());
        assert!(namespace_problem("").is_some());
        assert!(namespace_problem("Creeper").is_some());
        assert!(namespace_problem("2cool").is_some());
        assert!(namespace_problem("a:b").is_some());
        assert!(namespace_problem("trailing-").is_some());
        assert!(namespace_problem(&"a".repeat(33)).is_some());
        assert!(is_reserved("v20") && is_reserved("bri") && !is_reserved("creeper"));
        assert!(is_reserved("v20-weapons") && !is_reserved("creeper-kit"));
        assert_eq!(content_namespace("v20-weapons"), "v20");
        assert_eq!(content_namespace("creeper-kit"), "creeper-kit");
    }

    #[test]
    fn content_ids_parse_and_print() {
        let id = ContentId::parse("creeper:creature/creeper").unwrap();
        assert_eq!(id.namespace, "creeper");
        assert_eq!(id.kind, "creature");
        assert_eq!(id.to_string(), "creeper:creature/creeper");
        assert!(ContentId::parse("creeper:asset/sounds/hiss.ogg").is_ok());
        assert!(ContentId::parse("creeper/creature").is_err());
        assert!(ContentId::parse("creeper:creature").is_err());
        assert!(ContentId::parse("creeper:creature/").is_err());
        assert!(ContentId::parse("creeper:creature/Big").is_err());
        assert!(ContentId::parse("creeper:creature/../x").is_err());
        assert!(ContentId::parse("creeper:creature/a//b").is_err());
    }

    #[test]
    fn converted_names_are_mapped_into_the_grammar() {
        let id = |kind, source| native("v20", kind, source).unwrap().to_string();
        assert_eq!(id("brick", "brick1x1Data"), "v20:brick/brick1x1data");
        assert_eq!(id("weapon", "GunItem"), "v20:weapon/gunitem");
        assert_eq!(
            id("file", "Add-Ons\\Brick_Large_Cubes\\64x Cube.blb"),
            "v20:file/add-ons/brick_large_cubes/64x_cube.blb"
        );
        assert_eq!(id("vehicle", "horsearmor::activate"), "v20:vehicle/horsearmor__activate");
        assert_eq!(
            id("sound", "rocketExplodeSound (alternate definition)"),
            "v20:sound/rocketexplodesound__alternate_definition_"
        );
        assert!(native("v20", "file", "a//b").is_err());
        assert!(native("v20", "file", &"x".repeat(65)).is_err());
        let mut minter = Minter::default();
        assert!(minter.mint("v20", "brick", "Brick 2x2").is_ok());
        assert!(minter.mint("v20", "brick", "Brick 2x2").is_ok(), "same source twice");
        let clash = minter.mint("v20", "brick", "brick_2x2").unwrap_err();
        assert!(clash.contains("Brick 2x2"), "{clash}");
        assert!(minter.mint("addon", "brick", "brick_2x2").is_ok(), "other namespace");
    }

    #[test]
    fn versions_and_requirements() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("1.10.0") > v("1.9.9"));
        assert!(Version::parse("1.0").is_err());
        assert!(Version::parse("01.0.0").is_err());
        assert!(Version::parse("1.0.0-beta").is_err());
        let r = |s| Requirement::parse(s).unwrap();
        assert!(r("*").matches(v("0.0.1")));
        assert!(r(">=2").matches(v("3.0.0")));
        assert!(!r(">=2").matches(v("1.9.9")));
        assert!(r("^1.2").matches(v("1.9.0")));
        assert!(!r("^1.2").matches(v("2.0.0")));
        assert!(!r("1.2.0").matches(v("1.1.0")));
        assert!(r("=1.2.3").matches(v("1.2.3")));
        assert!(!r("=1.2.3").matches(v("1.2.4")));
        assert!(Requirement::parse("~1").is_err());
    }
}
