//! UI data recovered from decompiled scripts. Values are read as literals and
//! the relevant script logic is re-implemented here with line references. No
//! script is ever executed.

use crate::torque::{functions, unescape};
use anyhow::{Context, Result, bail};
use bri_ui::schema::{
    BindAtom, ColorDivision, DefaultBind, Device, EventTables, InputEventDef, OutputEventDef,
    ParamSpec, RemapEntry,
};
use std::collections::BTreeMap;

/// Quoted string arguments of a call line, unescaped.
fn quoted_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_str, mut esc) = (false, false);
    for ch in line.chars() {
        if in_str {
            if esc {
                cur.push('\\');
                cur.push(ch);
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                out.push(unescape(&cur));
                cur.clear();
                in_str = false;
            } else {
                cur.push(ch);
            }
        } else if ch == '"' {
            in_str = true;
        }
    }
    out
}

fn words_f32(s: &str) -> Option<Vec<f32>> {
    s.split_whitespace()
        .map(|w| w.parse::<f32>().ok())
        .collect()
}

/// `setSprayCanColors()` default colorset, including the int/float decision
/// (allGameScripts: setSprayCanColors / setSprayCanColorI / setSprayCanColor).
pub fn brick_colorset(server_script: &str) -> Result<Vec<ColorDivision>> {
    let f = functions(server_script);
    let (_, body) = f
        .get("setSprayCanColors")
        .context("setSprayCanColors not found")?;
    let mut divisions = Vec::new();
    let mut current = Vec::new();
    for line in body {
        if !line.contains(".writeLine(") {
            continue;
        }
        let Some(arg) = quoted_args(line).into_iter().next() else {
            continue;
        };
        if let Some(name) = arg.strip_prefix("DIV:") {
            divisions.push(ColorDivision {
                name: name.to_string(),
                colors: std::mem::take(&mut current),
            });
        } else if !arg.is_empty() {
            let v = words_f32(&arg)
                .filter(|v| v.len() == 4)
                .with_context(|| format!("bad colour line {arg:?}"))?;
            let v: Vec<f32> = v.iter().map(|x| x.abs()).collect();
            let fractional = v.iter().any(|x| x.floor() != *x);
            let unit = v.iter().all(|x| *x <= 1.0);
            let c = if fractional || unit {
                [
                    v[0].clamp(0.0, 1.0),
                    v[1].clamp(0.0, 1.0),
                    v[2].clamp(0.0, 1.0),
                    v[3].clamp(1.0 / 255.0, 1.0),
                ]
            } else {
                [
                    v[0].clamp(0.0, 255.0) / 255.0,
                    v[1].clamp(0.0, 255.0) / 255.0,
                    v[2].clamp(0.0, 255.0) / 255.0,
                    v[3].clamp(1.0, 255.0) / 255.0,
                ]
            };
            current.push(c);
        }
    }
    if !current.is_empty() {
        divisions.push(ColorDivision {
            name: String::new(),
            colors: current,
        });
    }
    if divisions.is_empty() {
        bail!("no colorset lines found");
    }
    Ok(divisions)
}

/// `ColorSetGui::defaults` avatar colours (client script).
pub fn avatar_colors(client_script: &str) -> Result<Vec<[f32; 4]>> {
    let f = functions(client_script);
    let (_, body) = f
        .get("ColorSetGui::defaults")
        .context("ColorSetGui::defaults not found")?;
    let mut out = Vec::new();
    for line in body {
        if !line.contains("$Avatar::Color[") {
            continue;
        }
        let Some(arg) = quoted_args(line).into_iter().next() else {
            continue;
        };
        let v = words_f32(&arg)
            .filter(|v| v.len() == 4)
            .with_context(|| format!("bad avatar colour {arg:?}"))?;
        let c = if line.contains("IColorToFColor") {
            [v[0] / 255.0, v[1] / 255.0, v[2] / 255.0, v[3] / 255.0]
        } else {
            [v[0], v[1], v[2], v[3]]
        };
        out.push(c);
    }
    Ok(out)
}

