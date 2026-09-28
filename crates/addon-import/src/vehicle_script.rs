//! What a vehicle's `onAdd` script sets up that a native vehicle keeps as
//! data: which wheels steer and drive (`setWheelSteering`/`setWheelPowered`)
//! and the animations its model plays (`playThread`, `setThreadDir`),
//! including a choice of sequence by the vehicle's speed. Read, never run.
use bri_convert::tscript::{Function, Script};
use bri_vehicles::schema::AnimationThread;

#[derive(Debug, Default)]
pub struct Setup {
    /// The datablock has an `onAdd`.
    pub found: bool,
    /// `onAdd` calls `Parent::onAdd`, so v20's wheel table applies first.
    pub calls_parent: bool,
    /// Per wheel index, the steering and power `onAdd` sets.
    pub steering: Vec<(usize, f32)>,
    pub powered: Vec<(usize, bool)>,
    pub threads: Vec<AnimationThread>,
}

/// Reads `<datablock>::onAdd` and the script functions it hands the object to.
pub fn setup(scripts: &[Script], datablock: &str) -> Setup {
    let functions: Vec<&Function> = scripts.iter().flat_map(|s| &s.functions).collect();
    let Some(on_add) = functions.iter().find(|f| {
        f.name.eq_ignore_ascii_case("onAdd")
            && f.namespace
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(datablock))
    }) else {
        return Setup::default();
    };
    let mut out = Setup {
        found: true,
        ..Setup::default()
    };
    for c in &on_add.calls {
        let callee = c.callee.to_ascii_lowercase();
        if callee == "parent::onadd" {
            out.calls_parent = true;
        }
        let (Some(index), Some(value)) = (
            c.args.first().and_then(|a| a.trim().parse::<usize>().ok()),
            c.args
                .get(1)
                .map(|a| a.trim().trim_matches('"').to_ascii_lowercase()),
        ) else {
            continue;
        };
        match callee.as_str() {
            "setwheelsteering" => {
                if let Ok(v) = value.parse::<f32>() {
                    out.steering.push((index, v.clamp(-1., 1.)));
                }
            }
            "setwheelpowered" => {
                out.powered.push((
                    index,
                    value == "true" || value.parse::<f32>().is_ok_and(|n| n != 0.),
                ));
            }
            _ => {}
        }
    }
    // onAdd, then each plain function it calls with the object.
    let mut bodies = vec![on_add.body.as_str()];
    for c in &on_add.calls {
        if c.receiver.is_none()
            && !c.callee.contains("::")
            && c.args.iter().any(|a| a.trim().eq_ignore_ascii_case("%obj"))
            && let Some(f) = functions
                .iter()
                .find(|f| f.namespace.is_none() && f.name.eq_ignore_ascii_case(&c.callee))
        {
            bodies.push(f.body.as_str());
        }
    }
    let mut conditioned = vec![];
    let mut plain = vec![];
    let mut backwards = vec![];
    for body in bodies {
        let s = compact(body);
        for (slot, sequence, range) in plays(&s) {
            let thread = AnimationThread {
                slot,
                sequence,
                rate: 1.,
                min_speed: range.and_then(|r| r.0),
                max_speed: range.and_then(|r| r.1),
            };
            if range.is_some() {
                conditioned.push(thread);
            } else {
                plain.push(thread);
            }
        }
        backwards.extend(reversed(&s));
    }
    // A slot switched by speed replaces what onAdd started there at once,
    // where v20 waits for the script's first check.
    plain.retain(|p| !conditioned.iter().any(|c| c.slot == p.slot));
    let mut threads = conditioned;
    for p in plain {
        if let Some(i) = threads.iter().position(|t| t.slot == p.slot) {
            threads[i] = p;
        } else {
            threads.push(p);
        }
    }
    for t in &mut threads {
        if backwards.contains(&t.slot) {
            t.rate = -1.;
        }
    }
    threads.dedup_by(|a, b| {
        a.slot == b.slot
            && a.sequence == b.sequence
            && a.min_speed == b.min_speed
            && a.max_speed == b.max_speed
    });
    out.threads = threads;
    out
}

/// Lower-case source without comments or whitespace.
fn compact(body: &str) -> String {
    body.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .flat_map(str::chars)
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The end (exclusive) of the bracketed group opening at `at`.
fn close(s: &str, at: usize, open: char, shut: char) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s[at..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == shut {
            depth -= 1;
            if depth == 0 {
                return Some(at + i + 1);
            }
        }
    }
    None
}

/// The `{...}` block or single statement starting at `at`.
fn statement(s: &str, at: usize) -> Option<(usize, usize)> {
    if s[at..].starts_with('{') {
        Some((at, close(s, at, '{', '}')?))
    } else {
        Some((at, at + s[at..].find(';')? + 1))
    }
}

type Range = (Option<f32>, Option<f32>);

