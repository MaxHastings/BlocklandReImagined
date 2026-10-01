//! The Report window: a score table the host shows (Slayer's End of Round
//! Report, `Slayer_CtrDisplay`): an optional VICTORY or DEFEAT banner, a
//! header of column titles, then each section's rows, names in their
//! team's colour. Slayer filled its window with ML text a line at a time;
//! here the host sends the table and this window lays it out.
use super::*;
use crate::api::ReportView;
use crate::view::EventKind;

const TEXT: &str = "Report_Text";
const SCROLL: &str = "Report_Scroll";
const CLOSE: &str = "Report_Close";
/// The name column's width, then each other column's (`<tab:150, 250,
/// ...>` in Slayer's header).
const NAME_WIDTH: i32 = 150;
const COLUMN_WIDTH: i32 = 110;

fn named(mut c: Control, name: &str) -> Control {
    c.name = Some(name.into());
    c
}

/// Text as it reads: no markup, no control characters.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '<' => '‹',
            '>' => '›',
            _ => c,
        })
        .collect()
}

/// The report as ML text: Slayer's `<h1>` banner, `<h2>` header and `<b>`
/// names (Arial Bold 24, 20 and 15).
pub fn markup(report: &ReportView) -> String {
    let mut stops = Vec::with_capacity(report.columns.len());
    for i in 0..report.columns.len() as i32 {
        stops.push((NAME_WIDTH + i * COLUMN_WIDTH).to_string());
    }
    let mut out = format!("<tab:{}><font:Arial:16>", stops.join(","));
    if let Some(banner) = &report.banner {
        out.push_str(&format!(
            "<just:center><font:Arial Bold:24>{}<font:Arial:16><just:left><br>",
            plain(banner)
        ));
    }
    out.push_str("<font:Arial Bold:20>Name");
    for title in &report.columns {
        out.push('\t');
        out.push_str(&plain(title));
    }
    out.push_str("<font:Arial:16><br>");
    for (i, section) in report.sections.iter().enumerate() {
        if section.rows.is_empty() {
            continue;
        }
        if !section.title.is_empty() {
            if i > 0 {
                out.push_str("<br>");
            }
            out.push_str(&format!(
                "<font:Arial Bold:15>{}<font:Arial:16><br>",
                plain(&section.title)
            ));
        }
        for row in &section.rows {
            out.push_str("<spush><font:Arial Bold:15>");
            if let Some([r, g, b, _]) = row.color {
                out.push_str(&format!("<color:{r:02x}{g:02x}{b:02x}>"));
            }
            out.push_str(&plain(&row.name));
            out.push_str("<spop>");
            for cell in &row.cells {
                out.push('\t');
                out.push_str(&plain(cell));
            }
            out.push_str("<br>");
        }
    }
    out
}

pub struct Report {
    view: View,
    /// The report shown, to redraw only when another arrives.
    shown: Option<ReportView>,
}

impl Report {
    pub fn new(core: &Core) -> Self {
        let columns = core.report.as_ref().map_or(4, |r| r.columns.len()) as i32;
        let w = (NAME_WIDTH + columns * COLUMN_WIDTH + 40).clamp(360, 620);
        let h = 340;
        let mut root = ctrl("GuiControl", "GuiDefaultProfile", Rect::new(0, 0, 640, 480));
        let mut win = ctrl(
            "GuiWindowCtrl",
            "BlockWindowProfile",
            Rect::new((640 - w) / 2, (480 - h) / 2, w, h),
        );
        win.text = Some(
            core.report
                .as_ref()
                .map(|r| plain(&r.title))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "Report".into()),
        );
        win.h_sizing = HSizing::Center;
        win.v_sizing = VSizing::Center;
        let mut scroll = named(
            ctrl(
                "GuiScrollCtrl",
                "BlockScrollProfile",
                Rect::new(12, 34, w - 24, h - 84),
            ),
            SCROLL,
        );
        scroll
            .fields
            .insert("hScrollBar".into(), "alwaysOff".into());
        scroll.fields.insert("vScrollBar".into(), "dynamic".into());
        let mut info = text("GuiMLTextProfile", Rect::new(4, 2, w - 48, 16), "");
        info.class = "GuiMLTextCtrl".into();
        scroll.children.push(named(info, TEXT));
        win.children.push(scroll);
        win.children.push(named(
            button(
                "BlockButtonProfile",
                Rect::new(w - 110, h - 42, 98, 28),
                "base/client/ui/button1",
                "Close",
                CLOSE,
            ),
            CLOSE,
        ));
        root.children.push(win);
        let mut view = View::new(&root);
        view.measure(&core.pack);
        let mut s = Self { view, shown: None };
        s.show(core);
        s
    }
    fn show(&mut self, core: &Core) {
        if core.report == self.shown {
            return;
        }
        self.shown = core.report.clone();
        let text = self.shown.as_ref().map(markup).unwrap_or_default();
        if let (Some(n), Some(scroll)) = (self.view.id(TEXT), self.view.id(SCROLL)) {
            let width = self.view.node(n).ctrl.extent[0];
            let h = View::ml_height(&core.pack, "GuiMLTextProfile", &text, width).max(16);
            self.view.nodes[n].ctrl.extent[1] = h;
            self.view.set_text(n, text);
            self.view.scroll_to(scroll, 0);
            self.view.relayout();
        }
    }
}

impl Screen for Report {
    fn id(&self) -> ScreenId {
        ScreenId::Report
    }
    fn view(&self) -> &View {
        &self.view
    }
    fn view_mut(&mut self) -> &mut View {
        &mut self.view
    }
    fn on_update(&mut self, core: &mut Core) {
        self.show(core);
    }
    fn on_key(&mut self, key: Key, _mods: Modifiers, core: &mut Core) -> bool {
        match key {
            Key::Escape | Key::Return | Key::NumpadEnter => core.pop(self.id()),
            _ => return false,
        }
        true
    }
    fn on_event(&mut self, ev: &ViewEvent, core: &mut Core) {
        let name = self
            .view
            .node(ev.node)
            .ctrl
            .name
            .clone()
            .unwrap_or_default();
        if matches!(
            (name.as_str(), ev.kind),
            (_, EventKind::Close) | (CLOSE, EventKind::Click)
        ) {
            core.pop(self.id());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{ReportRowView, ReportSectionView};

    #[test]
    fn a_report_lays_out_its_banner_header_and_coloured_rows_as_plain_text() {
        let report = ReportView {
            title: "End of Round Report".into(),
            banner: Some("VICTORY".into()),
            columns: vec!["Score".into(), "Flag Pick-ups".into()],
            sections: vec![
                ReportSectionView {
                    title: "Teams:".into(),
                    rows: vec![ReportRowView {
                        name: "Red".into(),
                        color: Some([255, 0, 0, 255]),
                        cells: vec!["3".into(), "2".into()],
                    }],
                },
                ReportSectionView {
                    title: "Players:".into(),
                    rows: vec![ReportRowView {
                        name: "<b>sneaky".into(),
                        color: None,
                        cells: vec!["1".into(), String::new()],
                    }],
                },
            ],
        };
        let text = markup(&report);
        assert!(text.starts_with("<tab:150,260>"), "{text}");
        assert!(text.contains("<just:center><font:Arial Bold:24>VICTORY"));
        assert!(text.contains("Name\tScore\tFlag Pick-ups"));
        assert!(text.contains("<color:ff0000>Red<spop>\t3\t2<br>"));
        assert!(
            text.contains("‹b›sneaky<spop>\t1\t<br>"),
            "names never become markup"
        );
        assert!(text.contains("<br><font:Arial Bold:15>Players:"));
    }
}
