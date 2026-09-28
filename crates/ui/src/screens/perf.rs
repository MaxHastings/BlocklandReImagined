//! Canvas overlays drawn above every screen, as `NetGraphGui` was added to
//! v20's Canvas: the net graph and the performance overlay. Nothing is
//! drawn (or measured) while they are hidden.
use crate::draw::DrawList;
use crate::geom::{Rect, Rgba};
use crate::models::perf::{
    FRAME_HISTORY, FrameSample, NET_GRAPH_POINTS, NET_PLOTS, NetGraph, PerfMode, PerfOverlay,
};
use crate::pack::Pack;
use crate::text::Font;
use crate::ui::Core;

/// `NetGraphGui` is authored at 640x480 with `horizSizing = "left"`, so it
/// keeps its distance from the right edge: the graph at "432 5", 200x200.
const AUTHORED_WIDTH: i32 = 640;
const GRAPH: Rect = Rect::new(432, 5, 200, 200);
/// The six labels (`allClientGuis.gui` `NetGraphGui`), in plot order, with
/// their profile, authored position and the text shown before any sample.
const LABELS: [(&str, i32, i32, &str, &str); NET_PLOTS] = [
    (
        "NetGraphGhostsActiveProfile",
        436,
        156,
        "Ghosts Active",
        "Ghosts Active: ",
    ),
    (
        "NetGraphGhostUpdatesProfile",
        536,
        156,
        "Ghost Updates",
        "Ghost Updates: ",
    ),
    (
        "NetGraphBitsSentProfile",
        436,
        170,
        "Bits Sent",
        "Bits Sent: ",
    ),
    (
        "NetGraphBitsReceivedProfile",
        536,
        170,
        "Bits Received",
        "Bits Received: ",
    ),
    ("NetGraphLatencyProfile", 436, 184, "Latency", "Latency: "),
    (
        "NetGraphPacketLossProfile",
        536,
        184,
        "Packet Loss",
        "Packet Loss: ",
    ),
];
/// The labels' authored height.
const LABEL_HEIGHT: i32 = 18;
/// `GuiGraphCtrl`'s plot colours, the same as the label profiles' fonts.
const PLOT_COLORS: [Rgba; NET_PLOTS] = [
    [255, 255, 255, 255],
    [255, 0, 0, 255],
    [0, 255, 0, 255],
    [0, 0, 255, 255],
    [0, 255, 255, 255],
    [0, 0, 0, 255],
];

/// The overlay's own look (not v20): a dark panel of monospace figures.
const OVERLAY_FONT: &str = "lucida console_12";
const PANEL: Rgba = [8, 10, 16, 190];
const EDGE: Rgba = [255, 255, 255, 40];
const LABEL: Rgba = [150, 160, 180, 255];
const VALUE: Rgba = [240, 240, 240, 255];
const GOOD: Rgba = [110, 220, 120, 255];
const FAIR: Rgba = [250, 200, 70, 255];
const BAD: Rgba = [255, 90, 80, 255];
const CPU: Rgba = [90, 160, 255, 255];
const GUIDE: Rgba = [255, 255, 255, 60];
const MARGIN: i32 = 8;
const PAD: i32 = 6;

pub fn draw(pack: &Pack, dl: &mut DrawList, core: &Core) {
    if let Some(graph) = &core.net_graph {
        net_graph(pack, dl, core.logical.0, graph);
    }
    if core.perf.visible() {
        let top = net_graph_bottom(core);
        overlay(pack, dl, core, top);
    }
}

/// Where the top-right corner is free again below the net graph.
fn net_graph_bottom(core: &Core) -> i32 {
    if core.net_graph.is_some() {
        GRAPH.bottom() + 4
    } else {
        MARGIN
    }
}

/// Where panels anchored top-right may start, below both overlays.
pub fn top_right_bottom(pack: &Pack, core: &Core) -> i32 {
    let top = net_graph_bottom(core);
    if !core.perf.visible() {
        return top;
    }
    match Font::get(pack, OVERLAY_FONT) {
        Some(font) => top + panel_size(&font, core).1 + 4,
        None => top,
    }
}

