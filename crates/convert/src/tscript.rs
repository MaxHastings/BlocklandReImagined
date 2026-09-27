//! Static TorqueScript structure: datablocks, functions, packages and the
//! calls inside them, each with its source line. This reads scripts; it never
//! evaluates them. Expressions stay as their source text, so a consumer can
//! tell a literal from something only a script run would know.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;
const MAX_ITEMS: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    /// Virtual path, such as `Add-Ons/Weapon_Shotgun/server.cs`.
    pub path: String,
    pub sha256: String,
    pub datablocks: Vec<Datablock>,
    pub functions: Vec<Function>,
    /// Calls outside any function body, in source order (`exec`,
    /// `ForceRequiredAddOn`, `AddDamageType`, `activatePackage`, ...).
    pub calls: Vec<Call>,
    /// Top-level `$Name = value;` assignments.
    pub globals: Vec<Global>,
    /// `new Class(name) { ... }` objects created outside any function.
    pub objects: Vec<Call>,
    /// Structure this reader skipped, with lines.
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Datablock {
    pub class: String,
    pub name: String,
    pub parent: Option<String>,
    /// Lower-case field key (`statename[0]`) to its source expression.
    pub fields: BTreeMap<String, String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// `shotgunImage` in `shotgunImage::onFire`.
    pub namespace: Option<String>,
    pub name: String,
    pub params: Vec<String>,
    /// The `package` this definition sits in; a packaged function overrides
    /// the existing one while the package is active.
    pub package: Option<String>,
    pub line: usize,
    pub end_line: usize,
    pub calls: Vec<Call>,
    /// Body source text without the outer braces.
    pub body: String,
}

impl Function {
    pub fn qualified(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{ns}::{}", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// `exec`, `Parent::onCollision`, or for methods the method name.
    pub callee: String,
    /// `%obj` for `%obj.setVelocity(...)`; `new` for object creation, whose
    /// callee is then the class (or its source expression).
    pub receiver: Option<String>,
    /// Each argument's source text.
    pub args: Vec<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: String,
    pub value: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Atom,
    Str,
    Tag,
    Sym(char),
}

#[derive(Debug, Clone)]
struct Tok {
    kind: Kind,
    text: String,
    start: usize,
    end: usize,
}

impl Tok {
    fn is(&self, word: &str) -> bool {
        self.kind == Kind::Atom && self.text.eq_ignore_ascii_case(word)
    }
    fn sym(&self, c: char) -> bool {
        self.kind == Kind::Sym(c)
    }
}

fn lex(source: &str) -> Result<Vec<Tok>> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if c == b'"' || c == b'\'' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != c {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            ensure!(i < bytes.len(), "Unterminated string at byte {start}");
            i += 1;
            out.push(Tok {
                kind: if c == b'"' { Kind::Str } else { Kind::Tag },
                text: source[start + 1..i - 1].to_owned(),
                start,
                end: i,
            });
        } else if c.is_ascii_alphanumeric() || b"_$%".contains(&c) || c >= 0x80 {
            let start = i;
            i += 1;
            loop {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric()
                        || b"_$%.".contains(&bytes[i])
                        || bytes[i] >= 0x80)
                {
                    i += 1;
                }
                // Globals are namespaced with `::` (`$DamageType::Gun`).
                if c == b'$' && bytes.get(i) == Some(&b':') && bytes.get(i + 1) == Some(&b':') {
                    i += 2;
                } else {
                    break;
                }
            }
            out.push(Tok {
                kind: Kind::Atom,
                text: source[start..i].to_owned(),
                start,
                end: i,
            });
        } else {
            let ch = source[i..].chars().next().unwrap_or('?');
            out.push(Tok {
                kind: Kind::Sym(ch),
                text: ch.to_string(),
                start: i,
                end: i + ch.len_utf8(),
            });
            i += ch.len_utf8();
        }
        ensure!(out.len() <= 4 * 1024 * 1024, "Token budget exceeded");
    }
    Ok(out)
}

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Tok>,
    lines: Vec<usize>,
    at: usize,
    script: Script,
}

