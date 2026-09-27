//! Read-only TorqueScript text scanner.
//!
//! This is deliberately *not* an interpreter: it tokenises script text and
//! recognises declarations (`datablock`/`new` objects with literal fields),
//! literal global assignments and identifier/string occurrences with their
//! enclosing function, enclosing call and conditional context. Nothing is
//! evaluated or executed.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    /// `%local` or `$global` including the sigil.
    Var(String),
    Str(String),
    Tagged(String),
    Num(String),
    Punct(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
}

const PUNCT3: &[&str] = &["!$="];
const PUNCT2: &[&str] = &[
    "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=", "%=", "$=", "::", "<<",
    ">>", "|=", "&=", "^=",
];
const PUNCT1: &[&str] = &[
    "{", "}", "(", ")", "[", "]", ";", ",", ".", ":", "=", "+", "-", "*", "/", "%", "<", ">", "!",
    "?", "&", "|", "^", "~", "@", "$", "#",
];

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Tokenise TorqueScript. Unknown bytes are skipped (the scanner is lenient
/// because decompiled text is evidence, not input to a compiler).
pub fn lex(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    let n = chars.len();
    let starts = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if starts(i, "//") {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if starts(i, "/*") {
            i += 2;
            while i < n && !starts(i, "*/") {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let start_line = line;
            let mut s = String::new();
            i += 1;
            while i < n && chars[i] != quote {
                let ch = chars[i];
                if ch == '\\' && i + 1 < n {
                    let e = chars[i + 1];
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' | '"' | '\'' => s.push(e),
                        _ => {
                            // \c0, \x.. and friends: keep raw.
                            s.push('\\');
                            s.push(e);
                        }
                    }
                    i += 2;
                    continue;
                }
                if ch == '\n' {
                    line += 1;
                }
                s.push(ch);
                i += 1;
            }
            i += 1;
            out.push(Token {
                tok: if quote == '"' {
                    Tok::Str(s)
                } else {
                    Tok::Tagged(s)
                },
                line: start_line,
            });
            continue;
        }
        if (c == '%' || c == '$') && chars.get(i + 1).is_some_and(|&d| is_ident_start(d)) {
            let mut s = String::from(c);
            i += 1;
            loop {
                while i < n && is_ident(chars[i]) {
                    s.push(chars[i]);
                    i += 1;
                }
                if starts(i, "::") && chars.get(i + 2).is_some_and(|&d| is_ident_start(d)) {
                    s.push_str("::");
                    i += 2;
                } else {
                    break;
                }
            }
            out.push(Token {
                tok: Tok::Var(s),
                line,
            });
            continue;
        }
        if is_ident_start(c) {
            let mut s = String::new();
            loop {
                while i < n && is_ident(chars[i]) {
                    s.push(chars[i]);
                    i += 1;
                }
                if starts(i, "::") && chars.get(i + 2).is_some_and(|&d| is_ident_start(d)) {
                    s.push_str("::");
                    i += 2;
                } else {
                    break;
                }
            }
            out.push(Token {
                tok: Tok::Ident(s),
                line,
            });
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let mut s = String::new();
            if starts(i, "0x") || starts(i, "0X") {
                s.push_str("0x");
                i += 2;
                while i < n && chars[i].is_ascii_hexdigit() {
                    s.push(chars[i]);
                    i += 1;
                }
            } else {
                while i < n && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    s.push(chars[i]);
                    i += 1;
                }
                if i < n && (chars[i] == 'e' || chars[i] == 'E') {
                    let save = i;
                    let mut t = String::from(chars[i]);
                    i += 1;
                    if i < n && (chars[i] == '+' || chars[i] == '-') {
                        t.push(chars[i]);
                        i += 1;
                    }
                    if i < n && chars[i].is_ascii_digit() {
                        while i < n && chars[i].is_ascii_digit() {
                            t.push(chars[i]);
                            i += 1;
                        }
                        s.push_str(&t);
                    } else {
                        i = save;
                    }
                }
            }
            out.push(Token {
                tok: Tok::Num(s),
                line,
            });
            continue;
        }
        if let Some(p) = PUNCT3
            .iter()
            .chain(PUNCT2)
            .chain(PUNCT1)
            .find(|p| starts(i, p))
        {
            out.push(Token {
                tok: Tok::Punct(p),
                line,
            });
            i += p.chars().count();
            continue;
        }
        i += 1; // unknown character
    }
    out
}

