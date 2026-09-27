//! `Map_Tutorial` data that is not a brick save: the target practice script
//! `targetSetup.txt`, read the way `readTargetLine` walks it.
use anyhow::{Context, Result};
use bri_content::tutorial::TargetLaunch;

/// `readTargetLine` over `targetSetup.txt`: a blank line waits one second,
/// `timeout: <ms>` sets the gap after each launch, and `<row> <speed> [type]`
/// launches a target. Returns the launches and the end-of-file time.
pub fn target_schedule(text: &str) -> Result<(Vec<TargetLaunch>, u32)> {
    let mut at: u32 = 0;
    let mut gap: u32 = 1000;
    let mut targets = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let step = match words.as_slice() {
            [] => 1000,
            ["timeout:", ms, ..] => {
                gap = ms.parse().context("Invalid target timeout")?;
                gap
            }
            [row, rest @ ..] => {
                let speed = rest
                    .first()
                    .map_or(Ok(1), |s| s.parse())
                    .context("Invalid target speed")?;
                targets.push(TargetLaunch {
                    at_ms: at,
                    row: row.parse().context("Invalid target row")?,
                    speed,
                    kind: rest
                        .get(1)
                        .map_or_else(String::new, |k| k.to_ascii_lowercase()),
                });
                gap
            }
        };
        at = at.checked_add(step).context("Target schedule too long")?;
    }
    Ok((targets, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_schedule_follows_read_target_line() {
        let (targets, end) = target_schedule("1 1\n\n2 3 m2\ntimeout: 400\n3\n").unwrap();
        assert_eq!(
            targets,
            vec![
                TargetLaunch {
                    at_ms: 0,
                    row: 1,
                    speed: 1,
                    kind: String::new()
                },
                TargetLaunch {
                    at_ms: 2000,
                    row: 2,
                    speed: 3,
                    kind: "m2".into()
                },
                TargetLaunch {
                    at_ms: 3400,
                    row: 3,
                    speed: 1,
                    kind: String::new()
                },
            ]
        );
        assert_eq!(end, 3800);
    }
}
