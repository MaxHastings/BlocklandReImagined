//! Change a few values in a JSON file's text and keep the rest as written:
//! its key order, spacing and every value nobody changed. A modder's file
//! saved back from an in-game tool (the Tune window) then differs from
//! what they wrote only where a number changed.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

/// A value's place in the text, and what is inside it.
struct Node {
    start: usize,
    end: usize,
    kind: Kind,
}
enum Kind {
    /// Each member's key and value; `close` is the closing brace.
    Object {
        members: Vec<(String, Node)>,
        close: usize,
    },
    Array(Vec<Node>),
    Scalar,
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }
    fn byte(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }
    fn expect(&mut self, b: u8) -> Result<()> {
        self.space();
        ensure!(
            self.byte() == Some(b),
            "Expected `{}` at byte {}",
            b as char,
            self.at
        );
        self.at += 1;
        Ok(())
    }
    /// A string's end (after its closing quote).
    fn string_end(&mut self) -> Result<()> {
        ensure!(
            self.byte() == Some(b'"'),
            "Expected a string at byte {}",
            self.at
        );
        self.at += 1;
        while let Some(b) = self.byte() {
            self.at += 1;
            match b {
                b'\\' => self.at += 1,
                b'"' => return Ok(()),
                _ => {}
            }
        }
        bail!("A string never ends")
    }
    fn value(&mut self, depth: usize) -> Result<Node> {
        ensure!(depth < 128, "The JSON nests too deeply");
        self.space();
        let start = self.at;
        let kind = match self.byte().context("The JSON ends early")? {
            b'{' => {
                self.at += 1;
                let mut members = Vec::new();
                self.space();
                if self.byte() != Some(b'}') {
                    loop {
                        self.space();
                        let key_start = self.at;
                        self.string_end()?;
                        let key: String = serde_json::from_slice(&self.text[key_start..self.at])
                            .context("A key")?;
                        self.expect(b':')?;
                        members.push((key, self.value(depth + 1)?));
                        self.space();
                        match self.byte() {
                            Some(b',') => self.at += 1,
                            _ => break,
                        }
                    }
                }
                self.space();
                let close = self.at;
                self.expect(b'}')?;
                Kind::Object { members, close }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.byte() != Some(b']') {
                    loop {
                        items.push(self.value(depth + 1)?);
                        self.space();
                        match self.byte() {
                            Some(b',') => self.at += 1,
                            _ => break,
                        }
                    }
                }
                self.expect(b']')?;
                Kind::Array(items)
            }
            b'"' => {
                self.string_end()?;
                Kind::Scalar
            }
            _ => {
                while let Some(b) = self.byte() {
                    if matches!(b, b',' | b'}' | b']') || b.is_ascii_whitespace() {
                        break;
                    }
                    self.at += 1;
                }
                ensure!(self.at > start, "Expected a value at byte {start}");
                Kind::Scalar
            }
        };
        Ok(Node {
            start,
            end: self.at,
            kind,
        })
    }
}

