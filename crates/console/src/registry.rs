//! Typed commands and cvars.
//!
//! A [`Registry`] is generic over the context its commands run against (the
//! UI core, the client app). Cvars are typed views of `$pref::` values in a
//! [`Store`], so the console and the Options menus change the same setting.
//! Commands a different layer owns are registered as *forwarded*: they are
//! listed, described and completed here, and [`Registry::exec`] hands their
//! statement back to the caller to deliver.
//!
//! Syntax: `name arg "quoted arg"`, statements separated by `;`. For people
//! with v20 habits, `name(a, b);` and `$pref::X = value;` also work.

use crate::log::{Level, Line};
use std::collections::BTreeMap;

/// Output of one [`Registry::exec`], appended to the log by the caller.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Output {
    pub lines: Vec<Line>,
}

impl Output {
    pub fn echo(&mut self, text: impl Into<String>) {
        self.push(Level::Normal, text.into());
    }
    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(Level::Warning, text.into());
    }
    pub fn error(&mut self, text: impl Into<String>) {
        self.push(Level::Error, text.into());
    }
    fn push(&mut self, level: Level, text: String) {
        self.lines.push(Line { level, text });
    }
    /// Write every line to the process-wide log.
    pub fn flush(self) {
        for line in self.lines {
            crate::log::print(line.level, &line.text);
        }
    }
}

/// Where cvars read and write their values.
pub trait Store {
    /// Current value (user override or stock default).
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    /// Every known `$pref::` name, for completion and `prefs` listings.
    fn keys(&self) -> Vec<String> {
        Vec::new()
    }
}

/// A command body. `args` excludes the command name.
pub type Run<C> = fn(&mut C, &[String], &mut Output) -> Result<(), String>;
/// Completion candidates for a command's first argument.
pub type Candidates<C> = fn(&C) -> Vec<String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInfo {
    pub name: String,
    /// Argument synopsis, e.g. `<address> [password]`.
    pub usage: String,
    pub help: String,
}

enum Handler<C> {
    Run(Run<C>),
    Forward,
    Help,
    Cvars,
}

pub struct Command<C> {
    pub info: CommandInfo,
    handler: Handler<C>,
    min_args: usize,
    complete: Option<Candidates<C>>,
    /// Arguments from this index on are secret.
    secret: Option<usize>,
}

impl<C> Command<C> {
    /// Print usage instead of running with fewer arguments.
    pub fn min_args(&mut self, n: usize) -> &mut Self {
        self.min_args = n;
        self
    }
    pub fn complete(&mut self, f: Candidates<C>) -> &mut Self {
        self.complete = Some(f);
        self
    }
    /// Arguments are secrets (passwords): [`Registry::redact`] hides them
    /// from the echoed input and callers keep the line out of history.
    pub fn secret(&mut self) -> &mut Self {
        self.secret_from(0)
    }
    /// Only arguments from index `n` on are secret (an optional password).
    pub fn secret_from(&mut self, n: usize) -> &mut Self {
        self.secret = Some(n);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Text,
}

impl Kind {
    /// Validate and normalise a typed value.
    pub fn parse(&self, value: &str) -> Result<String, String> {
        let v = value.trim();
        match *self {
            Kind::Bool => match v.to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "yes" => Ok("1".into()),
                "0" | "false" | "off" | "no" => Ok("0".into()),
                _ => Err("expected 1/0, on/off or true/false".into()),
            },
            Kind::Int { min, max } => {
                let n: i64 = v.parse().map_err(|_| "expected a whole number".to_string())?;
                if n < min || n > max {
                    return Err(format!("expected {min} to {max}"));
                }
                Ok(n.to_string())
            }
            Kind::Float { min, max } => {
                let n: f64 = v.parse().map_err(|_| "expected a number".to_string())?;
                if !n.is_finite() || n < min || n > max {
                    return Err(format!("expected {min} to {max}"));
                }
                Ok(n.to_string())
            }
            Kind::Text => Ok(v.to_string()),
        }
    }
    pub fn describe(&self) -> String {
        match *self {
            Kind::Bool => "bool".into(),
            Kind::Int { min, max } => format!("int {min}..{max}"),
            Kind::Float { min, max } => format!("float {min}..{max}"),
            Kind::Text => "text".into(),
        }
    }
}

