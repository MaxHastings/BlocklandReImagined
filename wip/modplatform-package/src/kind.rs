//! The content kinds this game build consumes from packages. A kind is only
//! listed once a real system reads it, so `check` can say precisely when a
//! package declares something nothing will use.
use crate::archive::Side;

#[derive(Debug, Clone, Copy)]
pub struct KindSpec {
    pub kind: &'static str,
    /// Where the declared file must live: client-visible data, or the
    /// server-only `server/` directory.
    pub side: Side,
    /// Allowed file extensions (lowercase, without the dot).
    pub extensions: &'static [&'static str],
    /// The system that reads it, for `check`'s interpretation report.
    pub consumer: &'static str,
}

pub const KINDS: &[KindSpec] = &[
    KindSpec {
        kind: "asset",
        side: Side::Client,
        extensions: crate::archive::DATA_EXTENSIONS,
        consumer: "package cache: synced to every joining client and addressable by id",
    },
    KindSpec {
        kind: "behaviour",
        side: Side::Server,
        extensions: &["luau"],
        consumer: "behaviour host: sandboxed server script with declared capabilities and budgets",
    },
];

pub fn find(kind: &str) -> Option<&'static KindSpec> {
    KINDS.iter().find(|spec| spec.kind == kind)
}

pub fn names() -> Vec<&'static str> {
    KINDS.iter().map(|spec| spec.kind).collect()
}