/// `GuiGraphCtrl::onRender`: each plot is a line through its last 200
/// samples, newest at the right edge, scaled so its largest value reaches
/// 95% of the height (`max * 1.05`). The control has no background.
fn net_graph(pack: &Pack, dl: &mut DrawList, width: i32, graph: &NetGraph) {
    let dx = width - AUTHORED_WIDTH;
    let r = GRAPH.offset(dx, 0);
    for (plot, color) in PLOT_COLORS.iter().enumerate() {
        let max = graph.scale(plot);
        if max <= 0.0 {
            continue;
        }
        let scale = r.h as f32 / (max * 1.05);
        let y = |v: f32| r.bottom() - 1 - (v * scale).round() as i32;
        let mut previous: Option<i32> = None;
        for (i, v) in graph.plot(plot).take(NET_GRAPH_POINTS).enumerate() {
            let x = r.right() - 1 - i as i32;
            let cur = y(v);
            let (top, bottom) = match previous {
                Some(p) => (p.min(cur), p.max(cur)),
                None => (cur, cur),
            };
            dl.fill(Rect::new(x, top, 1, bottom - top + 1), *color);
            previous = Some(cur);
        }
    }
    let latest = graph.latest();
    for (plot, (profile, x, y, idle, prefix)) in LABELS.iter().enumerate() {
        let Some(style) = pack.data.styles.get(*profile) else {
            continue;
        };
        let Some(font) = style.font.as_deref().and_then(|f| Font::get(pack, f)) else {
            continue;
        };
        let text = match latest {
            Some(s) => {
                let v = s.plots()[plot];
                // getPacketLoss is a fraction of a percent; the rest count.
                if plot == 5 {
                    format!("{prefix}{v:.1}")
                } else {
                    format!("{prefix}{v:.0}")
                }
            }
            None => (*idle).to_string(),
        };
        // GuiTextCtrl centres its line in the 18-pixel-high control.
        font.draw_outlined(
            dl,
            (x + dx) as f32,
            (y + (LABEL_HEIGHT - font.line_height()) / 2) as f32,
            &text,
            style.font_color.unwrap_or(PLOT_COLORS[plot]),
            style.font_outline,
            &[],
        );
    }
}

fn ms_color(ms: f32) -> Rgba {
    if ms <= 1000.0 / 60.0 * 1.05 {
        GOOD
    } else if ms <= 1000.0 / 30.0 {
        FAIR
    } else {
        BAD
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / (1024.0 * 1024.0))
}

/// Text lines of the panel, each a list of (text, colour) runs.
type Line = Vec<(String, Rgba)>;

fn label(s: impl Into<String>) -> (String, Rgba) {
    (s.into(), LABEL)
}
fn value(s: impl Into<String>) -> (String, Rgba) {
    (s.into(), VALUE)
}