/// A typed, named view of one `$pref::` value.
#[derive(Debug, Clone, PartialEq)]
pub struct Cvar {
    pub name: String,
    /// The `$pref::` it reads and writes.
    pub key: String,
    pub kind: Kind,
    pub help: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completion {
    /// The input line after completion.
    pub line: String,
    /// Every match when more than one remains (for listing).
    pub candidates: Vec<String>,
}

pub struct Registry<C> {
    commands: BTreeMap<String, Command<C>>,
    cvars: BTreeMap<String, Cvar>,
}

impl<C: Store> Default for Registry<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Store> Registry<C> {
    pub fn new() -> Self {
        let mut r = Registry {
            commands: BTreeMap::new(),
            cvars: BTreeMap::new(),
        };
        r.insert("help", "[name]", "List commands, or describe one command or cvar.", Handler::Help);
        r.insert("cvars", "[filter]", "List cvars and their values.", Handler::Cvars);
        r
    }

    fn insert(&mut self, name: &str, usage: &str, help: &str, handler: Handler<C>) -> &mut Command<C> {
        let key = name.to_ascii_lowercase();
        self.commands.insert(
            key.clone(),
            Command {
                info: CommandInfo {
                    name: name.into(),
                    usage: usage.into(),
                    help: help.into(),
                },
                handler,
                min_args: 0,
                complete: None,
                secret: None,
            },
        );
        self.commands.get_mut(&key).expect("just inserted")
    }

    /// Register (or replace) a command run in this registry's context.
    pub fn command(&mut self, name: &str, usage: &str, help: &str, run: Run<C>) -> &mut Command<C> {
        self.insert(name, usage, help, Handler::Run(run))
    }

    /// Register a command another layer runs; `exec` returns its statement.
    pub fn forward(&mut self, info: &CommandInfo) -> &mut Command<C> {
        self.insert(&info.name, &info.usage, &info.help, Handler::Forward)
    }

    pub fn cvar(&mut self, name: &str, key: &str, kind: Kind, help: &str) {
        self.cvars.insert(
            name.to_ascii_lowercase(),
            Cvar {
                name: name.into(),
                key: key.into(),
                kind,
                help: help.into(),
            },
        );
    }

    pub fn commands(&self) -> impl Iterator<Item = &CommandInfo> {
        self.commands.values().map(|c| &c.info)
    }

    pub fn cvars(&self) -> impl Iterator<Item = &Cvar> {
        self.cvars.values()
    }

    /// The line with secret commands' arguments replaced by `***`, and
    /// whether anything was hidden.
    pub fn redact(&self, line: &str) -> (String, bool) {
        let mut hidden = false;
        let parts: Vec<String> = split_statements(line)
            .into_iter()
            .map(|statement| {
                let tokens = tokenize(&statement);
                let from = tokens.first().and_then(|name| {
                    self.commands
                        .get(&name.to_ascii_lowercase())
                        .and_then(|c| c.secret)
                });
                match from {
                    Some(n) if tokens.len() > n + 1 => {
                        hidden = true;
                        let mut shown: Vec<String> = tokens[..=n].iter().map(|t| quote(t)).collect();
                        shown.push("***".into());
                        shown.join(" ")
                    }
                    _ => statement,
                }
            })
            .collect();
        if hidden { (parts.join("; "), true) } else { (line.to_string(), false) }
    }