/// Effective top-level `$...` assignments (last one wins), e.g. stock defaults.cs.
pub fn top_level_assignments(script: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut depth = 0i32;
    for line in script.lines() {
        let t = line.trim();
        if depth == 0
            && t.starts_with('$')
            && let Some(t) = t.strip_suffix(';')
            && let Some((k, v)) = t.split_once('=')
        {
            let v = v.trim();
            let v = if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
                unescape(&v[1..v.len() - 1])
            } else {
                v.to_string()
            };
            out.insert(k.trim().to_string(), v);
        }
        depth += t.matches('{').count() as i32 - t.matches('}').count() as i32;
    }
    out
}

/// `$Favorite::Brick<slot>_<i>` from stock defaults.
pub fn favorites(prefs: &BTreeMap<String, String>) -> BTreeMap<u8, Vec<String>> {
    let mut out: BTreeMap<u8, Vec<(u8, String)>> = BTreeMap::new();
    for (k, v) in prefs {
        let low = k.to_ascii_lowercase();
        if let Some(rest) = low.strip_prefix("$favorite::brick")
            && let Some((slot, idx)) = rest.split_once('_')
            && let (Ok(slot), Ok(idx)) = (slot.parse::<u8>(), idx.parse::<u8>())
        {
            out.entry(slot).or_default().push((idx, v.clone()));
        }
    }
    out.into_iter()
        .map(|(slot, mut v)| {
            v.sort();
            let mut names =
                vec![String::new(); v.iter().map(|(i, _)| *i as usize + 1).max().unwrap_or(0)];
            for (i, n) in v {
                names[i as usize] = n;
            }
            (slot, names)
        })
        .collect()
}

/// Options → Controls list: the `$RemapDivision/$RemapName/$RemapCmd` block.
pub fn remap_list(client_script: &str) -> Result<Vec<RemapEntry>> {
    let mut out: Vec<RemapEntry> = Vec::new();
    let mut division: Option<String> = None;
    let mut name: Option<String> = None;
    let mut started = false;
    for line in client_script.lines() {
        let t = line.trim();
        if t == "$RemapCount = 0;" {
            started = true;
            continue;
        }
        if !started {
            continue;
        }
        if t.starts_with("function ") {
            break;
        }
        if t.starts_with("$RemapDivision[") {
            division = quoted_args(t).into_iter().next();
        } else if t.starts_with("$RemapName[") {
            name = quoted_args(t).into_iter().next();
        } else if t.starts_with("$RemapCmd[") {
            let cmd = quoted_args(t).into_iter().next().context("remap cmd")?;
            out.push(RemapEntry {
                division: division.take(),
                name: name.take().context("remap name before cmd")?,
                command: cmd,
            });
        }
    }
    if out.is_empty() {
        bail!("remap list not found");
    }
    Ok(out)
}

fn atom(cond: &str) -> Option<BindAtom> {
    let c = cond.replace(' ', "");
    if c == "isWindows()" {
        return Some(BindAtom::Windows);
    }
    if let Some(n) = c.strip_prefix("%keyboard==") {
        return n.parse().ok().map(BindAtom::Keyboard);
    }
    if let Some(n) = c.strip_prefix("%mouse==") {
        return n.parse().ok().map(BindAtom::Mouse);
    }
    if c.starts_with("getBuildString()$=") {
        return Some(BindAtom::DebugBuild);
    }
    None
}

