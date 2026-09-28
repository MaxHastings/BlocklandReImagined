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
    "v20", "bri", "base", "core", "engine", "game", "vanilla", "blockland", "server", "client",
    "local", "system", "admin", "package", "packages",
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

pub fn is_reserved(namespace: &str) -> bool {
    RESERVED_NAMESPACES.contains(&namespace)
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
            return Err(format!("kind `{kind}` may contain only a-z, 0-9, '_' and '-'"));
        }
        if name.is_empty()
            || name.len() > MAX_CONTENT_NAME
            || !name.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.' | '/')
            })
            || name.starts_with('/')
            || name.ends_with('/')
            || name.split('/').any(|segment| segment.is_empty() || segment.chars().all(|c| c == '.'))
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
            part.parse().map_err(|_| format!("`{text}` has a component that is too large"))
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