    /// Run one input line. Returns the statements of forwarded commands, in
    /// order, for the caller to deliver to the layer that owns them.
    pub fn exec(&self, ctx: &mut C, line: &str, out: &mut Output) -> Vec<String> {
        let mut forwarded = Vec::new();
        for statement in split_statements(line) {
            let tokens = tokenize(&statement);
            let Some((name, args)) = tokens.split_first() else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            if let Some(cmd) = self.commands.get(&lower) {
                if args.len() < cmd.min_args {
                    out.error(format!("Usage: {}", synopsis(&cmd.info)));
                    continue;
                }
                match cmd.handler {
                    Handler::Run(run) => {
                        if let Err(e) = run(ctx, args, out) {
                            out.error(e);
                        }
                    }
                    Handler::Forward => forwarded.push(join(&tokens)),
                    Handler::Help => self.help(ctx, args.first(), out),
                    Handler::Cvars => self.list_cvars(ctx, args.first(), out),
                }
            } else if let Some(cvar) = self.cvars.get(&lower) {
                self.assign(ctx, &cvar.name, &cvar.key, Some(cvar.kind), args, out);
            } else if lower.starts_with("$pref::") {
                let kind = self
                    .cvars
                    .values()
                    .find(|c| c.key.eq_ignore_ascii_case(name))
                    .map(|c| c.kind);
                self.assign(ctx, name, name, kind, args, out);
            } else {
                out.error(format!("Unknown command: {name}. Type help for a list."));
            }
        }
        forwarded
    }

    fn assign(&self, ctx: &mut C, name: &str, key: &str, kind: Option<Kind>, args: &[String], out: &mut Output) {
        let args = match args.first() {
            Some(eq) if eq == "=" => &args[1..],
            _ => args,
        };
        if args.is_empty() {
            let value = ctx.get(key).unwrap_or_default();
            out.echo(format!("{name} = \"{value}\""));
            return;
        }
        let raw = args.join(" ");
        match kind.map_or(Ok(raw.clone()), |k| k.parse(&raw)) {
            Ok(value) => {
                ctx.set(key, &value);
                out.echo(format!("{name} = \"{value}\""));
            }
            Err(e) => out.error(format!("{name}: {e}")),
        }
    }

    fn help(&self, ctx: &C, name: Option<&String>, out: &mut Output) {
        let Some(name) = name else {
            out.echo("Commands:");
            for c in self.commands.values() {
                out.echo(format!("  {} - {}", synopsis(&c.info), c.info.help));
            }
            out.echo("Type cvars to list settings, or help <name> for one.");
            return;
        };
        let lower = name.to_ascii_lowercase();
        if let Some(c) = self.commands.get(&lower) {
            out.echo(synopsis(&c.info));
            out.echo(format!("  {}", c.info.help));
        } else if let Some(v) = self.cvars.get(&lower) {
            let value = ctx.get(&v.key).unwrap_or_default();
            out.echo(format!("{} = \"{}\" ({}, {})", v.name, value, v.kind.describe(), v.key));
            out.echo(format!("  {}", v.help));
        } else {
            out.error(format!("No command or cvar named {name}."));
        }
    }

    fn list_cvars(&self, ctx: &C, filter: Option<&String>, out: &mut Output) {
        let filter = filter.map(|f| f.to_ascii_lowercase());
        for v in self.cvars.values() {
            if filter
                .as_ref()
                .is_some_and(|f| !v.name.to_ascii_lowercase().contains(f.as_str()))
            {
                continue;
            }
            let value = ctx.get(&v.key).unwrap_or_default();
            out.echo(format!("  {} = \"{}\" - {}", v.name, value, v.help));
        }
    }