/// Every `playThread(slot, sequence)` with the speed range the enclosing
/// `if (%speed < n)`/`else` gives it, `%speed` being assigned
/// `vectorLen(....getVelocity())`.
fn plays(s: &str) -> Vec<(u8, String, Option<Range>)> {
    let speeds: Vec<&str> = s
        .match_indices("=vectorlen(")
        .filter_map(|(i, _)| {
            let var_start = s[..i]
                .rfind([';', '{', '}'])
                .map_or(0, |p| p + 1);
            let rest = &s[i..];
            let call_end = close(s, i + "=vectorlen".len(), '(', ')')?;
            (rest.starts_with("=vectorlen(") && s[i..call_end].contains(".getvelocity()"))
                .then(|| &s[var_start..i])
                .filter(|v| v.starts_with('%'))
        })
        .collect();
    let mut spans: Vec<(usize, usize, Range)> = vec![];
    for (i, _) in s.match_indices("if(") {
        if i > 0 && s[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let Some(cond_end) = close(s, i + 2, '(', ')') else {
            continue;
        };
        let cond = &s[i + 3..cond_end - 1];
        let Some(range) = speeds.iter().find_map(|v| {
            let rest = cond.strip_prefix(v)?;
            let (below, n) = if let Some(n) = rest.strip_prefix("<=").or(rest.strip_prefix('<')) {
                (true, n)
            } else if let Some(n) = rest.strip_prefix(">=").or(rest.strip_prefix('>')) {
                (false, n)
            } else {
                return None;
            };
            let n = n
                .parse::<f32>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.)?;
            Some(if below { (n, true) } else { (n, false) })
        }) else {
            continue;
        };
        let (n, below) = range;
        let Some((a0, a1)) = statement(s, cond_end) else {
            continue;
        };
        let (then, otherwise) = if below {
            ((None, Some(n)), (Some(n), None))
        } else {
            ((Some(n), None), (None, Some(n)))
        };
        spans.push((a0, a1, then));
        if s[a1..].starts_with("else")
            && let Some((b0, b1)) = statement(s, a1 + 4)
        {
            spans.push((b0, b1, otherwise));
        }
    }
    s.match_indices("playthread(")
        .filter_map(|(i, m)| {
            let end = close(s, i + m.len() - 1, '(', ')')?;
            let args = &s[i + m.len()..end - 1];
            let (slot, sequence) = args.split_once(',')?;
            let slot = slot.parse::<u8>().ok().filter(|s| *s < 4)?;
            let sequence = sequence.trim_matches('"');
            if sequence.is_empty() || !sequence.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            let range = spans
                .iter()
                .filter(|(a, b, _)| (*a..*b).contains(&i))
                .min_by_key(|(a, b, _)| b - a)
                .map(|(_, _, r)| *r);
            Some((slot, sequence.to_owned(), range))
        })
        .collect()
}

/// Slots `setThreadDir(slot, false)` turns backwards.
fn reversed(s: &str) -> Vec<u8> {
    s.match_indices("setthreaddir(")
        .filter_map(|(i, m)| {
            let args = &s[i + m.len()..];
            let args = &args[..args.find(')')?];
            let (slot, forward) = args.split_once(',')?;
            matches!(forward, "false" | "0")
                .then(|| slot.parse::<u8>().ok())
                .flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_speed_switch_becomes_speed_ranges_and_wheels_are_read() {
        let script = bri_convert::tscript::read(
            r#"
function PlaneVehicle::onAdd(%this, %obj)
{
   Parent::onAdd(%this, %obj);
   %obj.playThread(0, "propfast");
   %obj.setWheelSteering(0, 1);
   %obj.setWheelPowered(2, true);
   spinCheck(%obj);
}
function spinCheck(%obj)
{
   %speed = vectorLen(%obj.getVelocity());
   if(%speed < 5)
   {
      %obj.playThread(0, propslow); // idle
   }
   else
      %obj.playThread(0, propfast);
   schedule(2000, 0, "spinCheck", %obj);
}
"#,
            "Add-Ons/Vehicle_Test/server.cs",
        )
        .unwrap();
        let s = setup(&[script], "planevehicle");
        assert!(s.found && s.calls_parent);
        assert_eq!(s.steering, [(0, 1.)]);
        assert_eq!(s.powered, [(2, true)]);
        let t: Vec<_> = s
            .threads
            .iter()
            .map(|t| (t.slot, t.sequence.as_str(), t.min_speed, t.max_speed))
            .collect();
        assert_eq!(
            t,
            [
                (0, "propslow", None, Some(5.)),
                (0, "propfast", Some(5.), None)
            ]
        );
    }

    #[test]
    fn a_plain_thread_plays_always_and_set_thread_dir_reverses_it() {
        let s = compact("%obj.playThread(1, \"spin\"); %obj.setThreadDir(1, false);");
        assert_eq!(plays(&s), [(1, "spin".to_owned(), None)]);
        assert_eq!(reversed(&s), [1]);
    }
}