impl Parser<'_> {
    fn line(&self, byte: usize) -> usize {
        self.lines.partition_point(|&start| start <= byte)
    }
    fn tok(&self, offset: usize) -> Option<&Tok> {
        self.toks.get(self.at + offset)
    }
    fn text(&self, from: usize, to: usize) -> String {
        if from >= to || to > self.toks.len() {
            return String::new();
        }
        self.src[self.toks[from].start..self.toks[to - 1].end]
            .trim()
            .to_owned()
    }
    /// Index of the token closing the bracket opened at `open`.
    fn close(&self, open: usize) -> Option<usize> {
        let (a, b) = match self.toks.get(open)?.kind {
            Kind::Sym('(') => ('(', ')'),
            Kind::Sym('{') => ('{', '}'),
            Kind::Sym('[') => ('[', ']'),
            _ => return None,
        };
        let mut depth = 0usize;
        for (i, t) in self.toks.iter().enumerate().skip(open) {
            if t.sym(a) {
                depth += 1;
            } else if t.sym(b) {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }
    /// Arguments of the call whose `(` is at `open`, split at top-level commas.
    fn args(&self, open: usize, close: usize) -> Vec<String> {
        let mut args = vec![];
        let mut depth = 0i32;
        let mut start = open + 1;
        for i in open + 1..close {
            match self.toks[i].kind {
                Kind::Sym('(' | '[' | '{') => depth += 1,
                Kind::Sym(')' | ']' | '}') => depth -= 1,
                Kind::Sym(',') if depth == 0 => {
                    args.push(self.text(start, i));
                    start = i + 1;
                }
                _ => {}
            }
        }
        if close > start {
            args.push(self.text(start, close));
        }
        args
    }
    /// Calls and object creations between token indices `from..to`.
    fn calls(&self, from: usize, to: usize) -> (Vec<Call>, Vec<Call>) {
        let mut calls = vec![];
        let mut objects = vec![];
        let mut i = from;
        while i < to {
            let t = &self.toks[i];
            if t.is("new") {
                // `new Class(name) {...}` or `new (%expr)() {...}`.
                let (class, open) = match self.toks.get(i + 1) {
                    Some(c) if c.kind == Kind::Atom => (c.text.clone(), i + 2),
                    Some(c) if c.sym('(') => {
                        let close = self.close(i + 1).unwrap_or(i + 1);
                        (self.text(i + 1, close + 1), close + 1)
                    }
                    _ => (String::new(), i + 1),
                };
                let args = if self.toks.get(open).is_some_and(|t| t.sym('(')) {
                    self.close(open)
                        .map(|c| self.args(open, c))
                        .unwrap_or_default()
                } else {
                    vec![]
                };
                objects.push(Call {
                    callee: class,
                    receiver: Some("new".into()),
                    args,
                    line: self.line(t.start),
                });
                i = open;
                continue;
            }
            if t.kind == Kind::Atom
                && ![
                    "if", "for", "while", "switch", "switch$", "return", "function",
                ]
                .iter()
                .any(|k| t.is(k))
            {
                // `Ns::name(` spans atom ':' ':' atom.
                let (name, next) = if self.toks.get(i + 1).is_some_and(|x| x.sym(':'))
                    && self.toks.get(i + 2).is_some_and(|x| x.sym(':'))
                    && self.toks.get(i + 3).is_some_and(|x| x.kind == Kind::Atom)
                {
                    (format!("{}::{}", t.text, self.toks[i + 3].text), i + 4)
                } else {
                    (t.text.clone(), i + 1)
                };
                if self.toks.get(next).is_some_and(|x| x.sym('('))
                    && let Some(close) = self.close(next)
                {
                    let (receiver, callee) = match name.rsplit_once('.') {
                        Some((r, m)) if !name.contains("::") => (Some(r.to_owned()), m.to_owned()),
                        _ => (None, name),
                    };
                    calls.push(Call {
                        callee,
                        receiver,
                        args: self.args(next, close),
                        line: self.line(t.start),
                    });
                    i = next + 1;
                    continue;
                }
            }
            i += 1;
        }
        (calls, objects)
    }
    fn datablock(&mut self) -> Result<()> {
        let start = self.at;
        let line = self.line(self.toks[start].start);
        let atom = |t: Option<&Tok>| t.filter(|t| t.kind == Kind::Atom).map(|t| t.text.clone());
        let (Some(class), true) = (atom(self.tok(1)), self.tok(2).is_some_and(|t| t.sym('(')))
        else {
            self.skip(format!("line {line}: unreadable datablock header"));
            return Ok(());
        };
        let Some(name) = atom(self.tok(3)).filter(|n| !n.starts_with(['$', '%'])) else {
            self.skip(format!(
                "line {line}: datablock {class} without a literal name"
            ));
            return Ok(());
        };
        let mut at = self.at + 4;
        let parent = if self.toks.get(at).is_some_and(|t| t.sym(':')) {
            at += 2;
            atom(self.toks.get(at - 1))
        } else {
            None
        };
        if !self.toks.get(at).is_some_and(|t| t.sym(')'))
            || !self.toks.get(at + 1).is_some_and(|t| t.sym('{'))
        {
            self.skip(format!(
                "line {line}: datablock {name} header is not literal"
            ));
            return Ok(());
        }
        let open = at + 1;
        let Some(close) = self.close(open) else {
            self.skip(format!("line {line}: datablock {name} is not closed"));
            self.at = self.toks.len();
            return Ok(());
        };
        let mut fields = BTreeMap::new();
        let mut i = open + 1;
        while i < close {
            // key [ '[' index ']' ] '=' value ';'
            let mut j = i;
            while j < close && !self.toks[j].sym('=') && !self.toks[j].sym(';') {
                j += 1;
            }
            if j >= close || !self.toks[j].sym('=') {
                if j > i {
                    self.script.diagnostics.push(format!(
                        "line {}: datablock {name} statement without '=' skipped",
                        self.line(self.toks[i].start)
                    ));
                }
                i = j + 1;
                continue;
            }
            let key: String = self.toks[i..j]
                .iter()
                .map(|t| t.text.as_str())
                .collect::<String>()
                .to_ascii_lowercase();
            let mut k = j + 1;
            while k < close && !self.toks[k].sym(';') {
                k += 1;
            }
            fields.insert(key, self.text(j + 1, k));
            i = k + 1;
        }
        self.script.datablocks.push(Datablock {
            class,
            name,
            parent,
            fields,
            line,
        });
        self.at = close + 1;
        if self.tok(0).is_some_and(|t| t.sym(';')) {
            self.at += 1;
        }
        Ok(())
    }
    fn function(&mut self, package: Option<&str>) {
        let line = self.line(self.toks[self.at].start);
        let (namespace, name, open) = match (self.tok(1), self.tok(2), self.tok(3), self.tok(4)) {
            (Some(ns), Some(a), Some(b), Some(n))
                if a.sym(':') && b.sym(':') && n.kind == Kind::Atom =>
            {
                (Some(ns.text.clone()), n.text.clone(), self.at + 5)
            }
            (Some(n), _, _, _) if n.kind == Kind::Atom => (None, n.text.clone(), self.at + 2),
            _ => {
                self.skip(format!("line {line}: unreadable function header"));
                return;
            }
        };
        let Some(close) = self
            .toks
            .get(open)
            .filter(|t| t.sym('('))
            .and_then(|_| self.close(open))
        else {
            self.skip(format!(
                "line {line}: function {name} parameters unreadable"
            ));
            return;
        };
        let params = self.args(open, close);
        let Some(end) = self
            .toks
            .get(close + 1)
            .filter(|t| t.sym('{'))
            .and_then(|_| self.close(close + 1))
        else {
            self.skip(format!("line {line}: function {name} body unreadable"));
            return;
        };
        // Object creation inside a body is a call with receiver `new`.
        let (mut calls, objects) = self.calls(close + 2, end);
        calls.extend(objects);
        calls.sort_by_key(|c| c.line);
        let body = self.src[self.toks[close + 1].end..self.toks[end].start].to_owned();
        self.script.functions.push(Function {
            namespace,
            name,
            params,
            package: package.map(str::to_owned),
            line,
            end_line: self.line(self.toks[end].start),
            calls,
            body,
        });
        self.at = end + 1;
    }
    fn skip(&mut self, why: String) {
        self.script.diagnostics.push(why);
        self.at += 1;
    }
    fn run(&mut self) -> Result<()> {
        let mut plain_from = 0;
        while self.at < self.toks.len() {
            ensure!(
                self.script.datablocks.len() + self.script.functions.len() <= MAX_ITEMS,
                "Definition budget exceeded"
            );
            let t = &self.toks[self.at];
            if t.is("datablock") {
                self.flush(plain_from, self.at);
                self.datablock()?;
                plain_from = self.at;
            } else if t.is("function") {
                self.flush(plain_from, self.at);
                self.function(None);
                plain_from = self.at;
            } else if t.is("package")
                && self.tok(1).is_some_and(|t| t.kind == Kind::Atom)
                && self.tok(2).is_some_and(|t| t.sym('{'))
            {
                self.flush(plain_from, self.at);
                let package = self.toks[self.at + 1].text.clone();
                let close = self.close(self.at + 2).unwrap_or(self.toks.len());
                self.at += 3;
                while self.at < close {
                    if self.toks[self.at].is("function") {
                        self.function(Some(&package));
                    } else {
                        self.at += 1;
                    }
                }
                self.at = close + 1;
                plain_from = self.at;
            } else if t.kind == Kind::Atom
                && t.text.starts_with('$')
                && self.tok(1).is_some_and(|t| t.sym('='))
                && !self.tok(2).is_some_and(|t| t.sym('='))
            {
                let line = self.line(t.start);
                let name = t.text.clone();
                let mut end = self.at + 2;
                while end < self.toks.len() && !self.toks[end].sym(';') {
                    end += 1;
                }
                let value = self.text(self.at + 2, end);
                self.script.globals.push(Global { name, value, line });
                self.at = end + 1;
            } else {
                self.at += 1;
            }
        }
        self.flush(plain_from, self.toks.len());
        Ok(())
    }
    fn flush(&mut self, from: usize, to: usize) {
        let to = to.min(self.toks.len());
        if from < to {
            let (calls, objects) = self.calls(from, to);
            self.script.calls.extend(calls);
            self.script.objects.extend(objects);
        }
    }
}

pub fn read(source: &str, path: &str) -> Result<Script> {
    ensure!(source.len() <= MAX_SCRIPT_BYTES, "Script too large: {path}");
    let lines = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let mut parser = Parser {
        src: source,
        toks: lex(source)?,
        lines,
        at: 0,
        script: Script {
            path: path.to_owned(),
            sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
            datablocks: vec![],
            functions: vec![],
            calls: vec![],
            globals: vec![],
            objects: vec![],
            diagnostics: vec![],
        },
    };
    parser.run()?;
    Ok(parser.script)
}

/// Strips one layer of `"..."` from a field value; everything else is kept.
pub fn literal(value: &str) -> &str {
    let v = value.trim();
    v.strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
%error = ForceRequiredAddOn("Weapon_Gun");
if(%error == $Error::AddOn_NotFound)
   error("missing");
else
   exec("./Weapon_Thing.cs"); // loads the rest
$Thing::Count = 3;
datablock ProjectileData(thingProjectile : gunProjectile)
{
   directDamage = 45; // comment ; with semicolon
   stateName[0] = "Activate";
   colorShiftColor = thingItem.colorShiftColor;
};
function thingImage::onFire(%this, %obj, %slot)
{
    for(%i = 0; %i < 3; %i++)
    {
        %p = new (%this.projectileType)() { dataBlock = %this.projectile; };
        MissionCleanup.add(%p);
    }
    %obj.setVelocity(VectorAdd(%obj.getVelocity(), "0 0 1"));
    return %p;
}
package thingPackage
{
    function armor::onCollision(%this, %obj, %col)
    {
        Parent::onCollision(%this, %obj, %col);
    }
};
activatePackage(thingPackage);
"#;

    #[test]
    fn reads_structure_with_lines() {
        let s = read(SAMPLE, "Add-Ons/Weapon_Thing/server.cs").unwrap();
        let callees: Vec<_> = s.calls.iter().map(|c| c.callee.as_str()).collect();
        assert_eq!(
            callees,
            ["ForceRequiredAddOn", "error", "exec", "activatePackage"]
        );
        assert_eq!(s.calls[0].args, ["\"Weapon_Gun\""]);
        assert_eq!(s.calls[2].line, 6);
        assert_eq!(s.globals[0].name, "$Thing::Count");
        let d = &s.datablocks[0];
        assert_eq!(
            (d.class.as_str(), d.name.as_str()),
            ("ProjectileData", "thingProjectile")
        );
        assert_eq!(d.parent.as_deref(), Some("gunProjectile"));
        assert_eq!(d.line, 8);
        assert_eq!(d.fields["directdamage"], "45");
        assert_eq!(d.fields["statename[0]"], "\"Activate\"");
        assert_eq!(d.fields["colorshiftcolor"], "thingItem.colorShiftColor");
        let f = &s.functions[0];
        assert_eq!(f.qualified(), "thingImage::onFire");
        assert_eq!((f.line, f.end_line), (14, 23));
        assert_eq!(f.params, ["%this", "%obj", "%slot"]);
        let names: Vec<_> = f.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(names.contains(&"setVelocity") && names.contains(&"add"));
        let set = f.calls.iter().find(|c| c.callee == "setVelocity").unwrap();
        assert_eq!(set.receiver.as_deref(), Some("%obj"));
        assert!(f.body.contains("new (%this.projectileType)()"));
        let packaged = &s.functions[1];
        assert_eq!(packaged.package.as_deref(), Some("thingPackage"));
        assert_eq!(packaged.qualified(), "armor::onCollision");
        assert_eq!(packaged.calls[0].callee, "Parent::onCollision");
        assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    }

    #[test]
    fn broken_scripts_degrade_instead_of_failing() {
        let s = read(
            "datablock ItemData($name) { a = 1; };\nfunction x( {",
            "t.cs",
        )
        .unwrap();
        assert!(s.datablocks.is_empty());
        assert_eq!(s.diagnostics.len(), 2, "{:?}", s.diagnostics);
        assert!(read("x = \"open", "t.cs").is_err());
        assert_eq!(literal(" \"./a.dts\" "), "./a.dts");
    }
}