/// A literal-ish field value.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Value {
    Str(String),
    Ident(String),
    Num(f64),
    Var(String),
    /// `a @ b @ ...` string concatenation of literals/variables.
    Concat(Vec<Value>),
    /// Anything else, as source text.
    Expr(String),
}

impl Value {
    /// Text form used for name matching (identifiers and plain strings).
    pub fn as_name(&self) -> Option<&str> {
        match self {
            Value::Ident(s) | Value::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(v) => Some(*v),
            Value::Str(s) | Value::Ident(s) => match s.to_ascii_lowercase().as_str() {
                "true" => Some(1.0),
                "false" => Some(0.0),
                other => other.trim().parse().ok(),
            },
            _ => None,
        }
    }
    pub fn raw(&self) -> String {
        match self {
            Value::Str(s) => format!("\"{s}\""),
            Value::Ident(s) | Value::Var(s) | Value::Expr(s) => s.clone(),
            Value::Num(v) => format!("{v}"),
            Value::Concat(parts) => parts.iter().map(Value::raw).collect::<Vec<_>>().join(" @ "),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Lower-cased name with index, e.g. `statesound[2]`.
    pub key: String,
    /// Name as written, e.g. `stateSound[2]`.
    pub name: String,
    pub value: Value,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// `datablock` or `new`.
    pub keyword: String,
    pub class: String,
    pub name: Option<String>,
    pub parent: Option<String>,
    pub fields: Vec<Field>,
    pub line: u32,
    /// Innermost-last enclosing contexts, e.g. `function foo`, `if (!isObject(x))`.
    pub context: Vec<String>,
    pub function: Option<String>,
}

impl Decl {
    pub fn field(&self, key: &str) -> Option<&Field> {
        // Last assignment wins, as in Torque.
        self.fields.iter().rev().find(|f| f.key == key)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalAssign {
    /// Lower-cased name including `$`.
    pub name: String,
    pub value: Value,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    /// As written.
    pub text: String,
    pub is_string: bool,
    pub line: u32,
    pub function: Option<String>,
    /// Innermost enclosing call's callee (`alxPlay`, `ServerPlay3D`, `playAudio`...).
    pub callee: Option<String>,
    /// Argument index within that call.
    pub arg_index: Option<usize>,
    /// Callee was invoked as a method (`%obj.playAudio(...)`).
    pub method_call: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ScanResult {
    pub decls: Vec<Decl>,
    pub globals: Vec<GlobalAssign>,
    /// Identifier/string occurrences outside declaration bodies matching the
    /// requested names (only filled when names are given).
    pub occurrences: Vec<Occurrence>,
    /// `function` definitions: (name, line).
    pub functions: Vec<(String, u32)>,
}

#[derive(Debug, Clone)]
enum FrameKind {
    Function(String),
    Package,
    If(String),
    Other,
}

struct CallFrame {
    callee: Option<String>,
    method: bool,
    args: usize,
}

fn tokens_text(toks: &[Token]) -> String {
    let mut s = String::new();
    for t in toks {
        let piece = match &t.tok {
            Tok::Ident(x) | Tok::Var(x) | Tok::Num(x) => x.clone(),
            Tok::Str(x) => format!("\"{x}\""),
            Tok::Tagged(x) => format!("'{x}'"),
            Tok::Punct(p) => (*p).to_string(),
        };
        let word = matches!(
            t.tok,
            Tok::Ident(_) | Tok::Var(_) | Tok::Num(_) | Tok::Str(_) | Tok::Tagged(_)
        );
        if word
            && s.chars()
                .last()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '"')
        {
            s.push(' ');
        }
        s.push_str(&piece);
    }
    s
}

fn is_punct(t: Option<&Token>, p: &str) -> bool {
    matches!(t, Some(Token { tok: Tok::Punct(x), .. }) if *x == p)
}

/// Index of the token matching the opener at `open` (`(`/`{`/`[`), or `toks.len()`.
fn matching(toks: &[Token], open: usize) -> usize {
    let (o, c) = match &toks[open].tok {
        Tok::Punct("(") => ("(", ")"),
        Tok::Punct("{") => ("{", "}"),
        Tok::Punct("[") => ("[", "]"),
        _ => return open,
    };
    let mut depth = 0i32;
    for (k, t) in toks.iter().enumerate().skip(open) {
        if let Tok::Punct(p) = &t.tok {
            if *p == o {
                depth += 1;
            } else if *p == c {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
        }
    }
    toks.len()
}

fn parse_value(toks: &[Token]) -> Value {
    match toks {
        [
            Token {
                tok: Tok::Str(s), ..
            },
        ]
        | [
            Token {
                tok: Tok::Tagged(s),
                ..
            },
        ] => Value::Str(s.clone()),
        [
            Token {
                tok: Tok::Ident(s), ..
            },
        ] => Value::Ident(s.clone()),
        [
            Token {
                tok: Tok::Var(s), ..
            },
        ] => Value::Var(s.clone()),
        [
            Token {
                tok: Tok::Num(s), ..
            },
        ] => s
            .parse()
            .map(Value::Num)
            .unwrap_or_else(|_| Value::Expr(s.clone())),
        [
            Token {
                tok: Tok::Punct("-"),
                ..
            },
            Token {
                tok: Tok::Num(s), ..
            },
        ] => s
            .parse::<f64>()
            .map(|v| Value::Num(-v))
            .unwrap_or_else(|_| Value::Expr(format!("-{s}"))),
        _ => {
            // a @ b @ c with simple operands
            let parts: Vec<&[Token]> = toks.split(|t| matches!(t.tok, Tok::Punct("@"))).collect();
            if parts.len() > 1 && parts.iter().all(|p| p.len() == 1) {
                let vals: Vec<Value> = parts.iter().map(|p| parse_value(p)).collect();
                if vals
                    .iter()
                    .all(|v| matches!(v, Value::Str(_) | Value::Var(_) | Value::Num(_)))
                {
                    return Value::Concat(vals);
                }
            }
            Value::Expr(tokens_text(toks))
        }
    }
}

/// Parse a declaration starting at `datablock`/`new` token `i`.
/// Returns declarations (the object and nested children) and the index after it.
fn parse_decl(toks: &[Token], i: usize, context: &[FrameKind]) -> Option<(Vec<Decl>, usize)> {
    let keyword = match &toks[i].tok {
        Tok::Ident(k) => k.to_ascii_lowercase(),
        _ => return None,
    };
    let Some(Token {
        tok: Tok::Ident(class),
        ..
    }) = toks.get(i + 1)
    else {
        return None;
    };
    if !is_punct(toks.get(i + 2), "(") {
        return None;
    }
    let close = matching(toks, i + 2);
    if close >= toks.len() {
        return None;
    }
    let inner = &toks[i + 3..close];
    let (name, parent) = match inner.iter().position(|t| matches!(t.tok, Tok::Punct(":"))) {
        Some(p) => (name_of(&inner[..p]), name_of(&inner[p + 1..])),
        None => (name_of(inner), None),
    };
    let ctx_strings: Vec<String> = context
        .iter()
        .filter_map(|f| match f {
            FrameKind::Function(n) => Some(format!("function {n}")),
            FrameKind::If(c) => Some(format!("if ({c})")),
            FrameKind::Package | FrameKind::Other => None,
        })
        .collect();
    let function = context.iter().rev().find_map(|f| match f {
        FrameKind::Function(n) => Some(n.clone()),
        _ => None,
    });
    let mut decl = Decl {
        keyword,
        class: class.clone(),
        name,
        parent,
        fields: Vec::new(),
        line: toks[i].line,
        context: ctx_strings,
        function,
    };
    let mut out = Vec::new();
    let mut j = close + 1;
    if is_punct(toks.get(j), "{") {
        let end = matching(toks, j);
        let mut k = j + 1;
        while k < end {
            match &toks[k].tok {
                Tok::Ident(kw)
                    if (kw.eq_ignore_ascii_case("new") || kw.eq_ignore_ascii_case("datablock")) =>
                {
                    if let Some((mut children, next)) = parse_decl(toks, k, context) {
                        out.append(&mut children);
                        k = next;
                        continue;
                    }
                    k += 1;
                }
                Tok::Ident(fname) => {
                    // field [ '[' index ']' ] '=' value ';'
                    let mut m = k + 1;
                    let mut key = fname.clone();
                    if is_punct(toks.get(m), "[") {
                        let e = matching(toks, m);
                        key.push('[');
                        key.push_str(&tokens_text(&toks[m + 1..e.min(end)]));
                        key.push(']');
                        m = e + 1;
                    }
                    if is_punct(toks.get(m), "=") {
                        let mut e = m + 1;
                        let mut depth = 0i32;
                        while e < end {
                            match &toks[e].tok {
                                Tok::Punct("(") | Tok::Punct("[") => depth += 1,
                                Tok::Punct(")") | Tok::Punct("]") => depth -= 1,
                                Tok::Punct(";") if depth <= 0 => break,
                                _ => {}
                            }
                            e += 1;
                        }
                        decl.fields.push(Field {
                            key: key.to_ascii_lowercase(),
                            name: key,
                            value: parse_value(&toks[m + 1..e]),
                            line: toks[k].line,
                        });
                        k = e + 1;
                    } else {
                        k = m.max(k + 1);
                    }
                }
                _ => k += 1,
            }
        }
        j = end + 1;
    }
    out.insert(0, decl);
    Some((out, j))
}

fn name_of(toks: &[Token]) -> Option<String> {
    match toks {
        [] => None,
        [
            Token {
                tok: Tok::Ident(s), ..
            },
        ]
        | [
            Token {
                tok: Tok::Str(s), ..
            },
        ] => Some(s.trim().to_string()),
        other => Some(tokens_text(other)),
    }
}

/// Scan one script. When `names` is given (lower-case), identifier and
/// string occurrences matching them outside declaration bodies are recorded.
pub fn scan(toks: &[Token], names: Option<&HashSet<String>>) -> ScanResult {
    let mut res = ScanResult::default();
    let mut frames: Vec<FrameKind> = Vec::new();
    let mut pending: Option<FrameKind> = None;
    let mut calls: Vec<CallFrame> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        match &t.tok {
            Tok::Ident(kw) if kw.eq_ignore_ascii_case("function") => {
                // function Name ( args ) {
                let mut name = String::new();
                let mut k = i + 1;
                while k < toks.len() && !is_punct(toks.get(k), "(") {
                    if let Tok::Ident(s) = &toks[k].tok {
                        name.push_str(s);
                    } else if let Tok::Punct(p) = &toks[k].tok {
                        name.push_str(p);
                    }
                    k += 1;
                }
                res.functions.push((name.clone(), t.line));
                let close = if k < toks.len() {
                    matching(toks, k)
                } else {
                    toks.len()
                };
                pending = Some(FrameKind::Function(name));
                i = close + 1;
                continue;
            }
            Tok::Ident(kw) if kw.eq_ignore_ascii_case("package") => {
                if let Some(Token {
                    tok: Tok::Ident(n), ..
                }) = toks.get(i + 1)
                {
                    let _ = n;
                    pending = Some(FrameKind::Package);
                    i += 2;
                    continue;
                }
            }
            Tok::Ident(kw)
                if kw.eq_ignore_ascii_case("datablock") || kw.eq_ignore_ascii_case("new") =>
            {
                let ctx: Vec<FrameKind> = frames.clone();
                if let Some((decls, next)) = parse_decl(toks, i, &ctx) {
                    res.decls.extend(decls);
                    i = next;
                    continue;
                }
            }
            Tok::Ident(kw) if kw.eq_ignore_ascii_case("if") && is_punct(toks.get(i + 1), "(") => {
                let close = matching(toks, i + 1);
                pending = Some(FrameKind::If(tokens_text(
                    &toks[i + 2..close.min(toks.len())],
                )));
            }
            Tok::Ident(kw)
                if ["else", "for", "while", "switch", "switch$", "do"]
                    .iter()
                    .any(|k| kw.eq_ignore_ascii_case(k)) =>
            {
                pending = Some(FrameKind::Other);
            }
            Tok::Punct("{") => {
                frames.push(pending.take().unwrap_or(FrameKind::Other));
            }
            Tok::Punct("}") => {
                frames.pop();
                pending = None;
            }
            Tok::Punct("(") => {
                let (callee, method) = match i.checked_sub(1).map(|k| &toks[k]) {
                    Some(Token {
                        tok: Tok::Ident(n), ..
                    }) => (Some(n.clone()), i >= 2 && is_punct(toks.get(i - 2), ".")),
                    _ => (None, false),
                };
                calls.push(CallFrame {
                    callee,
                    method,
                    args: 0,
                });
            }
            Tok::Punct(")") => {
                calls.pop();
            }
            Tok::Punct(",") => {
                if let Some(c) = calls.last_mut() {
                    c.args += 1;
                }
            }
            Tok::Punct(";") => pending = None,
            Tok::Var(v) if v.starts_with('$') && is_punct(toks.get(i + 1), "=") => {
                let mut e = i + 2;
                while e < toks.len() && !is_punct(toks.get(e), ";") {
                    e += 1;
                }
                res.globals.push(GlobalAssign {
                    name: v.to_ascii_lowercase(),
                    value: parse_value(&toks[i + 2..e.min(toks.len())]),
                    line: t.line,
                });
            }
            _ => {}
        }
        if let Some(names) = names {
            record(t, names, &frames, &calls, &mut res.occurrences);
        }
        i += 1;
    }
    res
}

fn record(
    t: &Token,
    names: &HashSet<String>,
    frames: &[FrameKind],
    calls: &[CallFrame],
    out: &mut Vec<Occurrence>,
) {
    let (text, is_string) = match &t.tok {
        Tok::Ident(s) => (s, false),
        Tok::Str(s) => (s, true),
        _ => return,
    };
    if !names.contains(&text.to_ascii_lowercase()) {
        return;
    }
    let function = frames.iter().rev().find_map(|f| match f {
        FrameKind::Function(n) => Some(n.clone()),
        _ => None,
    });
    let call = calls.last();
    out.push(Occurrence {
        text: text.clone(),
        is_string,
        line: t.line,
        function,
        callee: call.and_then(|c| c.callee.clone()),
        arg_index: call.map(|c| c.args),
        method_call: call.is_some_and(|c| c.method),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_torque_specifics() {
        let t = lex("if (%a $= \"x\" && $B::c !$= 'y') // c\n{ %v = 1.5e3; } /* x\n */ z");
        let kinds: Vec<_> = t.iter().map(|t| t.tok.clone()).collect();
        assert!(kinds.contains(&Tok::Punct("$=")));
        assert!(kinds.contains(&Tok::Punct("!$=")));
        assert!(kinds.contains(&Tok::Var("$B::c".into())));
        assert!(kinds.contains(&Tok::Tagged("y".into())));
        assert!(kinds.contains(&Tok::Num("1.5e3".into())));
        assert_eq!(t.last().unwrap().line, 3);
    }

    #[test]
    fn parses_datablocks_inheritance_and_context() {
        let src = r#"
$SimAudioType = 2;
datablock AudioProfile(Beep_A) { filename = "./a.wav"; description = AudioClosest3d; preload = false; };
datablock AudioProfile(Beep_B : Beep_A) { filename = "./b.wav"; };
if(!isObject(rocketLoopSound))
{
   datablock AudioProfile(rocketLoopSound) { filename = "./sound/rocketLoop.wav"; };
}
new AudioDescription(AudioGui) { volume = 1; type = $GuiAudioType; };
datablock ShapeBaseImageData(gunImage) { stateName[2] = "Fire"; stateSound[2] = gunShot1Sound; };
function foo(%x) { alxPlay(AudioError); %obj.playAudio(0, "Beep_A"); ServerPlay3D(Beep_B, %pos); }
"#;
        let toks = lex(src);
        let r = scan(&toks, None);
        assert_eq!(r.globals[0].name, "$simaudiotype");
        let b = r
            .decls
            .iter()
            .find(|d| d.name.as_deref() == Some("Beep_B"))
            .unwrap();
        assert_eq!(b.parent.as_deref(), Some("Beep_A"));
        assert!(b.context.is_empty());
        let rl = r
            .decls
            .iter()
            .find(|d| d.name.as_deref() == Some("rocketLoopSound"))
            .unwrap();
        assert!(!rl.context.is_empty());
        assert!(rl.context[0].contains("isObject"));
        let g = r
            .decls
            .iter()
            .find(|d| d.class == "ShapeBaseImageData")
            .unwrap();
        assert_eq!(
            g.field("statesound[2]").unwrap().value,
            Value::Ident("gunShot1Sound".into())
        );
        let d = r.decls.iter().find(|d| d.keyword == "new").unwrap();
        assert_eq!(
            d.field("type").unwrap().value,
            Value::Var("$GuiAudioType".into())
        );

        let names: HashSet<String> = ["audioerror", "beep_a", "beep_b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let r = scan(&toks, Some(&names));
        let occ: Vec<_> = r
            .occurrences
            .iter()
            .map(|o| {
                (
                    o.text.as_str(),
                    o.callee.as_deref(),
                    o.arg_index,
                    o.method_call,
                    o.function.as_deref(),
                )
            })
            .collect();
        assert!(occ.contains(&("AudioError", Some("alxPlay"), Some(0), false, Some("foo"))));
        assert!(occ.contains(&("Beep_A", Some("playAudio"), Some(1), true, Some("foo"))));
        assert!(occ.contains(&("Beep_B", Some("ServerPlay3D"), Some(0), false, Some("foo"))));
        // Declaration names/parents are not counted as occurrences.
        assert_eq!(occ.len(), 3);
    }
}