/// The rows above and below the frame graph.
fn lines(core: &Core) -> (Vec<Line>, Vec<Line>) {
    let o: &PerfOverlay = &core.perf;
    let s = o.summary();
    let gpu = s
        .gpu_ms
        .map_or_else(|| "n/a".to_string(), |g| format!("{g:.1}"));
    let head = vec![
        vec![
            (format!("{:>4.0} FPS", s.fps), ms_color(s.frame_ms)),
            value(format!("  {:>5.1} ms", s.frame_ms)),
        ],
        vec![
            (format!("CPU {:>4.1}", s.cpu_ms), CPU),
            label(format!("  GPU {gpu}")),
        ],
    ];
    if o.mode != PerfMode::Expanded {
        return (head, Vec::new());
    }
    let mut head = head;
    head[0].push(label(format!("  worst {:.1}", s.worst_ms)));
    head[1].push(label(format!("  wait {:.1} ms", s.wait_ms)));
    let st = &o.stats;
    let mut body: Vec<Line> = Vec::new();
    match (&st.server, st.remote_server) {
        (Some(sv), _) => body.push(vec![
            label("Server "),
            (
                format!("{:.0} tps", sv.ticks_per_second),
                if sv.ticks_per_second >= 110.0 {
                    GOOD
                } else {
                    FAIR
                },
            ),
            value(format!("  tick {:.2} ms", sv.tick_ms_mean)),
            label(format!("  max {:.2}", sv.tick_ms_max)),
        ]),
        (None, true) => body.push(vec![label("Server "), value("another computer hosts")]),
        (None, false) => {}
    }
    if let Some(n) = &o.net {
        let kb = |bits: f32| n.per_second(bits) / 8.0 / 1024.0;
        body.push(vec![
            label("Net    "),
            value(format!("{:.0} ms", n.latency_ms)),
            (
                format!("  {:.1}% loss", n.packet_loss),
                if n.packet_loss < 1.0 {
                    VALUE
                } else if n.packet_loss < 5.0 {
                    FAIR
                } else {
                    BAD
                },
            ),
        ]);
        body.push(vec![
            label("  in   "),
            value(format!(
                "{:>6.1} KB/s {:>4.0} pk/s",
                kb(n.bits_received),
                n.per_second(n.packets_received)
            )),
        ]);
        body.push(vec![
            label("  out  "),
            value(format!(
                "{:>6.1} KB/s {:>4.0} pk/s",
                kb(n.bits_sent),
                n.per_second(n.packets_sent)
            )),
        ]);
    }
    if let Some(bricks) = st.bricks {
        body.push(vec![label("World  "), value(format!("{bricks} bricks"))]);
        body.push(vec![
            label("       "),
            value(format!(
                "{} players  {} vehicles  {} entities",
                st.players.unwrap_or(0),
                st.vehicles.unwrap_or(0),
                st.entities.unwrap_or(0)
            )),
        ]);
    }
    if let Some(mem) = st.memory_bytes {
        let mut line = vec![label("Memory "), value(megabytes(mem))];
        if let Some(private) = st.private_bytes {
            line.push(label(format!("  private {}", megabytes(private))));
        }
        body.push(line);
    }
    if let Some(sv) = &st.server
        && !sv.script_ms.is_empty()
    {
        body.push(vec![label("Add-On scripts, ms per tick")]);
        for (id, ms) in sv.script_ms.iter().take(6) {
            let name: String = id.chars().take(24).collect();
            body.push(vec![
                value(format!("  {name:<24}")),
                (
                    format!("{ms:>6.3}"),
                    if *ms < 0.5 {
                        VALUE
                    } else if *ms < 2.0 {
                        FAIR
                    } else {
                        BAD
                    },
                ),
            ]);
        }
    }
    if !st.gpu.is_empty() {
        let gpu: String = st.gpu.chars().take(40).collect();
        body.push(vec![label(gpu)]);
    }
    (head, body)
}

fn line_width(font: &Font, line: &Line) -> i32 {
    line.iter().map(|(t, _)| font.width(t)).sum()
}

fn graph_height(core: &Core) -> i32 {
    if core.perf.mode == PerfMode::Expanded {
        48
    } else {
        20
    }
}

fn line_step(font: &Font) -> i32 {
    font.line_height() + 2
}

fn panel_size(font: &Font, core: &Core) -> (i32, i32) {
    let (head, body) = lines(core);
    let text_w = head
        .iter()
        .chain(&body)
        .map(|l| line_width(font, l))
        .max()
        .unwrap_or(0);
    let graph_w = if core.perf.mode == PerfMode::Expanded {
        FRAME_HISTORY as i32
    } else {
        120
    };
    let w = text_w.max(graph_w) + PAD * 2;
    let h = PAD * 2 + (head.len() + body.len()) as i32 * line_step(font) + graph_height(core) + 6;
    (w, h)
}

fn overlay(pack: &Pack, dl: &mut DrawList, core: &Core, top: i32) {
    let Some(font) = Font::get(pack, OVERLAY_FONT) else {
        return;
    };
    let (w, h) = panel_size(&font, core);
    let (sw, sh) = core.logical;
    let x = (sw - w - MARGIN).max(0);
    let panel = Rect::new(x, top, w.min(sw), h.min((sh - top).max(0)));
    if !dl.push_clip(panel) {
        return;
    }
    dl.fill(panel, PANEL);
    dl.frame(panel, EDGE);
    let (head, body) = lines(core);
    let step = line_step(&font);
    let mut y = top + PAD;
    let text = |dl: &mut DrawList, y: i32, line: &Line| {
        let mut pen = (x + PAD) as f32;
        for (t, c) in line {
            pen = font.draw(dl, pen, y as f32, t, *c, &[]);
        }
    };
    for line in &head {
        text(dl, y, line);
        y += step;
    }
    y += 2;
    let graph = Rect::new(x + PAD, y, w - PAD * 2, graph_height(core));
    frame_graph(
        dl,
        graph,
        core.perf.frames(),
        core.perf.mode == PerfMode::Expanded,
    );
    y = graph.bottom() + 4;
    for line in &body {
        text(dl, y, line);
        y += step;
    }
    dl.pop_clip();
}

