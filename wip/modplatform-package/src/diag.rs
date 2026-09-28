//! Machine-readable diagnostics. Every problem a package can have is reported
//! as a [`Diagnostic`] with a stable `code` an agent can match on, the place it
//! was found, a readable message and, where one exists, the fix.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable dotted code, e.g. `manifest.id.reserved`. Codes are never
    /// reused for a different meaning.
    pub code: String,
    /// Where: a package-relative file, optionally with a JSON pointer
    /// (`package.json#/provides/0/id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub message: String,
    /// How to fix it, when there is a known fix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, code, message)
    }
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, code, message)
    }
    pub fn info(code: &str, message: impl Into<String>) -> Self {
        Self::new(Severity::Info, code, message)
    }
    fn new(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: code.into(),
            location: None,
            message: message.into(),
            hint: None,
        }
    }
    pub fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        write!(f, "{severity}[{}]", self.code)?;
        if let Some(location) = &self.location {
            write!(f, " {location}")?;
        }
        write!(f, ": {}", self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " (fix: {hint})")?;
        }
        Ok(())
    }
}

/// An ordered list of diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl Diagnostics {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }
    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        self.0.extend(other);
    }
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter().filter(|d| d.severity == Severity::Error)
    }
    pub fn codes(&self) -> Vec<&str> {
        self.0.iter().map(|d| d.code.as_str()).collect()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Turn errors into one `anyhow` error listing each on its own line.
    pub fn into_result(self) -> anyhow::Result<Self> {
        if self.has_errors() {
            let lines: Vec<String> = self.errors().map(ToString::to_string).collect();
            anyhow::bail!("{}", lines.join("\n"));
        }
        Ok(self)
    }
}

/// Error carrying structured diagnostics through `anyhow`, so callers that
/// print JSON can recover the codes (`error.downcast_ref::<Rejected>()`).
#[derive(Debug, Clone)]
pub struct Rejected(pub Diagnostics);

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lines: Vec<String> = self.0.errors().map(ToString::to_string).collect();
        f.write_str(&lines.join("\n"))
    }
}

impl std::error::Error for Rejected {}
