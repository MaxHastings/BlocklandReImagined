//! Score reports: a table of named rows under titled columns, shown to a
//! player in a window of its own (Slayer's End of Round Report).
//!
//! The engine owns the table and its window; a game's rules fill it
//! (`show_report`). Another Add-On may change a game's columns
//! (`report_column`): Capture the Flag puts Flag Pick-ups and Flag Returns
//! where Slayer shows Kills and Deaths, as its `scoreListInit` and
//! `scoreListAdd` callbacks did. The engine puts the two together when it
//! sends the report, so neither Add-On depends on the other running first.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most columns a report has, the name column included.
pub const MAX_REPORT_COLUMNS: usize = 12;
/// Most sections (Teams, Players).
pub const MAX_REPORT_SECTIONS: usize = 4;
/// Most rows over every section.
pub const MAX_REPORT_ROWS: usize = 256;
/// Longest title, banner, row name or cell, in characters.
pub const MAX_REPORT_TEXT: usize = 64;
/// Longest column or row key.
pub const MAX_REPORT_KEY: usize = 32;
/// Most column changes one game keeps.
pub const MAX_REPORT_OVERRIDES: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// The window's title.
    #[serde(default)]
    pub title: String,
    /// Large text over the table ("VICTORY", "DEFEAT"), or none.
    #[serde(default)]
    pub banner: Option<String>,
    /// The columns after each row's name, left to right.
    #[serde(default)]
    pub columns: Vec<ReportColumn>,
    #[serde(default)]
    pub sections: Vec<ReportSection>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportColumn {
    /// What rows' cells are keyed by, and what a change names.
    pub key: String,
    pub title: String,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSection {
    /// Its heading ("Teams:"), or empty for none.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rows: Vec<ReportRow>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRow {
    /// What a column change fills this row's cell by: `team:<id>` or
    /// `player:<id>` by convention.
    #[serde(default)]
    pub key: String,
    pub name: String,
    /// The name's palette colour (a team's), or the window's own text
    /// colour.
    #[serde(default)]
    pub color: Option<u8>,
    /// Cells by column key; a missing one is blank.
    #[serde(default)]
    pub cells: BTreeMap<String, String>,
}

/// One Add-On's change to a game's report columns: the column `key`
/// retitled and filled from `cells` by row key, or added at the end when
/// the report has no such column; `title: None` takes the column out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnChange {
    pub key: String,
    pub title: Option<String>,
    pub cells: BTreeMap<String, String>,
}

pub fn is_report_key(key: &str) -> bool {
    (1..=MAX_REPORT_KEY).contains(&key.len())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
}
fn is_text(text: &str) -> bool {
    text.chars().count() <= MAX_REPORT_TEXT && !text.chars().any(char::is_control)
}