/// The indent of the line holding byte `at`.
fn line_indent(text: &str, at: usize) -> &str {
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let rest = &text[line..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

/// `value` written as pretty JSON whose later lines sit at `indent`.
fn pretty(value: &Value, indent: &str) -> Result<String> {
    let text = serde_json::to_string_pretty(value)?;
    Ok(text.replace('\n', &format!("\n{indent}")))
}

/// Make `text` (a JSON document) read back as `new` by changing it only at
/// `paths` (each a list of keys; a number indexes an array): a value there
/// is replaced, and a key the text leaves out is added with `new`'s value.
/// Everything else keeps its text. Fails, changing nothing, when the
/// result would not read back as `new`.
pub fn set_paths(text: &str, new: &Value, paths: &[Vec<String>]) -> Result<String> {
    let mut parser = Parser {
        text: text.as_bytes(),
        at: 0,
    };
    let root = parser.value(0)?;
    parser.space();
    ensure!(parser.at == text.len(), "Text after the JSON value");
    // (start, end, replacement), never overlapping.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for path in paths {
        let mut node = &root;
        let mut value = new;
        let mut keys = path.iter().peekable();
        while let Some(key) = keys.next() {
            let next = match value {
                Value::Object(map) => map.get(key),
                Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
                _ => None,
            }
            .with_context(|| format!("`{}` is not in the new value", path.join(".")))?;
            let found = match &node.kind {
                Kind::Object { members, .. } => {
                    members.iter().find(|(k, _)| k == key).map(|m| &m.1)
                }
                Kind::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get(i)),
                Kind::Scalar => None,
            };
            match found {
                Some(child) => {
                    node = child;
                    value = next;
                }
                None => {
                    let edit = match &node.kind {
                        // The key is new here: add it, with all of its value.
                        Kind::Object { members, close } => {
                            let (at, lead, indent) = match members.last() {
                                Some((_, last)) => {
                                    let indent = line_indent(text, last.start);
                                    (last.end, format!(",\n{indent}"), indent.to_string())
                                }
                                None => {
                                    let outer = line_indent(text, node.start);
                                    let indent = format!("{outer}  ");
                                    (*close, format!("\n{indent}"), indent)
                                }
                            };
                            let tail = if members.is_empty() {
                                format!("\n{}", line_indent(text, node.start))
                            } else {
                                String::new()
                            };
                            let body = format!(
                                "{lead}{}: {}{tail}",
                                serde_json::to_string(key)?,
                                pretty(next, &indent)?
                            );
                            (at, at, body)
                        }
                        // Anything else is written whole.
                        _ => (
                            node.start,
                            node.end,
                            pretty(value, line_indent(text, node.start))?,
                        ),
                    };
                    edits.push(edit);
                    break;
                }
            }
            if keys.peek().is_none() {
                edits.push((
                    node.start,
                    node.end,
                    pretty(value, line_indent(text, node.start))?,
                ));
            }
        }
    }
    edits.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    edits.dedup();
    // An addition into the same place twice is one addition (two paths
    // under one new key); two different edits overlapping is not allowed.
    let mut out = String::with_capacity(text.len() + 64);
    let mut at = 0;
    for (start, end, replacement) in &edits {
        ensure!(*start >= at, "Two changes overlap in the text");
        out.push_str(&text[at..*start]);
        out.push_str(replacement);
        at = *end;
    }
    out.push_str(&text[at..]);
    let check: Value = serde_json::from_str(&out).context("The changed text")?;
    ensure!(
        check == *new,
        "The changed text would not read back as the new values"
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn path(p: &str) -> Vec<String> {
        p.split('.').map(str::to_owned).collect()
    }

    #[test]
    fn changed_numbers_keep_the_rest_of_the_file_as_written() {
        let text = "{\n  \"name\": \"Runner\",\n  \"movement\": {\n    \"forward\": 9,\n    \"jump\": 12.5\n  },\n  \"sockets\": {\"right_hand\": {\"offset\": [0.5, 0, 1]}}\n}\n";
        let mut new: Value = serde_json::from_str(text).unwrap();
        new["movement"]["forward"] = json!(20.0);
        new["sockets"]["right_hand"]["offset"][1] = json!(0.125);
        let out = set_paths(
            text,
            &new,
            &[
                path("movement.forward"),
                path("sockets.right_hand.offset.1"),
            ],
        )
        .unwrap();
        assert_eq!(
            out,
            "{\n  \"name\": \"Runner\",\n  \"movement\": {\n    \"forward\": 20.0,\n    \"jump\": 12.5\n  },\n  \"sockets\": {\"right_hand\": {\"offset\": [0.5, 0.125, 1]}}\n}\n"
        );
    }

    #[test]
    fn a_part_the_file_leaves_out_is_added_once_at_its_indent() {
        let text = "{\n  \"name\": \"Runner\",\n  \"look\": {}\n}";
        let mut new: Value = serde_json::from_str(text).unwrap();
        new["camera"] = json!({"bob": {"amplitude": 0.5, "frequency": 2.0}});
        new["look"]["scale"] = json!(1.5);
        let out = set_paths(
            text,
            &new,
            &[
                path("camera.bob.amplitude"),
                path("camera.bob.frequency"),
                path("look.scale"),
            ],
        )
        .unwrap();
        assert_eq!(
            out,
            "{\n  \"name\": \"Runner\",\n  \"look\": {\n    \"scale\": 1.5\n  },\n  \"camera\": {\n    \"bob\": {\n      \"amplitude\": 0.5,\n      \"frequency\": 2.0\n    }\n  }\n}"
        );
        let back: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(back, new);
    }

    #[test]
    fn a_change_that_would_not_read_back_is_refused() {
        let text = "{\"a\": 1, \"b\": 2}";
        // `b` changed too, but only `a` is named: the text cannot match.
        let new = json!({"a": 3, "b": 4});
        assert!(set_paths(text, &new, &[path("a")]).is_err());
        assert!(set_paths("{\"a\": 1} x", &new, &[path("a")]).is_err());
    }
}