    /// Tab completion of the command/cvar name, or of a command's first
    /// argument when it offers candidates.
    pub fn complete(&self, ctx: &C, line: &str) -> Completion {
        let lead = line.len() - line.trim_start().len();
        let body = &line[lead..];
        let (prefix, partial, candidates): (String, &str, Vec<String>) =
            match body.find(char::is_whitespace) {
                None => {
                    let names = self.commands.values().map(|c| c.info.name.clone());
                    let cvars = self.cvars.values().map(|v| v.name.clone());
                    let mut all: Vec<String> = names.chain(cvars).collect();
                    if body.starts_with('$') {
                        all = ctx.keys();
                    }
                    (line[..lead].to_string(), body, all)
                }
                Some(end) => {
                    let name = &body[..end];
                    let rest = body[end..].trim_start();
                    let Some(f) = self
                        .commands
                        .get(&name.to_ascii_lowercase())
                        .and_then(|c| c.complete)
                    else {
                        return Completion {
                            line: line.into(),
                            candidates: Vec::new(),
                        };
                    };
                    // Only the first argument completes; a closed quote or a
                    // space outside quotes means it is already finished.
                    let quoted = rest.starts_with('"');
                    let finished = if quoted {
                        rest[1..].contains('"')
                    } else {
                        rest.contains(char::is_whitespace)
                    };
                    if finished {
                        return Completion {
                            line: line.into(),
                            candidates: Vec::new(),
                        };
                    }
                    let partial = rest.trim_start_matches('"');
                    (format!("{}{name} ", &line[..lead]), partial, f(ctx))
                }
            };
        let low = partial.to_ascii_lowercase();
        let mut matches: Vec<String> = candidates
            .into_iter()
            .filter(|c| c.to_ascii_lowercase().starts_with(&low))
            .collect();
        matches.sort_by_key(|c| c.to_ascii_lowercase());
        matches.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        match matches.len() {
            0 => Completion {
                line: line.into(),
                candidates: Vec::new(),
            },
            1 => Completion {
                line: format!("{prefix}{} ", quote(&matches[0])),
                candidates: Vec::new(),
            },
            _ => {
                let common = common_prefix(&matches);
                let line = if common.len() > partial.len() {
                    let open = if common.contains(char::is_whitespace) { "\"" } else { "" };
                    format!("{prefix}{open}{common}")
                } else {
                    line.into()
                };
                Completion {
                    line,
                    candidates: matches,
                }
            }
        }
    }
}

fn synopsis(info: &CommandInfo) -> String {
    if info.usage.is_empty() {
        info.name.clone()
    } else {
        format!("{} {}", info.name, info.usage)
    }
}

fn quote(s: &str) -> String {
    if s.is_empty() || s.contains(char::is_whitespace) || s.contains(';') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn join(tokens: &[String]) -> String {
    tokens.iter().map(|t| quote(t)).collect::<Vec<_>>().join(" ")
}

/// Case-insensitive common prefix, spelled as in the first candidate.
fn common_prefix(items: &[String]) -> String {
    let first = &items[0];
    let mut end = first.len();
    for other in &items[1..] {
        let n = first
            .char_indices()
            .zip(other.chars())
            .take_while(|((_, a), b)| a.eq_ignore_ascii_case(b))
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8());
        end = end.min(n);
    }
    first[..end].to_string()
}

/// Split on `;` outside double quotes.
pub fn split_statements(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            escaped = false;
        } else if c == '\\' && quoted {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            out.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(c);
    }
    out.push(cur);
    out.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Words, with `"..."` grouping (and `\"` inside). `name(a, "b c")` becomes
/// `name a "b c"`; `name=value` becomes `name = value`.
pub fn tokenize(statement: &str) -> Vec<String> {
    let s = statement.trim().trim_end_matches(';').trim_end();
    if let Some(open) = s.find('(')
        && s.ends_with(')')
        && !s[..open].trim().is_empty()
        && !s[..open].contains(|c: char| c.is_whitespace() || c == '"')
    {
        let mut tokens = vec![s[..open].trim().to_string()];
        let inner = &s[open + 1..s.len() - 1];
        for arg in split_outside_quotes(inner, ',') {
            let words = words(&arg);
            if !words.is_empty() {
                tokens.push(words.join(" "));
            }
        }
        return tokens;
    }
    let mut tokens = words(s);
    if let Some(first) = tokens.first().cloned()
        && !first.starts_with('"')
        && let Some((name, value)) = first.split_once('=')
        && !name.is_empty()
    {
        let mut split = vec![name.to_string(), "=".to_string()];
        if !value.is_empty() {
            split.push(value.to_string());
        }
        tokens.splice(0..1, split);
    }
    tokens
}

fn split_outside_quotes(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == sep && !quoted {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if quoted => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Ctx {
        prefs: BTreeMap<String, String>,
        ran: Vec<Vec<String>>,
        players: Vec<String>,
    }
    impl Store for Ctx {
        fn get(&self, key: &str) -> Option<String> {
            self.prefs.get(&key.to_ascii_lowercase()).cloned()
        }
        fn set(&mut self, key: &str, value: &str) {
            self.prefs.insert(key.to_ascii_lowercase(), value.into());
        }
        fn keys(&self) -> Vec<String> {
            vec!["$pref::Audio::masterVolume".into(), "$pref::Audio::PlayMusic".into()]
        }
    }

    fn registry() -> Registry<Ctx> {
        let mut r: Registry<Ctx> = Registry::new();
        r.command("kick", "<player>", "Kick a player.", |c, a, _| {
            c.ran.push(a.to_vec());
            Ok(())
        })
        .min_args(1)
        .complete(|c| c.players.clone());
        r.command("login", "<password>", "Log in.", |_, _, _| Ok(())).secret();
        r.command("join", "<address> [password]", "Join.", |_, _, _| Ok(())).secret_from(1);
        r.command("fail", "", "Always fails.", |_, _, _| Err("nope".into()));
        r.forward(&CommandInfo {
            name: "netstats".into(),
            usage: "".into(),
            help: "Host-side stats.".into(),
        });
        r.cvar("volume", "$pref::Audio::masterVolume", Kind::Float { min: 0.0, max: 1.0 }, "Master volume.");
        r.cvar("music", "$pref::Audio::PlayMusic", Kind::Bool, "Play music.");
        r
    }

    fn text(out: &Output) -> Vec<(Level, &str)> {
        out.lines.iter().map(|l| (l.level, l.text.as_str())).collect()
    }

    #[test]
    fn parses_plain_quoted_and_v20_call_syntax() {
        assert_eq!(tokenize(r#"kick "Blockhead 99" now"#), ["kick", "Blockhead 99", "now"]);
        assert_eq!(tokenize(r#"say "a \"b\"""#), ["say", r#"a "b""#]);
        assert_eq!(tokenize("quit();"), ["quit"]);
        assert_eq!(tokenize(r#"echo("hi there", 2)"#), ["echo", "hi there", "2"]);
        assert_eq!(tokenize("$pref::X=5"), ["$pref::X", "=", "5"]);
        assert_eq!(split_statements(r#"a; b "c;d" ;; e"#), ["a", r#"b "c;d""#, "e"]);
    }

    #[test]
    fn runs_commands_cvars_and_forwards() {
        let r = registry();
        let mut ctx = Ctx::default();
        let mut out = Output::default();
        let fwd = r.exec(&mut ctx, r#"KICK "Some One"; netstats x; volume 0.5; music = on"#, &mut out);
        assert_eq!(ctx.ran, [vec!["Some One".to_string()]]);
        assert_eq!(fwd, ["netstats x"]);
        assert_eq!(ctx.get("$pref::audio::mastervolume").as_deref(), Some("0.5"));
        assert_eq!(ctx.get("$pref::Audio::PlayMusic").as_deref(), Some("1"));
        assert_eq!(
            text(&out),
            [(Level::Normal, "volume = \"0.5\""), (Level::Normal, "music = \"1\"")]
        );
    }

    #[test]
    fn reports_usage_range_and_unknown_errors() {
        let r = registry();
        let mut ctx = Ctx::default();
        let mut out = Output::default();
        r.exec(&mut ctx, "kick; volume 2; music maybe; fail; bogus", &mut out);
        assert!(ctx.ran.is_empty());
        assert_eq!(ctx.get("$pref::Audio::masterVolume"), None);
        let got = text(&out);
        assert_eq!(got.len(), 5);
        assert!(got.iter().all(|(l, _)| *l == Level::Error));
        assert_eq!(got[0].1, "Usage: kick <player>");
        assert_eq!(got[1].1, "volume: expected 0 to 1");
        assert_eq!(got[3].1, "nope");
    }

    #[test]
    fn raw_prefs_use_the_cvar_type_when_one_is_bound() {
        let r = registry();
        let mut ctx = Ctx::default();
        let mut out = Output::default();
        r.exec(&mut ctx, "$pref::Audio::PlayMusic = off; $pref::Other::Thing = hello world", &mut out);
        assert_eq!(ctx.get("$pref::audio::playmusic").as_deref(), Some("0"));
        assert_eq!(ctx.get("$pref::other::thing").as_deref(), Some("hello world"));
        r.exec(&mut ctx, "$pref::Audio::PlayMusic banana; $pref::Other::Thing", &mut out);
        assert_eq!(ctx.get("$pref::audio::playmusic").as_deref(), Some("0"));
        assert_eq!(out.lines.last().unwrap().text, "$pref::Other::Thing = \"hello world\"");
    }

    #[test]
    fn redacts_secret_arguments() {
        let r = registry();
        assert_eq!(r.redact("login hunter2; kick x"), ("login ***; kick x".to_string(), true));
        assert_eq!(r.redact("kick x;"), ("kick x;".to_string(), false));
        assert_eq!(r.redact("join 1.2.3.4"), ("join 1.2.3.4".to_string(), false));
        assert_eq!(r.redact("join 1.2.3.4 pw"), ("join 1.2.3.4 ***".to_string(), true));
    }

    #[test]
    fn help_describes_commands_and_cvars() {
        let r = registry();
        let mut ctx = Ctx::default();
        ctx.set("$pref::Audio::masterVolume", "0.8");
        let mut out = Output::default();
        r.exec(&mut ctx, "help volume; help kick", &mut out);
        let got = text(&out);
        assert_eq!(got[0].1, "volume = \"0.8\" (float 0..1, $pref::Audio::masterVolume)");
        assert_eq!(got[2].1, "kick <player>");
        let mut out = Output::default();
        r.exec(&mut ctx, "help", &mut out);
        assert!(out.lines.iter().any(|l| l.text.contains("netstats")));
    }

    #[test]
    fn completes_names_arguments_and_prefs() {
        let r = registry();
        let ctx = Ctx {
            players: vec!["Blockhead".into(), "Block Party".into(), "Zed".into()],
            ..Ctx::default()
        };
        assert_eq!(r.complete(&ctx, "vol").line, "volume ");
        let c = r.complete(&ctx, "  he");
        assert_eq!(c.line, "  help ");
        let c = r.complete(&ctx, "kick b");
        assert_eq!(c.line, "kick Block");
        assert_eq!(c.candidates, ["Block Party", "Blockhead"]);
        assert_eq!(r.complete(&ctx, "kick blockh").line, "kick Blockhead ");
        assert_eq!(r.complete(&ctx, "kick \"block p").line, "kick \"Block Party\" ");
        assert_eq!(r.complete(&ctx, "kick Zed now").line, "kick Zed now");
        assert_eq!(r.complete(&ctx, "$pref::audio::pl").line, "$pref::Audio::PlayMusic ");
        let c = r.complete(&ctx, "zzz");
        assert_eq!((c.line.as_str(), c.candidates.len()), ("zzz", 0));
    }
}
