//! The behaviour share report: what share of bot time each kind of
//! activity gets, against a band per kind (`tests/data/behaviour_bands.json`).
//! Nothing dormant, nothing dominant: a kind the chooser offered but that
//! stays under its band's floor is DORMANT, one over its ceiling DOMINANT.
//!
//! Kinds are read from the brain's own readout (`BotThought`), not new
//! instrumentation: the behaviour in effect, `goof` while a flavour
//! interrupt runs, `vehicle` while mounted; and, per other choice point,
//! its option in effect (`aim:feet`, `route:left`). A kind is *offered*
//! when the chooser scored it above zero (its candidates), or when a kind
//! its band names in `offered_by` was. Only `enforced` bands fail a test.
use super::{Report, TICKS_PER_SECOND};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const BANDS_JSON: &str = include_str!("../data/behaviour_bands.json");
/// Goof waves are measured over windows this long.
pub const WINDOW_TICKS: u64 = 10 * TICKS_PER_SECOND as u64;
/// A choice point's decision older than this is no longer in effect.
const IN_EFFECT_TICKS: u64 = TICKS_PER_SECOND as u64;

/// One living bot's tick.
pub fn sample(
    report: &mut Report,
    thought: &bri_sim::session::BotThought,
    mounted: bool,
    tick: u64,
    first: u64,
) {
    let kind = if mounted {
        "vehicle"
    } else if thought.surprise.interrupt.is_some() {
        "goof"
    } else {
        thought.behaviour
    };
    *report.kinds.entry(kind.into()).or_default() += 1;
    report.kind_ticks += 1;
    if !report.offered.contains(kind) {
        report.offered.insert(kind.into());
    }
    let window = ((tick - first) / WINDOW_TICKS) as usize;
    if report.goof_windows.len() <= window {
        report.goof_windows.resize(window + 1, (0, 0));
    }
    report.goof_windows[window].1 += 1;
    if kind == "goof" {
        report.goof_windows[window].0 += 1;
    }
    for d in &thought.surprise.decisions {
        let offered = d.candidates.iter().filter(|c| c.score > 0.0);
        if d.domain == "behaviour" {
            for c in offered {
                if !report.offered.contains(&c.option) {
                    report.offered.insert(c.option.clone());
                }
            }
            continue;
        }
        for c in offered {
            let key = format!("{}:{}", d.domain, c.option);
            if !report.offered.contains(&key) {
                report.offered.insert(key);
            }
        }
        if tick.saturating_sub(d.tick) <= IN_EFFECT_TICKS {
            *report.domain_ticks.entry(d.domain.into()).or_default() += 1;
            *report
                .options
                .entry(format!("{}:{}", d.domain, d.chosen))
                .or_default() += 1;
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    #[serde(default)]
    pub min: f32,
    #[serde(default = "one")]
    pub max: f32,
    /// Which edge fails a test: `floor`, `ceiling` or `both`; none reports
    /// only.
    #[serde(default)]
    pub enforced: Enforce,
    /// The kind counts as offered when any of these was (besides itself).
    #[serde(default)]
    pub offered_by: Vec<String>,
    #[serde(default)]
    pub note: String,
}
fn one() -> f32 {
    1.0
}
/// A scenario's change to a band.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandChange {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub enforced: Option<Enforce>,
    #[serde(default)]
    pub note: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Enforce {
    #[default]
    Off,
    Floor,
    Ceiling,
    Both,
}
impl Enforce {
    /// Whether `flag` breaks a test under this.
    pub fn fails(self, flag: Flag) -> bool {
        matches!(
            (self, flag),
            (Self::Floor | Self::Both, Flag::Dormant)
                | (Self::Ceiling | Self::Both, Flag::Dominant)
        )
    }
    fn word(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Floor => " floor enforced",
            Self::Ceiling => " ceiling enforced",
            Self::Both => " enforced",
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bands {
    pub schema_version: u32,
    #[serde(default)]
    pub note: String,
    pub kinds: BTreeMap<String, Band>,
    /// By scenario name (a trailing `*` matches a prefix), changes to bands.
    #[serde(default)]
    pub scenarios: BTreeMap<String, BTreeMap<String, BandChange>>,
}
impl Bands {
    pub fn shipped() -> Self {
        let bands: Self = serde_json::from_str(BANDS_JSON).expect("behaviour_bands.json");
        assert_eq!(bands.schema_version, 1, "behaviour_bands.json schema");
        for (kind, band) in &bands.kinds {
            assert!(
                (0.0..=1.0).contains(&band.min) && band.min <= band.max && band.max <= 1.0,
                "behaviour_bands.json: {kind}'s band"
            );
        }
        bands
    }
    /// `kind`'s band in `scenario`.
    pub fn band(&self, scenario: &str, kind: &str) -> Option<Band> {
        let mut band = self.kinds.get(kind)?.clone();
        for (pattern, changes) in &self.scenarios {
            let hit = match pattern.strip_suffix('*') {
                Some(prefix) => scenario.starts_with(prefix),
                None => scenario == pattern,
            };
            if let Some(change) = changes.get(kind).filter(|_| hit) {
                band.min = change.min.unwrap_or(band.min);
                band.max = change.max.unwrap_or(band.max);
                band.enforced = change.enforced.unwrap_or(band.enforced);
            }
        }
        Some(band)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Ok,
    Dormant,
    Dominant,
    /// Never offered here: no judgement.
    Absent,
    /// Seen but no band.
    Unbanded,
}
impl Flag {
    pub fn word(self) -> &'static str {
        match self {
            Flag::Ok => "ok",
            Flag::Dormant => "DORMANT",
            Flag::Dominant => "DOMINANT",
            Flag::Absent => "-",
            Flag::Unbanded => "no band",
        }
    }
    pub fn bad(self) -> bool {
        matches!(self, Flag::Dormant | Flag::Dominant)
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub kind: String,
    pub share: f32,
    pub band: Option<Band>,
    pub flag: Flag,
}

/// The share of `kind` in `report`: of bot time, or of its choice
/// point's time for an option (`aim:feet`).
pub fn share(report: &Report, kind: &str) -> f32 {
    match kind.split_once(':') {
        Some((domain, _)) => {
            let total = report.domain_ticks.get(domain).copied().unwrap_or(0);
            report.options.get(kind).copied().unwrap_or(0) as f32 / total.max(1) as f32
        }
        None => {
            report.kinds.get(kind).copied().unwrap_or(0) as f32 / report.kind_ticks.max(1) as f32
        }
    }
}

/// Every banded kind, then every kind seen without a band.
pub fn rows(report: &Report, bands: &Bands) -> Vec<Row> {
    let mut names: Vec<String> = bands.kinds.keys().cloned().collect();
    for seen in report.kinds.keys().chain(report.options.keys()) {
        if !names.contains(seen) {
            names.push(seen.clone());
        }
    }
    names
        .into_iter()
        .map(|kind| {
            let share = share(report, &kind);
            let band = bands.band(&report.name, &kind);
            let flag = match &band {
                None => Flag::Unbanded,
                Some(b) => {
                    let offered = report.offered.contains(&kind)
                        || b.offered_by.iter().any(|k| report.offered.contains(k));
                    if share > b.max + 1e-6 {
                        Flag::Dominant
                    } else if !offered {
                        Flag::Absent
                    } else if share + 1e-6 < b.min {
                        Flag::Dormant
                    } else {
                        Flag::Ok
                    }
                }
            };
            Row {
                kind,
                share,
                band,
                flag,
            }
        })
        .collect()
}

/// Shannon entropy, in bits, of the kinds' shares of bot time: 0 when one
/// kind fills it.
pub fn variety(report: &Report) -> f32 {
    let total = report.kind_ticks.max(1) as f32;
    report
        .kinds
        .values()
        .map(|t| *t as f32 / total)
        .filter(|p| *p > 0.0)
        .map(|p| -p * p.log2())
        .sum::<f32>()
        .max(0.0)
        + 0.0
}

/// Standard deviation of the goof share over ten-second windows: goofing
/// that comes in waves scores above goofing spread evenly (or none).
pub fn goof_waves(report: &Report) -> f32 {
    let shares: Vec<f32> = report
        .goof_windows
        .iter()
        .filter(|(_, ticks)| *ticks >= WINDOW_TICKS / 2)
        .map(|(goof, ticks)| *goof as f32 / *ticks as f32)
        .collect();
    if shares.len() < 2 {
        return 0.0;
    }
    let mean = shares.iter().sum::<f32>() / shares.len() as f32;
    (shares.iter().map(|s| (s - mean).powi(2)).sum::<f32>() / shares.len() as f32).sqrt()
}

/// The compact table: one line per kind seen or flagged.
pub fn table(report: &Report, rows: &[Row]) -> String {
    let mut out = format!(
        "SHARES {}: {:.1} bot-min, variety {:.2} bits, goof waves {:.3}\n",
        report.name,
        report.kind_ticks as f32 / (60.0 * TICKS_PER_SECOND as f32),
        variety(report),
        goof_waves(report),
    );
    for row in rows {
        if row.share == 0.0 && !row.flag.bad() {
            continue;
        }
        let band = row.band.as_ref().map_or("".to_string(), |b| {
            format!(
                "{:>3.0}-{:<3.0}%{}",
                100.0 * b.min,
                100.0 * b.max,
                b.enforced.word()
            )
        });
        out.push_str(&format!(
            "  {:<16} {:>5.1}%  {:<26} {}\n",
            row.kind,
            100.0 * row.share,
            band,
            row.flag.word()
        ));
    }
    out
}

/// Prints the table and fails on an enforced band's flag.
pub fn check(report: &Report) {
    let bands = Bands::shipped();
    let rows = rows(report, &bands);
    eprint!("{}", table(report, &rows));
    let broken: Vec<String> = rows
        .iter()
        .filter(|r| r.band.as_ref().is_some_and(|b| b.enforced.fails(r.flag)))
        .map(|r| format!("{} {} at {:.1}%", r.kind, r.flag.word(), 100.0 * r.share))
        .collect();
    assert!(
        broken.is_empty(),
        "{}: enforced behaviour bands broken: {}",
        report.name,
        broken.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(kinds: &[(&str, u64)], offered: &[&str]) -> Report {
        let mut r = Report {
            name: "test_scenario".into(),
            ..Default::default()
        };
        for (k, t) in kinds {
            r.kinds.insert(k.to_string(), *t);
            r.kind_ticks += t;
        }
        r.offered = offered.iter().map(|s| s.to_string()).collect();
        r
    }

    #[test]
    fn dormant_needs_an_offer_and_dominant_does_not() {
        let bands: Bands = serde_json::from_str(
            r#"{"schema_version": 1, "kinds": {
                "fight": {"min": 0.1, "max": 0.8},
                "chase": {"min": 0.1, "max": 0.8},
                "goof": {"min": 0.05, "max": 0.2, "offered_by": ["wander"]}},
              "scenarios": {"test_*": {"chase": {"max": 0.95}}}}"#,
        )
        .unwrap();
        let r = report(
            &[("fight", 90), ("wander", 10)],
            &["fight", "chase", "wander"],
        );
        let rows = super::rows(&r, &bands);
        let flag = |k: &str| rows.iter().find(|r| r.kind == k).unwrap().flag;
        assert_eq!(flag("fight"), Flag::Dominant);
        assert_eq!(flag("chase"), Flag::Dormant, "offered, never taken");
        assert_eq!(flag("goof"), Flag::Dormant, "offered with wander");
        assert_eq!(flag("wander"), Flag::Unbanded);
        let r = report(&[("chase", 90), ("fight", 10)], &["fight", "chase"]);
        let rows = super::rows(&r, &bands);
        let flag = |k: &str| rows.iter().find(|r| r.kind == k).unwrap().flag;
        assert_eq!(flag("chase"), Flag::Ok, "the scenario's own ceiling");
        assert_eq!(flag("goof"), Flag::Absent, "no pause, no goof");
        assert!((variety(&r) - 0.469).abs() < 0.01);
    }

    #[test]
    fn the_shipped_bands_parse() {
        let bands = Bands::shipped();
        assert!(bands.kinds.values().any(|b| b.enforced != Enforce::Off));
    }
}
