//! Minimal, non-executing reader for static object literals in decompiled
//! TorqueScript/GUI files (`new Class(Name) { field = value; ... };`).
//!
//! Function, package and datablock bodies are skipped. Top-level
//! `if (!isObject(X)) { new ... }` guards are transparent, because v20 defines its
//! standard profiles that way. Nothing is ever evaluated.

use std::collections::BTreeMap;

/// First private-use code point used for `\cN` colour codes (N = 0..9).
pub const COLOR_CODE_BASE: u32 = 0xE000;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A quoted string literal, unescaped.
    Str(String),
    /// A bare numeric/identifier token or expression (kept verbatim).
    Bare(String),
}

impl Value {
    pub fn as_text(&self) -> &str {
        match self {
            Value::Str(s) | Value::Bare(s) => s,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Object {
    pub class: String,
    pub name: Option<String>,
    pub parent: Option<String>,
    pub line: u32,
    pub fields: BTreeMap<String, Value>,
    pub children: Vec<Object>,
}

impl Object {
    pub fn text(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(Value::as_text)
    }
    pub fn find(&self, name: &str) -> Option<&Object> {
        if self.name.as_deref() == Some(name) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }
}

/// Decode TorqueScript string escapes. `\cN` becomes U+E000+N; `\cr`, `\cp`
/// and `\co` become U+E00A..U+E00C.
pub fn unescape(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            match n {
                'n' => {
                    out.push('\n');
                    i += 2;
                    continue;
                }
                't' => {
                    out.push('\t');
                    i += 2;
                    continue;
                }
                'r' => {
                    out.push('\r');
                    i += 2;
                    continue;
                }
                '"' | '\\' | '\'' => {
                    out.push(n);
                    i += 2;
                    continue;
                }
                'c' if i + 2 < chars.len() => {
                    let k = chars[i + 2];
                    let code = match k {
                        '0'..='9' => Some(COLOR_CODE_BASE + (k as u32 - '0' as u32)),
                        'r' => Some(COLOR_CODE_BASE + 10),
                        'p' => Some(COLOR_CODE_BASE + 11),
                        'o' => Some(COLOR_CODE_BASE + 12),
                        _ => None,
                    };
                    if let Some(code) = code.and_then(char::from_u32) {
                        out.push(code);
                        i += 3;
                        continue;
                    }
                }
                'x' if i + 3 < chars.len() => {
                    let hex: String = chars[i + 2..i + 4].iter().collect();
                    if let Ok(v) = u8::from_str_radix(&hex, 16) {
                        out.push(v as char);
                        i += 4;
                        continue;
                    }
                }
                _ => {}
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn parse_value(raw: &str) -> Value {
    let v = raw.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        Value::Str(unescape(&v[1..v.len() - 1]))
    } else {
        Value::Bare(v.to_string())
    }
}

/// Net `{` minus `}` outside string literals.
fn brace_delta(line: &str) -> i32 {
    let (mut d, mut in_str, mut esc) = (0, false, false);
    for ch in line.chars() {
        if esc {
            esc = false;
            continue;
        }
        match ch {
            '\\' => esc = true,
            '"' => in_str = !in_str,
            '{' if !in_str => d += 1,
            '}' if !in_str => d -= 1,
            _ => {}
        }
    }
    d
}

/// Parse `new Class(args)` returning (class, args, opens_brace_on_line).
fn parse_new(line: &str) -> Option<(String, String, bool)> {
    let t = line.trim();
    let t = match t.find("new ") {
        Some(p) if t[..p].trim().is_empty() || t[..p].trim_end().ends_with('=') => &t[p + 4..],
        _ => return None,
    };
    let open = t.find('(')?;
    let class = t[..open].trim();
    if class.is_empty() || !class.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let close = t.rfind(')')?;
    let args = t[open + 1..close].trim().trim_matches('"').to_string();
    let rest = t[close + 1..].trim();
    if rest.starts_with('{') {
        Some((class.to_string(), args, true))
    } else if rest.is_empty() {
        Some((class.to_string(), args, false))
    } else {
        None // `new X();` statement or similar, not a literal body
    }
}

fn parse_field(line: &str) -> Option<(String, Value)> {
    let t = line.trim();
    let t = t.strip_suffix(';')?;
    let eq = t.find('=')?;
    let key = t[..eq].trim();
    let valid = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']');
    if !valid || t[eq + 1..].starts_with('=') {
        return None;
    }
    Some((key.to_string(), parse_value(&t[eq + 1..])))
}

/// Parse all static object literals in `text`.
pub fn parse_objects(text: &str) -> Vec<Object> {
    let mut roots = Vec::new();
    let mut stack: Vec<Object> = Vec::new();
    let mut pending: Option<Object> = None;
    let mut code_depth: i32 = 0;
    let mut awaiting_code_open = false;
    let mut guard_depth: i32 = 0;
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        let ln = idx as u32 + 1;
        if code_depth > 0 {
            code_depth += brace_delta(line);
            continue;
        }
        if awaiting_code_open {
            let d = brace_delta(line);
            if d > 0 {
                code_depth = d;
                awaiting_code_open = false;
            }
            continue;
        }
        if let Some(obj) = pending.take() {
            if line == "{" {
                stack.push(obj);
                continue;
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(obj),
                None => roots.push(obj),
            }
        }
        if let Some((class, args, opens)) = parse_new(raw) {
            let (name, parent) = match args.split_once(':') {
                Some((n, p)) => (n.trim().to_string(), Some(p.trim().to_string())),
                None => (args.clone(), None),
            };
            let obj = Object {
                class,
                name: (!name.is_empty()).then_some(name),
                parent,
                line: ln,
                ..Default::default()
            };
            if opens {
                stack.push(obj);
            } else {
                pending = Some(obj);
            }
            continue;
        }
        if !stack.is_empty() {
            if line == "};" || line == "}" {
                let obj = stack.pop().expect("non-empty");
                match stack.last_mut() {
                    Some(parent) => parent.children.push(obj),
                    None => roots.push(obj),
                }
                continue;
            }
            if let Some((k, v)) = parse_field(raw) {
                stack.last_mut().expect("non-empty").fields.insert(k, v);
            }
            continue;
        }
        if line.starts_with("function")
            || line.starts_with("package")
            || line.starts_with("datablock")
        {
            let d = brace_delta(line);
            if d > 0 {
                code_depth = d;
            } else {
                awaiting_code_open = true;
            }
            continue;
        }
        let is_guard =
            line.starts_with("if ") || line.starts_with("if(") || line.starts_with("else");
        if is_guard {
            guard_depth += brace_delta(line);
            continue;
        }
        if line == "{" {
            guard_depth += 1;
            continue;
        }
        if (line == "}" || line == "};") && guard_depth > 0 {
            guard_depth -= 1;
        }
    }
    roots
}

/// Top-level function bodies by name: (start line, lines).
pub fn functions(text: &str) -> BTreeMap<String, (u32, Vec<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if let Some(rest) = t.strip_prefix("function ")
            && let Some(p) = rest.find('(')
        {
            let name = rest[..p].trim().to_string();
            let start = i;
            let mut depth = 0;
            let mut opened = false;
            let mut j = i;
            while j < lines.len() {
                let d = brace_delta(lines[j]);
                if d != 0 || lines[j].contains('{') {
                    opened |= lines[j].contains('{');
                }
                depth += d;
                if opened && depth <= 0 {
                    break;
                }
                j += 1;
            }
            let body = lines[start..=j.min(lines.len() - 1)]
                .iter()
                .map(|s| s.to_string())
                .collect();
            out.insert(name, (start as u32 + 1, body));
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_literals_guards_and_skips_functions() {
        let src = r#"
new GuiControl(Root)
{
	extent = "640 480";
	new GuiBitmapButtonCtrl(Btn)
	{
		text = "Hi \c3there";
		command = "doIt(\"x\");";
		accelerator = 0;
	};
};
function foo()
{
	new GuiControl(Dynamic) { };
	if (x)
	{
	}
}
if (!isObject(GuiDefaultProfile))
{
	new GuiControlProfile(GuiDefaultProfile)
	{
		fontType = "Arial";
	};
}
new GuiControlProfile(Child : GuiDefaultProfile)
{
	fontSize = 18;
};
"#;
        let objs = parse_objects(src);
        let names: Vec<_> = objs.iter().map(|o| o.name.clone().unwrap()).collect();
        assert_eq!(names, ["Root", "GuiDefaultProfile", "Child"]);
        let btn = objs[0].find("Btn").unwrap();
        assert_eq!(btn.text("text"), Some("Hi \u{E003}there"));
        assert_eq!(btn.text("command"), Some("doIt(\"x\");"));
        assert_eq!(btn.fields["accelerator"], Value::Bare("0".into()));
        assert_eq!(objs[2].parent.as_deref(), Some("GuiDefaultProfile"));
        assert_eq!(objs[0].line, 2);
        let f = functions(src);
        assert_eq!(f["foo"].0, 12);
    }
}