/// Parse `map.bind(device, key, command);` / `map.bindCmd(device, key, make, break);`.
fn parse_bind(line: &str) -> Option<(String, Device, String, String)> {
    let t = line.trim();
    let dot = t.find(".bind")?;
    let map = t[..dot].trim().to_string();
    let open = t.find('(')?;
    let inner = t[open + 1..].trim_end_matches(';').trim_end_matches(')');
    let is_cmd = t[dot..open].starts_with(".bindCmd");
    let parts: Vec<String> = split_args(inner);
    if parts.len() < 3 {
        return None;
    }
    let dev = parts[0].trim_matches('"').to_ascii_lowercase();
    let device = if dev.starts_with("mouse") {
        Device::Mouse
    } else {
        Device::Keyboard
    };
    let key = parts[1].trim().trim_matches('"').to_string();
    let command = if is_cmd {
        parts
            .get(3)
            .map(|s| unescape(s.trim().trim_matches('"')))
            .unwrap_or_default()
    } else {
        parts[2].trim().trim_matches('"').to_string()
    };
    Some((map, device, key, command))
}

fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_str, mut esc) = (false, false);
    for ch in s.chars() {
        if esc {
            cur.push(ch);
            esc = false;
            continue;
        }
        match ch {
            '\\' if in_str => {
                cur.push(ch);
                esc = true;
            }
            '"' => {
                in_str = !in_str;
                cur.push(ch);
            }
            ',' if !in_str => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Binds from `defaultControlsGui::apply` with their structured conditions.
pub fn default_binds(client_script: &str, warnings: &mut Vec<String>) -> Result<Vec<DefaultBind>> {
    let f = functions(client_script);
    let (start, body) = f
        .get("defaultControlsGui::apply")
        .context("defaultControlsGui::apply not found")?;
    let begin = body
        .iter()
        .position(|l| l.contains("new ActionMap(moveMap)"))
        .context("apply(): moveMap creation not found")?;
    let mut out = Vec::new();
    // Stack of active block conditions; chains of if/else-if per depth.
    let mut stack: Vec<Vec<(BindAtom, bool)>> = Vec::new();
    let mut chain: BTreeMap<usize, Vec<BindAtom>> = BTreeMap::new();
    let mut pending: Option<Vec<(BindAtom, bool)>> = None;
    let mut unknown: Option<String> = None;
    for (i, raw) in body.iter().enumerate().skip(begin + 1) {
        let line_no = start + i as u32;
        let t = raw.trim();
        let depth = stack.len();
        let (is_else, cond) = if let Some(c) = t
            .strip_prefix("else if (")
            .and_then(|c| c.strip_suffix(')'))
        {
            (true, Some(c))
        } else if let Some(c) = t.strip_prefix("if (").and_then(|c| c.strip_suffix(')')) {
            (false, Some(c))
        } else {
            (false, None)
        };
        if let Some(c) = cond {
            let Some(a) = atom(c) else {
                // Unknown conditions disable their block; binds inside are reported below.
                unknown = Some(format!("{c:?} (line {line_no})"));
                pending = Some(vec![(BindAtom::DebugBuild, true)]);
                continue;
            };
            let prior = if is_else {
                chain.get(&depth).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            let mut conds: Vec<(BindAtom, bool)> = prior.iter().map(|p| (*p, false)).collect();
            conds.push((a, true));
            let mut next = prior;
            next.push(a);
            chain.insert(depth, next);
            pending = Some(conds);
            continue;
        }
        if t == "else" {
            let prior = chain.get(&depth).cloned().unwrap_or_default();
            pending = Some(prior.iter().map(|p| (*p, false)).collect());
            chain.insert(depth, Vec::new());
            continue;
        }
        if t == "{" {
            stack.push(pending.take().unwrap_or_default());
            chain.insert(depth + 1, Vec::new());
            continue;
        }
        if t == "}" {
            stack.pop();
            continue;
        }
        if let Some((map, device, key, command)) = parse_bind(t)
            && map == "moveMap"
        {
            let mut when: Vec<(BindAtom, bool)> = stack.iter().flatten().copied().collect();
            if when.contains(&(BindAtom::DebugBuild, true))
                && let Some(u) = &unknown
            {
                warnings.push(format!(
                    "apply() line {line_no}: bind {key:?} under unknown condition {u} skipped"
                ));
                continue;
            }
            if let Some(p) = pending.take() {
                when.extend(p);
            }
            out.push(DefaultBind {
                device,
                key,
                command,
                when,
                source_line: line_no,
            });
        }
    }
    Ok(out)
}

/// Top-level `GlobalActionMap` binds (client script end).
pub fn global_binds(client_script: &str) -> Vec<DefaultBind> {
    let lines: Vec<&str> = client_script.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if !l.trim_start().starts_with("GlobalActionMap.bind") {
            continue;
        }
        let debug = (i.saturating_sub(3)..i).any(|j| lines[j].contains("getBuildString()"));
        if let Some((_, device, key, command)) = parse_bind(l) {
            out.push(DefaultBind {
                device,
                key,
                command,
                when: if debug {
                    vec![(BindAtom::DebugBuild, true)]
                } else {
                    vec![]
                },
                source_line: i as u32 + 1,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorset_follows_script_int_float_rule() {
        let src = "function setSprayCanColors()\n{\n\t%file.writeLine(\"0.900 0.000 0.000 1.000\");\n\t%file.writeLine(\"100 50 0 255\");\n\t%file.writeLine(\"DIV:Standard\");\n\t%file.writeLine(\"\");\n\t%file.writeLine(\"255 255 255 64\");\n\t%file.writeLine(\"DIV:Bold\");\n}\n";
        let c = brick_colorset(src).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "Standard");
        assert_eq!(c[0].colors[0], [0.9, 0.0, 0.0, 1.0]);
        assert!((c[0].colors[1][0] - 100.0 / 255.0).abs() < 1e-6);
        assert!((c[1].colors[0][3] - 64.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn binds_carry_nested_if_else_conditions() {
        let src = r#"function defaultControlsGui::apply()
{
	moveMap.delete();
	new ActionMap(moveMap);
	moveMap.bind(keyboard, "w", moveforward);
	if (isWindows())
	{
		moveMap.bind(keyboard, "ctrl z", undoBrick);
	}
	else
	{
		moveMap.bind(keyboard, "cmd z", undoBrick);
	}
	if (%keyboard == 0)
	{
		if (isWindows())
		{
			moveMap.bind(keyboard, "+", shiftBrickUp);
		}
	}
	else
	{
		moveMap.bind(keyboard, "i", shiftBrickAway);
	}
	if (%mouse == 0)
	{
		moveMap.bind(keyboard, "up", invUp);
	}
	else if (%mouse == 1)
	{
		moveMap.bind(mouse0, "button1", Jet);
	}
	moveMap.bindCmd(keyboard, "escape", "", "escapeMenu.toggle();");
}
"#;
        let mut w = Vec::new();
        let b = default_binds(src, &mut w).unwrap();
        assert!(w.is_empty(), "{w:?}");
        let find = |k: &str| b.iter().find(|x| x.key == k).unwrap();
        assert!(find("w").when.is_empty());
        assert_eq!(find("ctrl z").when, vec![(BindAtom::Windows, true)]);
        assert_eq!(find("cmd z").when, vec![(BindAtom::Windows, false)]);
        assert_eq!(
            find("+").when,
            vec![(BindAtom::Keyboard(0), true), (BindAtom::Windows, true)]
        );
        assert_eq!(find("i").when, vec![(BindAtom::Keyboard(0), false)]);
        assert_eq!(
            find("button1").when,
            vec![(BindAtom::Mouse(0), false), (BindAtom::Mouse(1), true)]
        );
        assert_eq!(find("button1").device, Device::Mouse);
        assert_eq!(find("escape").command, "escapeMenu.toggle();");
    }

    #[test]
    fn remap_and_favorites() {
        let src = "$RemapCount = 0;\n$RemapDivision[$RemapCount] = \"Movement\";\n$RemapName[$RemapCount] = \"Forward\";\n$RemapCmd[$RemapCount] = \"moveforward\";\n$RemapCount++;\n$RemapName[$RemapCount] = \"Backward\";\n$RemapCmd[$RemapCount] = \"movebackward\";\n$RemapCount++;\nfunction x()\n{\n}\n";
        let r = remap_list(src).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].division.as_deref(), Some("Movement"));
        assert_eq!(r[1].division, None);
        let prefs = top_level_assignments(
            "$Favorite::Brick1_1 = \"2x4\";\n$Favorite::Brick1_0 = \"32x32 Base\";\n",
        );
        assert_eq!(
            favorites(&prefs)[&1],
            vec!["32x32 Base".to_string(), "2x4".to_string()]
        );
    }
}

/// Split the argument list of the first call on a line and evaluate simple
/// string expressions (`"a" TAB "b"`, `"a" SPC "b"`, `"a" @ "b"`).
pub fn call_args(line: &str) -> Vec<String> {
    let Some(open) = line.find('(') else {
        return Vec::new();
    };
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let (mut in_str, mut esc) = (false, false);
    let mut raw_end = false;
    for ch in line[open + 1..].chars() {
        if in_str {
            cur.push(ch);
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_str = true;
                cur.push(ch);
            }
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' if depth == 0 => {
                raw_end = true;
                break;
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    if raw_end && !cur.trim().is_empty() {
        args.push(cur);
    }
    args.iter().map(|a| eval_string_expr(a.trim())).collect()
}

fn eval_string_expr(expr: &str) -> String {
    let mut out = String::new();
    let mut rest = expr.trim();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('"') {
            let mut end = None;
            let mut esc = false;
            for (i, ch) in r.char_indices() {
                if esc {
                    esc = false;
                } else if ch == '\\' {
                    esc = true;
                } else if ch == '"' {
                    end = Some(i);
                    break;
                }
            }
            let e = end.unwrap_or(r.len());
            out.push_str(&unescape(&r[..e]));
            rest = r[(e + 1).min(r.len())..].trim_start();
        } else {
            let tok_end = rest
                .find(|c: char| c.is_whitespace() || c == '"')
                .unwrap_or(rest.len());
            let tok = &rest[..tok_end];
            match tok {
                "TAB" => out.push('\t'),
                "SPC" => out.push(' '),
                "NL" => out.push('\n'),
                "@" => {}
                other => out.push_str(other),
            }
            rest = rest[tok_end..].trim_start();
        }
    }
    out
}

/// `registerInputEvent` / `registerOutputEvent` calls at top level, in order.
pub fn event_tables(server_script: &str) -> EventTables {
    let mut t = EventTables::default();
    for (i, line) in server_script.lines().enumerate() {
        let l = line.trim_start();
        let n = i as u32 + 1;
        if l.starts_with("registerInputEvent(") {
            let a = call_args(l);
            if a.len() >= 3 {
                let targets = a[2]
                    .split('\t')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| {
                        let mut w = s.split_whitespace();
                        (
                            w.next().unwrap_or("").to_string(),
                            w.next().unwrap_or("").to_string(),
                        )
                    })
                    .collect();
                t.inputs.push(InputEventDef {
                    class: a[0].clone(),
                    name: a[1].clone(),
                    targets,
                    source_line: n,
                });
            }
        } else if l.starts_with("registerOutputEvent(") {
            let a = call_args(l);
            if a.len() >= 2 {
                let params = a
                    .get(2)
                    .map(|p| {
                        p.split('\t')
                            .filter(|s| !s.trim().is_empty())
                            .map(ParamSpec::parse)
                            .collect()
                    })
                    .unwrap_or_default();
                let append_client = a.get(3).is_none_or(|v| v.trim() != "0");
                t.outputs.push(OutputEventDef {
                    class: a[0].clone(),
                    name: a[1].clone(),
                    params,
                    append_client,
                    source_line: n,
                });
            }
        }
    }
    t
}

/// Parse a part list (`hat.txt`): one name per non-empty line.
pub fn part_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .map(str::to_string)
        .collect()
}

/// `accent.txt`: first line = all accents, following = `<hat> <accents…>`.
pub fn accents(text: &str) -> (Vec<String>, BTreeMap<String, Vec<String>>) {
    let mut lines = part_list(text).into_iter();
    let all = lines
        .next()
        .map(|l| l.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let mut per = BTreeMap::new();
    for l in lines {
        let mut w = l.split_whitespace();
        if let Some(h) = w.next() {
            per.insert(h.to_ascii_lowercase(), w.map(str::to_string).collect());
        }
    }
    (all, per)
}

#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn parses_event_registrations() {
        let s = r#"registerInputEvent("fxDTSBrick", "onActivate", "Self fxDTSBrick" TAB "Player Player");
registerOutputEvent("fxDTSBrick", "setColor", "paintColor 0", 0);
registerOutputEvent("fxDTSBrick", "fakeKillBrick", "vector 200" TAB "int 0 300 5");
registerOutputEvent("GameConnection", "CenterPrint", "string 200 156" TAB "int 1 10 3");
registerOutputEvent("fxDTSBrick", "setEmitterDirection", "list Up 0 Down 1 North 2");
"#;
        let t = event_tables(s);
        assert_eq!(t.inputs.len(), 1);
        assert_eq!(t.inputs[0].targets[1], ("Player".into(), "Player".into()));
        assert_eq!(t.outputs.len(), 4);
        assert_eq!(
            t.outputs[0].params,
            vec![ParamSpec::PaintColor { default: 0 }]
        );
        assert!(!t.outputs[0].append_client);
        assert!(t.outputs[1].append_client);
        assert_eq!(
            t.outputs[1].params[1],
            ParamSpec::Int {
                min: 0,
                max: 300,
                default: 5
            }
        );
        assert_eq!(
            t.outputs[2].params[0],
            ParamSpec::String {
                max_length: 200,
                width: 156
            }
        );
        assert_eq!(
            t.outputs[3].params[0],
            ParamSpec::List {
                items: vec![("Up".into(), 0), ("Down".into(), 1), ("North".into(), 2)]
            }
        );
        let (all, per) = accents("//c\nnone plume visor\nhelmet none visor\n");
        assert_eq!(all.len(), 3);
        assert_eq!(per["helmet"], vec!["none".to_string(), "visor".to_string()]);
    }
}

/// Windows-1252, the help files' encoding (Torque read them as bytes).
fn cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9f => HIGH[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect()
}

/// A `.hfl` help page in the ML subset the UI draws. Margins, tab stops,
/// fonts and colours have no equivalent there and are dropped; links keep
/// their text but not their (long dead) blockland.us targets; tabs indent.
pub fn help_text(bytes: &[u8]) -> String {
    let src = cp1252(bytes).replace('\r', "");
    let mut out = String::new();
    let mut rest = src.as_str();
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let tag = tail[1..end].to_ascii_lowercase();
        if tag == "br" || tag.starts_with("just:") {
            out.push_str(&tail[..=end]);
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out.replace('\t', "    ").trim().to_string()
}

/// `HelpFileList.sortNumerical(0)`: by the page's leading number.
pub fn help_order(name: &str) -> (u32, String) {
    let number = name
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(u32::MAX);
    (number, name.to_ascii_lowercase())
}

#[cfg(test)]
mod help_tests {
    use super::*;

    #[test]
    fn help_pages_keep_text_and_drop_unsupported_markup() {
        let src = b"<lmargin%:3><font:Arial Bold:16>1. Select\n<lmargin%:10>Press <color:0000FF>B<color:000000> now.\n\t<a:blockland.us/x>Eric</a> \xe9\x93";
        assert_eq!(
            help_text(src),
            "1. Select\nPress B now.\n    Eric \u{e9}\u{201c}"
        );
        assert_eq!(help_text(b"<just:center>Hi<br>x"), "<just:center>Hi<br>x");
        let mut names = vec!["7. Loading", "10. Late", "0. Credits"];
        names.sort_by_key(|n| help_order(n));
        assert_eq!(names, ["0. Credits", "7. Loading", "10. Late"]);
    }
}
