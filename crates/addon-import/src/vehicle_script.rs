//! What a vehicle's `onAdd` script sets up that a native vehicle keeps as
//! data: which wheels steer and drive (`setWheelSteering`/`setWheelPowered`),
//! the animations its model plays (`playThread`, `setThreadDir`) and the
//! images it mounts (`mountImage`), each possibly chosen by the vehicle's
//! speed. Read, never run.
use bri_convert::tscript::{Function, Script};
use bri_vehicles::schema::AnimationThread;
use std::collections::BTreeMap;

/// An image the script mounts on the vehicle, within a speed range.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageMount {
    /// The image datablock, lower case.
    pub image: String,
    pub slot: u32,
    pub min_speed: Option<f32>,
    pub max_speed: Option<f32>,
}

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
    pub images: Vec<ImageMount>,
}

/// Reads `<datablock>::onAdd` and the script functions it hands the object to.
/// `fields` are the datablock's own (lower-case key, literal value), which a
/// speed test may compare against (`%obj.dataBlock.minContrailSpeed`).
pub fn setup(scripts: &[Script], datablock: &str, fields: &BTreeMap<String, String>) -> Setup {
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
    let mut images = vec![];
    for body in bodies {
        let s = compact(body);
        let spans = speed_spans(&s, fields);
        for (image, slot, range) in mounts(&s, &spans) {
            let mount = ImageMount {
                image,
                slot,
                min_speed: range.and_then(|r| r.0),
                max_speed: range.and_then(|r| r.1),
            };
            if !images.contains(&mount) {
                images.push(mount);
            }
        }
        for (slot, sequence, range) in plays(&s, &spans) {
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
    out.images = images;
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

type Span = (usize, usize, Range);

/// The statements an `if (%speed < n)`/`else` on the vehicle's speed guards,
/// with the speed range each runs in: `%speed` is assigned
/// `vectorLen(....getVelocity())`, and `n` is a number or one of the
/// datablock's own `fields` (`%obj.dataBlock.x`, `%obj.getDataBlock().x`,
/// `%this.x`).
fn speed_spans(s: &str, fields: &BTreeMap<String, String>) -> Vec<Span> {
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
    let threshold = |n: &str| -> Option<f32> {
        let n = match ["%obj.datablock.", "%obj.getdatablock().", "%this."]
            .iter()
            .find_map(|p| n.strip_prefix(p))
        {
            Some(key) => fields
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.trim().trim_matches('"'))?,
            None => n,
        };
        n.parse::<f32>().ok().filter(|n| n.is_finite() && *n >= 0.)
    };
    let mut spans: Vec<Span> = vec![];
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
            Some((threshold(n)?, below))
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
    spans
}

/// The speed range of the innermost span holding `at`, if any.
fn range_at(spans: &[Span], at: usize) -> Option<Range> {
    spans
        .iter()
        .filter(|(a, b, _)| (*a..*b).contains(&at))
        .min_by_key(|(a, b, _)| b - a)
        .map(|(_, _, r)| *r)
}

/// Every `mountImage(image, slot)` on the object, with its speed range.
fn mounts(s: &str, spans: &[Span]) -> Vec<(String, u32, Option<Range>)> {
    s.match_indices(".mountimage(")
        .filter_map(|(i, m)| {
            let end = close(s, i + m.len() - 1, '(', ')')?;
            let (image, slot) = s[i + m.len()..end - 1].split_once(',')?;
            let image = image.trim_matches('"');
            if image.is_empty() || !image.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return None;
            }
            let slot = slot.parse::<u32>().ok().filter(|s| *s < 4)?;
            Some((image.to_owned(), slot, range_at(spans, i)))
        })
        .collect()
}

/// Every `playThread(slot, sequence)` with its speed range.
fn plays(s: &str, spans: &[Span]) -> Vec<(u8, String, Option<Range>)> {
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
            Some((slot, sequence.to_owned(), range_at(spans, i)))
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
        let s = setup(&[script], "planevehicle", &BTreeMap::new());
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
    fn images_mounted_past_a_datablock_speed_become_speed_ranges() {
        // The Stunt Plane's stuntplane_Contrail.cs, trimmed.
        let script = bri_convert::tscript::read(
            r#"
function stuntplanevehicle::onadd(%this,%obj)
{
	parent::onadd(%this,%obj);
	contrailCheck(%obj);
}
function contrailCheck(%obj)
{
	if(!isObject(%obj))
		return;
	%speed = vectorLen(%obj.getVelocity());
	if(%speed < %obj.dataBlock.minContrailSpeed)
	{
		if(%obj.getMountedImage(3) !$= "")
		{
			%obj.unMountImage(2);
			%obj.unMountImage(3);
		}
	}
	else
	{
		if(%obj.getMountedImage(3) $= 0)
		{
			%obj.mountImage(contrailImage1,2);
			%obj.mountImage(contrailImage2,3);
		}
	}
	schedule(2000,0,"contrailCheck",%obj);
}
"#,
            "Add-Ons/Vehicle_Stunt_Plane/stuntplane_Contrail.cs",
        )
        .unwrap();
        let fields = BTreeMap::from([("mincontrailspeed".to_owned(), "30".to_owned())]);
        let s = setup(std::slice::from_ref(&script), "stuntplaneVehicle", &fields);
        let images: Vec<_> = s
            .images
            .iter()
            .map(|m| (m.image.as_str(), m.slot, m.min_speed, m.max_speed))
            .collect();
        assert_eq!(
            images,
            [
                ("contrailimage1", 2, Some(30.), None),
                ("contrailimage2", 3, Some(30.), None)
            ]
        );
        // A threshold field the datablock lacks guards nothing.
        let s = setup(&[script], "stuntplaneVehicle", &BTreeMap::new());
        assert!(s.images.iter().all(|m| m.min_speed.is_none()));
    }

    #[test]
    fn a_plain_thread_plays_always_and_set_thread_dir_reverses_it() {
        let s = compact("%obj.playThread(1, \"spin\"); %obj.setThreadDir(1, false);");
        assert_eq!(plays(&s, &[]), [(1, "spin".to_owned(), None)]);
        assert_eq!(reversed(&s), [1]);
    }
}