/// One bar per frame, newest at the right: its height is the frame time,
/// coloured by the frame rate it meets; the expanded view marks the CPU's
/// share at the bottom of each bar and the 60 and 30 FPS lines.
fn frame_graph<'a>(
    dl: &mut DrawList,
    r: Rect,
    frames: impl Iterator<Item = &'a FrameSample>,
    detail: bool,
) {
    dl.fill(r, [0, 0, 0, 90]);
    let frames: Vec<&FrameSample> = frames.take(r.w.max(0) as usize).collect();
    let worst = frames.iter().map(|f| f.frame_ms).fold(0.0, f32::max);
    // Keep 30 FPS on the graph; grow for hitches, up to 100 ms.
    let top_ms = worst.clamp(1000.0 / 30.0 * 1.2, 100.0);
    let px = |ms: f32| ((ms / top_ms) * r.h as f32).round().clamp(0.0, r.h as f32) as i32;
    for (i, f) in frames.iter().enumerate() {
        let x = r.right() - 1 - i as i32;
        let bar = px(f.frame_ms).max(1);
        dl.fill(Rect::new(x, r.bottom() - bar, 1, bar), ms_color(f.frame_ms));
        if detail {
            let cpu = px(f.cpu_ms).min(bar);
            dl.fill(Rect::new(x, r.bottom() - cpu, 1, cpu), CPU);
        }
    }
    if detail {
        for fps in [60.0, 30.0] {
            let y = r.bottom() - px(1000.0 / fps);
            dl.fill(Rect::new(r.x, y, r.w, 1), GUIDE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawCmd;
    use crate::models::perf::NetSample;

    #[test]
    fn net_graph_draws_each_plot_right_anchored_newest_at_the_edge() {
        let mut g = NetGraph::default();
        for v in [10.0, 20.0] {
            g.add(NetSample {
                interval_ms: 32.0,
                latency_ms: v,
                ..Default::default()
            });
        }
        let mut dl = DrawList::new(Rect::new(0, 0, 1280, 720));
        let pack = Pack::from_parts(Default::default(), Default::default());
        net_graph(&pack, &mut dl, 1280, &g);
        let fills: Vec<Rect> = dl
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Fill { dst, color, .. } if *color == PLOT_COLORS[4] => Some(*dst),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 2, "only the latency plot has data");
        let right = 1280 - (AUTHORED_WIDTH - GRAPH.right());
        assert_eq!(fills[0].x, right - 1);
        // The newest (20) is the plot's maximum, drawn at 1/1.05 of the height.
        let top = GRAPH.bottom() - 1 - (200.0 / 1.05_f32).round() as i32;
        assert_eq!(fills[0].y, top);
        // The older point joins it with a vertical span.
        assert_eq!(fills[1].x, right - 2);
        assert_eq!(fills[1].y, top);
    }

    #[test]
    fn frame_graph_bars_are_coloured_by_frame_rate() {
        let frames = [
            FrameSample {
                frame_ms: 50.0,
                ..Default::default()
            },
            FrameSample {
                frame_ms: 10.0,
                ..Default::default()
            },
        ];
        let mut dl = DrawList::new(Rect::new(0, 0, 640, 480));
        frame_graph(&mut dl, Rect::new(0, 0, 100, 40), frames.iter(), false);
        let colors: Vec<Rgba> = dl
            .cmds
            .iter()
            .skip(1)
            .filter_map(|c| match c {
                DrawCmd::Fill { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        assert_eq!(colors, vec![BAD, GOOD]);
    }
}