impl Report {
    /// Within the report's limits.
    pub fn is_bounded(&self) -> bool {
        let rows: usize = self.sections.iter().map(|s| s.rows.len()).sum();
        is_text(&self.title)
            && self.banner.as_deref().is_none_or(is_text)
            && self.columns.len() < MAX_REPORT_COLUMNS
            && self
                .columns
                .iter()
                .all(|c| is_report_key(&c.key) && is_text(&c.title))
            && self.sections.len() <= MAX_REPORT_SECTIONS
            && rows <= MAX_REPORT_ROWS
            && self.sections.iter().all(|s| {
                is_text(&s.title)
                    && s.rows.iter().all(|r| {
                        (r.key.is_empty() || is_report_key(&r.key))
                            && is_text(&r.name)
                            && r.cells.len() < MAX_REPORT_COLUMNS
                            && r.cells.iter().all(|(k, v)| is_report_key(k) && is_text(v))
                    })
            })
    }
    /// Apply a game's column changes, in the order they were made.
    pub fn apply(&mut self, changes: &[ColumnChange]) {
        for change in changes {
            let at = self.columns.iter().position(|c| c.key == change.key);
            match (&change.title, at) {
                (None, Some(at)) => {
                    self.columns.remove(at);
                }
                (None, None) => continue,
                (Some(title), Some(at)) => self.columns[at].title = title.clone(),
                (Some(title), None) => {
                    if self.columns.len() + 1 >= MAX_REPORT_COLUMNS {
                        continue;
                    }
                    self.columns.push(ReportColumn {
                        key: change.key.clone(),
                        title: title.clone(),
                    });
                }
            }
            for row in self.sections.iter_mut().flat_map(|s| s.rows.iter_mut()) {
                match change
                    .cells
                    .get(&row.key)
                    .filter(|_| change.title.is_some())
                {
                    Some(cell) => {
                        row.cells.insert(change.key.clone(), cell.clone());
                    }
                    None => {
                        row.cells.remove(&change.key);
                    }
                }
            }
        }
    }
}
impl ColumnChange {
    pub fn is_bounded(&self) -> bool {
        is_report_key(&self.key)
            && self.title.as_deref().is_none_or(is_text)
            && self.cells.len() <= MAX_REPORT_ROWS
            && self
                .cells
                .iter()
                .all(|(k, v)| is_report_key(k) && is_text(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, cells: &[(&str, &str)]) -> ReportRow {
        ReportRow {
            key: key.into(),
            name: key.into(),
            color: None,
            cells: cells
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
        }
    }

    #[test]
    fn a_column_change_retitles_refills_adds_and_removes_by_key() {
        let mut report = Report {
            title: "End of Round Report".into(),
            banner: Some("VICTORY".into()),
            columns: ["score", "kills", "deaths"]
                .map(|k| ReportColumn {
                    key: k.into(),
                    title: k.into(),
                })
                .into(),
            sections: vec![ReportSection {
                title: "Players:".into(),
                rows: vec![
                    row("player:1", &[("score", "3"), ("kills", "2")]),
                    row("player:2", &[("score", "1"), ("kills", "5")]),
                ],
            }],
        };
        assert!(report.is_bounded());
        report.apply(&[
            ColumnChange {
                key: "kills".into(),
                title: Some("Flag Pick-ups".into()),
                cells: BTreeMap::from([("player:1".into(), "4".into())]),
            },
            ColumnChange {
                key: "deaths".into(),
                title: None,
                cells: BTreeMap::new(),
            },
            ColumnChange {
                key: "assists".into(),
                title: Some("Assists".into()),
                cells: BTreeMap::from([("player:2".into(), "7".into())]),
            },
        ]);
        let titles: Vec<&str> = report.columns.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["score", "Flag Pick-ups", "Assists"]);
        let rows = &report.sections[0].rows;
        assert_eq!(rows[0].cells.get("kills").map(String::as_str), Some("4"));
        assert_eq!(
            rows[1].cells.get("kills"),
            None,
            "a row the change leaves out is blank"
        );
        assert_eq!(rows[1].cells.get("assists").map(String::as_str), Some("7"));
        assert_eq!(rows[0].cells.get("score").map(String::as_str), Some("3"));
    }

    #[test]
    fn a_report_past_its_limits_is_refused() {
        let long = Report {
            title: "x".repeat(MAX_REPORT_TEXT + 1),
            ..Default::default()
        };
        assert!(!long.is_bounded());
        let control = Report {
            sections: vec![ReportSection {
                title: String::new(),
                rows: vec![row("player:1", &[("score", "a\nb")])],
            }],
            ..Default::default()
        };
        assert!(!control.is_bounded());
        let many = Report {
            sections: vec![ReportSection {
                title: String::new(),
                rows: vec![row("player:1", &[]); MAX_REPORT_ROWS + 1],
            }],
            ..Default::default()
        };
        assert!(!many.is_bounded());
        assert!(
            !ColumnChange {
                key: "bad key".into(),
                title: None,
                cells: BTreeMap::new()
            }
            .is_bounded()
        );
    }
}
