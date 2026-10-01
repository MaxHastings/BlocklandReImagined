//! HUD inventory state machine: the bricks / paint / tools scroll modes and
//! the sliding boxes (v20 client script, c:3664–4990, c:6027–6905).
//!
//! Server state (brick inventory, tools, colorset) comes in through setters;
//! requests go out through [`Outbox`]. Authoritative `apply_active_*` updates
//! only change presentation state; they never echo equipment requests. Inventory
//! updates must precede active-slot updates; absent/out-of-range slots clear the
//! corresponding selection instead of being remembered for a later inventory.
//! Nothing here draws; `screens::play`
//! renders this model with the original HUD art.

use crate::api::{BrickInfo, PaintDivision, ToolInfo, UiAction};

pub const NUM_BRICK_SLOTS: usize = 10;
/// FX column labels (`shiftPaintColumn`, c:4623). Index 8 is "Undulo" but
/// uses the `FXjello` art.
pub const FX_NAMES: [&str; 9] = [
    "None", "Pearl", "Chrome", "Glow", "Blink", "Swirl", "Rainbow", "Stable", "Undulo",
];
pub const FX_ART: [&str; 9] = [
    "",
    "fxpearl",
    "fxchrome",
    "fxglow",
    "fxblink",
    "fxswirl",
    "fxrainbow",
    "fxstable",
    "fxjello",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScrollMode {
    Bricks,
    Paint,
    Tools,
    None,
}

/// Requests and client-side notices produced by models.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outbox {
    pub actions: Vec<UiAction>,
    /// `clientCmdCenterPrint(text, seconds)` issued by client script.
    pub center_prints: Vec<(String, f32)>,
}

/// One scheduled `hide*Box(dist, steps)` call: moves `dist` px over `steps`
/// ticks of 10 ms, the last step taking the remainder. Several can run at
/// once (v20 does not cancel a running slide when a new one starts).
#[derive(Debug, Clone, PartialEq)]
struct SlideRun {
    dist: i32,
    steps: i32,
    cur: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Slide {
    /// Accumulated offset in pixels (positive = hidden direction).
    pub offset: i32,
    runs: Vec<SlideRun>,
    acc_ms: u64,
}

impl Slide {
    pub fn start(&mut self, dist: i32, steps: i32) {
        // The first step runs immediately (the script calls itself with
        // currStep 0 synchronously), later steps every 10 ms.
        let mut r = SlideRun {
            dist,
            steps,
            cur: 0,
        };
        self.offset += step_dist(&r);
        r.cur = 1;
        if r.cur < r.steps {
            self.runs.push(r);
        }
    }
    pub fn tick(&mut self, dt_ms: u64) {
        self.acc_ms += dt_ms;
        while self.acc_ms >= 10 {
            self.acc_ms -= 10;
            for r in &mut self.runs {
                self.offset += step_dist(r);
                r.cur += 1;
            }
            self.runs.retain(|r| r.cur < r.steps);
            if self.runs.is_empty() {
                self.acc_ms = 0;
                break;
            }
        }
    }
    pub fn moving(&self) -> bool {
        !self.runs.is_empty()
    }
    /// Jump to the end of all running slides.
    pub fn finish(&mut self) {
        while self.moving() {
            self.tick(10);
        }
    }
}

fn step_dist(r: &SlideRun) -> i32 {
    let per = r.dist.div_euclid(r.steps);
    if r.cur == r.steps - 1 {
        r.dist - (r.steps - 1) * per
    } else {
        per
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HudPrefs {
    pub hide_brick_box: bool,
    pub hide_paint_box: bool,
    pub hide_tool_box: bool,
    pub show_tooltips: bool,
    pub reverse_brick_scroll: bool,
    pub recolor_brick_icons: bool,
    pub show_slot_numbers: bool,
}

impl Default for HudPrefs {
    fn default() -> Self {
        HudPrefs {
            hide_brick_box: true,
            hide_paint_box: true,
            hide_tool_box: true,
            show_tooltips: true,
            reverse_brick_scroll: false,
            recolor_brick_icons: true,
            show_slot_numbers: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HudModel {
    pub mode: ScrollMode,
    pub prefs: HudPrefs,
    /// `$InvData` (authoritative, from the server).
    pub bricks: Vec<Option<BrickInfo>>,
    /// `$CurrScrollBrickSlot` (`None` = "").
    pub cur_brick: Option<usize>,
    pub brick_active: bool,
    pub brick_name: String,
    pub last_instant_use: Option<BrickInfo>,
    instant_use: bool,
    pub tools: Vec<Option<ToolInfo>>,
    pub cur_tool: Option<usize>,
    pub tool_active: bool,
    pub tool_name: String,
    pub paint: Vec<PaintDivision>,
    /// Swatch counts per column including the FX column (9).
    pub paint_rows: Vec<usize>,
    pub paint_row: usize,
    pub paint_swatch: usize,
    pub paint_active: bool,
    pub paint_name: String,
    /// `$currSprayCanIndex` (brick icon tint), flattened colour index.
    pub spray_index: u32,
    pub building_disabled: bool,
    pub painting_disabled: bool,
    pub brick_slide: Slide,
    pub paint_slide: Slide,
    pub tool_slide: Slide,
    /// HUD boxes hidden while the brick selector is open.
    pub boxes_visible: bool,
    /// The tool in hand takes the paint cans (a duplicator's fill
    /// colour), so opening paint from it keeps it in hand.
    pub tool_takes_paint: bool,
}

impl Default for HudModel {
    fn default() -> Self {
        HudModel::new(HudPrefs::default())
    }
}

impl HudModel {
    pub fn new(prefs: HudPrefs) -> Self {
        HudModel {
            mode: ScrollMode::None,
            prefs,
            bricks: vec![None; NUM_BRICK_SLOTS],
            cur_brick: None,
            brick_active: false,
            brick_name: String::new(),
            last_instant_use: None,
            instant_use: false,
            tool_takes_paint: false,
            tools: vec![None; 5],
            cur_tool: None,
            tool_active: false,
            tool_name: String::new(),
            paint: Vec::new(),
            paint_rows: vec![9],
            paint_row: 0,
            paint_swatch: 0,
            paint_active: false,
            paint_name: String::new(),
            spray_index: 0,
            building_disabled: false,
            painting_disabled: false,
            brick_slide: Slide::default(),
            paint_slide: Slide::default(),
            tool_slide: Slide::default(),
            boxes_visible: true,
        }
    }

    // ------------------------------------------------------------ geometry

    /// Pixel width of the paint box columns (`%boxWidth`).
    pub fn paint_box_width(&self) -> i32 {
        self.paint_rows.len() as i32 * 17 + 1
    }
    /// `HUD_PaintBox.extent.x` = columns + 100 px label.
    pub fn paint_box_extent(&self) -> i32 {
        self.paint_box_width() + 100
    }
    fn brick_hide_dist(&self) -> i32 {
        if self.prefs.show_tooltips { 64 } else { 87 }
    }
    fn paint_hide_dist(&self) -> i32 {
        if self.prefs.show_tooltips {
            self.paint_box_extent() - 100 + 5
        } else {
            self.paint_box_extent() + 5
        }
    }
    fn tool_hide_dist(&self) -> i32 {
        let n = self.tools.len() as i32 * 64;
        if self.prefs.show_tooltips { n } else { n + 25 }
    }

    // ------------------------------------------------------- server state

    /// `createInvHud` state after a brick inventory update. The HUD starts
    /// hidden unless in brick mode.
    pub fn set_bricks(&mut self, bricks: Vec<Option<BrickInfo>>) {
        let mut b = bricks;
        b.resize(NUM_BRICK_SLOTS, None);
        self.bricks = b;
        self.cur_brick = self.cur_brick.filter(|&i| self.has_brick(i));
        if self.mode == ScrollMode::Bricks && !self.instant_use {
            self.apply_active_brick(self.cur_brick);
        } else if self.mode != ScrollMode::Bricks {
            self.set_active_inv(None);
        }
    }

    /// First HUD build for a session: boxes start in their hidden positions.
    pub fn reset_layout(&mut self) {
        self.brick_slide = Slide::default();
        self.paint_slide = Slide::default();
        self.tool_slide = Slide::default();
        if self.mode != ScrollMode::Bricks && self.prefs.hide_brick_box {
            self.brick_slide.offset = self.brick_hide_dist();
        }
        if self.mode != ScrollMode::Paint && self.prefs.hide_paint_box {
            self.paint_slide.offset = self.paint_hide_dist();
        }
        if self.mode != ScrollMode::Tools && self.prefs.hide_tool_box {
            self.tool_slide.offset = self.tool_hide_dist();
        }
    }

    /// `PlayGui::LoadPaint`: one column per division plus the FX column.
    /// Catalogs may arrive after entering Play. Keep hidden geometry aligned
    /// with the new width without resetting active modes or ongoing slides.
    pub fn set_colorset(&mut self, divisions: Vec<PaintDivision>) {
        let old_hide_dist = self.paint_hide_dist();
        self.paint_rows = divisions
            .iter()
            .map(|d| d.colors.len())
            .chain(std::iter::once(9))
            .collect();
        self.paint = divisions;
        if self.paint_row >= self.paint_rows.len() {
            self.paint_row = 0;
        }
        if self.mode != ScrollMode::Paint && self.prefs.hide_paint_box {
            self.paint_slide.offset += self.paint_hide_dist() - old_hide_dist;
        }
    }

    pub fn set_tools(&mut self, tools: Vec<Option<ToolInfo>>) {
        let hidden = self.mode != ScrollMode::Tools && self.prefs.hide_tool_box;
        let old_hide_dist = self.tool_hide_dist();
        self.tools = tools;
        if hidden {
            self.tool_slide.offset += self.tool_hide_dist() - old_hide_dist;
        }
        self.cur_tool = self.cur_tool.filter(|&i| self.has_tool(i));
        if self.mode == ScrollMode::Tools {
            self.apply_active_tool(self.cur_tool);
        } else {
            self.set_active_tool(None);
        }
    }

    /// Apply authoritative equipment state without generating `UseTool` or
    /// `UnUseTool`. Missing, empty or invalid slots are deselection. Clearing a
    /// tool leaves TOOLS mode but keeps PAINT and brick mode, as v20's HUD
    /// does: a can in hand has no tool slot.
    pub fn apply_active_tool(&mut self, slot: Option<usize>) {
        let slot = slot.filter(|&i| self.has_tool(i));
        self.cur_tool = slot;
        if slot.is_some() {
            self.change_scroll_mode(ScrollMode::Tools);
        } else if self.mode == ScrollMode::Tools {
            // No tool slot is exactly what holding a can means, so the
            // host confirming it must not leave PAINT: E would then only
            // re-enter it instead of shifting column.
            self.change_scroll_mode(ScrollMode::None);
        }
        self.set_active_tool(slot);
    }

    /// Apply authoritative brick selection without echoing `UseBrickSlot`.
    /// `None`, an empty slot or an out-of-range slot clears brick mode while
    /// leaving an unrelated tool/paint selection alone. Repeated assignments
    /// are idempotent; they do not use the user-command toggle behavior.
    pub fn apply_active_brick(&mut self, slot: Option<usize>) {
        let slot = slot.filter(|&i| self.has_brick(i));
        self.cur_brick = slot;
        if slot.is_some() {
            self.change_scroll_mode(ScrollMode::Bricks);
            self.instant_use = false;
        } else if self.mode == ScrollMode::Bricks {
            self.change_scroll_mode(ScrollMode::None);
        }
        self.set_active_inv(slot);
    }

    /// Flattened paint colour for an index (`getColorIDTable`).
    pub fn color(&self, index: u32) -> Option<[f32; 4]> {
        self.paint
            .iter()
            .flat_map(|d| d.colors.iter())
            .nth(index as usize)
            .copied()
    }

    /// HUD brick icon tint (`RecolorBrickIcons`: alpha clamped to ≥0.1).
    pub fn brick_icon_tint(&self) -> [f32; 4] {
        if !self.prefs.recolor_brick_icons {
            return [1.0; 4];
        }
        let c = self.color(self.spray_index).unwrap_or([1.0; 4]);
        [c[0], c[1], c[2], c[3].clamp(0.1, 1.0)]
    }

    /// Colour of the paint-can label (`updatePaintActive`: the swatch colour;
    /// FX swatches are white, "none" is 0.2 grey).
    pub fn paint_label_color(&self) -> [f32; 4] {
        if self.paint_row == self.paint_rows.len() - 1 {
            if self.paint_swatch == 0 {
                [0.2, 0.2, 0.2, 1.0]
            } else {
                [1.0; 4]
            }
        } else {
            self.paint
                .get(self.paint_row)
                .and_then(|d| d.colors.get(self.paint_swatch))
                .copied()
                .unwrap_or([1.0; 4])
        }
    }

    // ------------------------------------------------------------ modes

    /// `setScrollMode` (c:4852). Returns false if already in that mode.
    pub fn set_scroll_mode(&mut self, new: ScrollMode, out: &mut Outbox) -> bool {
        let keeps_tool =
            self.tool_takes_paint && self.mode == ScrollMode::Tools && new == ScrollMode::Paint;
        let unuse = matches!(self.mode, ScrollMode::Paint | ScrollMode::Tools)
            && !self.instant_use
            && !keeps_tool;
        let changed = self.change_scroll_mode(new);
        if changed && unuse {
            out.actions.push(UiAction::UnUseTool);
        }
        changed
    }

    /// `clientCmdSetScrollMode`: the host switches the boxes shown, without
    /// any request back (what is in hand stays).
    pub fn apply_scroll_mode(&mut self, mode: ScrollMode) {
        self.change_scroll_mode(mode);
        if mode == ScrollMode::Tools {
            self.set_active_tool(self.cur_tool);
        }
    }

    /// Shared visual transition. Network/host requests belong only to the
    /// user-command wrapper above, never to authoritative state application.
    fn change_scroll_mode(&mut self, new: ScrollMode) -> bool {
        if self.mode == new {
            return false;
        }
        match self.mode {
            ScrollMode::Bricks => {
                self.brick_active = false;
                self.brick_name.clear();
                if self.prefs.hide_brick_box {
                    let d = self.brick_hide_dist();
                    self.brick_slide.start(d, 10);
                }
            }
            ScrollMode::Paint => {
                self.paint_active = false;
                self.paint_name.clear();
                if self.prefs.hide_paint_box {
                    let d = self.paint_hide_dist();
                    self.paint_slide.start(d, 10);
                }
            }
            ScrollMode::Tools => {
                self.tool_active = false;
                self.tool_name.clear();
                if self.prefs.hide_tool_box {
                    let d = self.tool_hide_dist();
                    self.tool_slide.start(d, 20);
                }
            }
            ScrollMode::None => {}
        }
        self.mode = new;
        match new {
            ScrollMode::Bricks if self.prefs.hide_brick_box => {
                let d = self.brick_hide_dist();
                self.brick_slide.start(-d, 10);
            }
            ScrollMode::Paint if self.prefs.hide_paint_box => {
                let d = self.paint_hide_dist();
                self.paint_slide.start(-d, 1);
            }
            ScrollMode::Tools if self.prefs.hide_tool_box => {
                let d = self.tool_hide_dist();
                self.tool_slide.start(-d, 10);
            }
            _ => {}
        }
        self.instant_use = false;
        true
    }

    fn has_brick(&self, i: usize) -> bool {
        self.bricks.get(i).is_some_and(Option::is_some)
    }
    fn has_tool(&self, i: usize) -> bool {
        self.tools.get(i).is_some_and(Option::is_some)
    }

    fn set_active_inv(&mut self, index: Option<usize>) {
        match index {
            None => {
                self.brick_active = false;
                self.brick_name.clear();
            }
            Some(i) => {
                self.brick_active = true;
                self.brick_name = self.bricks[i]
                    .as_ref()
                    .map(|b| b.ui_name.clone())
                    .unwrap_or_default();
            }
        }
    }

    /// `directSelectInv` (c:3695). Returns false when there is nothing to use.
    pub fn direct_select_inv(
        &mut self,
        index: usize,
        open_bsd_key: &str,
        out: &mut Outbox,
    ) -> bool {
        if self.has_brick(index) {
            if self.mode == ScrollMode::Bricks {
                if self.cur_brick == Some(index) && self.brick_active {
                    self.set_active_inv(None);
                    out.actions.push(UiAction::UnUseTool);
                    self.set_scroll_mode(ScrollMode::None, out);
                } else {
                    self.set_active_inv(Some(index));
                    self.cur_brick = Some(index);
                    out.actions.push(UiAction::UseBrickSlot { slot: index });
                }
            } else {
                self.set_scroll_mode(ScrollMode::Bricks, out);
                self.set_active_inv(Some(index));
                self.cur_brick = Some(index);
                out.actions.push(UiAction::UseBrickSlot { slot: index });
            }
            return true;
        }
        // Empty slot: walk forward from the *current* scroll slot (the script
        // decrements `$CurrScrollBrickSlot`, not the pressed index).
        let mut cur = self.cur_brick.map_or(-1, |c| c as i32 - 1);
        for _ in 0..NUM_BRICK_SLOTS - 1 {
            cur += 1;
            if cur < 0 {
                cur = NUM_BRICK_SLOTS as i32 - 1;
            }
            if cur >= NUM_BRICK_SLOTS as i32 {
                cur = 0;
            }
            if self.has_brick(cur as usize) {
                break;
            }
        }
        let cur = cur.max(0) as usize;
        self.cur_brick = Some(cur);
        if self.has_brick(cur) {
            self.set_active_inv(Some(cur));
            out.actions.push(UiAction::UseBrickSlot { slot: cur });
        } else if let Some(b) = self.last_instant_use.clone() {
            out.actions.push(UiAction::InstantUseBrick {
                brick: b.id.clone(),
            });
            self.set_active_inv(None);
            self.set_scroll_mode(ScrollMode::Bricks, out);
            self.instant_use = true;
            self.brick_name = b.ui_name;
            return true;
        } else {
            let msg = if self.building_disabled {
                ("\u{E005}Building is currently disabled.".to_string(), 2.0)
            } else {
                (
                    format!(
                        "\u{E005}You don't have any bricks!\nPress {open_bsd_key} to open the brick selector."
                    ),
                    3.0,
                )
            };
            out.center_prints.push(msg);
            return false;
        }
        if self.mode != ScrollMode::Bricks {
            self.set_scroll_mode(ScrollMode::Bricks, out);
        }
        true
    }

    /// `useBricks` (key 1): the first brick in the bar, like 2-0 pick theirs.
    ///
    /// Deliberate change from v20 (`useBricks`@c:4380), which re-selected
    /// the *current* slot: once another slot had been used, 1 never reached
    /// the first brick and pressing it while holding a brick put the brick
    /// away. The HUD hint "Press 1 or 2 3 ... 0 to use bricks" groups 1 with
    /// the slot keys, so 1 now selects the first filled slot (an empty
    /// first slot is skipped) and, like the other slot keys, a second press
    /// deselects it. The wheel still returns to any other slot.
    pub fn use_bricks(&mut self, open_bsd_key: &str, out: &mut Outbox) {
        if self.building_disabled {
            out.center_prints
                .push(("\u{E005}Building is currently disabled.".into(), 2.0));
            return;
        }
        let first = (0..NUM_BRICK_SLOTS)
            .find(|&i| self.has_brick(i))
            .unwrap_or(0);
        self.direct_select_inv(first, open_bsd_key, out);
    }

    /// `scrollBricks` (c:4542).
    pub fn scroll_bricks(&mut self, dir: i32, out: &mut Outbox) {
        let dir = if self.prefs.reverse_brick_scroll {
            -dir
        } else {
            dir
        };
        let start = self.cur_brick;
        let mut cur: i32 = match self.cur_brick {
            Some(c) => c as i32,
            None if dir > 0 => -1,
            None => 1,
        };
        for _ in 0..NUM_BRICK_SLOTS - 1 {
            cur += dir;
            if cur < 0 {
                cur = NUM_BRICK_SLOTS as i32 - 1;
            }
            if cur >= NUM_BRICK_SLOTS as i32 {
                cur = 0;
            }
            if self.has_brick(cur as usize) {
                break;
            }
        }
        let cur = cur as usize;
        self.cur_brick = Some(cur);
        if self.has_brick(cur) && start != Some(cur) {
            self.set_active_inv(Some(cur));
            out.actions.push(UiAction::UseBrickSlot { slot: cur });
        }
    }

    /// `useTools` (Q).
    pub fn use_tools(&mut self, out: &mut Outbox) {
        if self.mode != ScrollMode::Tools {
            let n = self.tools.len();
            let start = self.cur_tool.unwrap_or(0);
            let Some(idx) = (0..n).map(|i| (i + start) % n).find(|&i| self.has_tool(i)) else {
                return;
            };
            self.cur_tool = Some(idx);
            self.set_scroll_mode(ScrollMode::Tools, out);
            self.set_active_tool(Some(idx));
            out.actions.push(UiAction::UseTool { slot: idx });
        } else {
            self.set_scroll_mode(ScrollMode::None, out);
        }
    }

    fn set_active_tool(&mut self, index: Option<usize>) {
        match index {
            None => {
                self.tool_active = false;
                self.tool_name.clear();
            }
            Some(i) => {
                self.tool_active = true;
                self.tool_name = self.tools[i]
                    .as_ref()
                    .map(|t| t.name.trim().to_string())
                    .unwrap_or_default();
            }
        }
    }

    /// `scrollTools` (c:4808).
    pub fn scroll_tools(&mut self, dir: i32, out: &mut Outbox) {
        let n = self.tools.len() as i32;
        if n == 0 {
            return;
        }
        let mut cur: i32 = match self.cur_tool {
            Some(c) => c as i32,
            None if dir > 0 => -1,
            None => 1,
        };
        for _ in 0..n {
            cur += dir;
            if cur < 0 {
                cur = n - 1;
            } else if cur >= n {
                cur = 0;
            }
            if self.has_tool(cur as usize) {
                break;
            }
        }
        self.cur_tool = Some(cur as usize);
        if self.has_tool(cur as usize) {
            self.set_active_tool(Some(cur as usize));
            out.actions.push(UiAction::UseTool { slot: cur as usize });
        } else {
            self.set_active_tool(None);
        }
    }

    /// Legacy scripted **use command**, including its outbound request. Native
    /// authoritative view-model updates must use `apply_active_tool` instead.
    pub fn server_set_active_tool(&mut self, slot: usize, out: &mut Outbox) {
        if !self.has_tool(slot) {
            return;
        }
        self.set_scroll_mode(ScrollMode::Tools, out);
        self.cur_tool = Some(slot);
        self.set_active_tool(Some(slot));
        out.actions.push(UiAction::UseTool { slot });
    }

    /// Legacy scripted **use command** (v20 switches to TOOLS mode first).
    /// Native authoritative updates must use `apply_active_brick` instead.
    pub fn server_set_active_brick(&mut self, slot: usize, key: &str, out: &mut Outbox) {
        if !self.has_brick(slot) {
            return;
        }
        self.set_scroll_mode(ScrollMode::Tools, out);
        self.cur_brick = Some(slot);
        if self.has_brick(slot) {
            self.direct_select_inv(slot, key, out);
        }
    }

    fn paint_name_for(&self) -> String {
        if self.paint_row == self.paint_rows.len() - 1 {
            format!("FX - {}", FX_NAMES[self.paint_swatch.min(8)])
        } else {
            let div = self
                .paint
                .get(self.paint_row)
                .map(|d| d.name.as_str())
                .unwrap_or("");
            format!("{div} - {}", self.paint_swatch + 1)
        }
    }

    fn apply_paint(&mut self, out: &mut Outbox) {
        if self.paint_row == self.paint_rows.len() - 1 {
            self.paint_name = self.paint_name_for();
            out.actions.push(UiAction::UseFxCan {
                fx: self.paint_swatch as u32,
            });
        } else {
            self.paint_name = self.paint_name_for();
            let idx: usize =
                self.paint_rows[..self.paint_row].iter().sum::<usize>() + self.paint_swatch;
            self.spray_index = idx as u32;
            out.actions
                .push(UiAction::UseSprayCan { color: idx as u32 });
        }
    }

    /// `shiftPaintColumn` (c:4590): E enters PAINT, then cycles columns.
    pub fn shift_paint_column(&mut self, dir: i32, out: &mut Outbox) {
        if self.mode != ScrollMode::Paint {
            self.set_scroll_mode(ScrollMode::Paint, out);
            self.paint_active = true;
        } else {
            let n = self.paint_rows.len() as i32;
            let mut r = self.paint_row as i32 + dir;
            if r >= n {
                r = 0;
            }
            if r < 0 {
                r = n - 1;
            }
            self.paint_row = r as usize;
        }
        if self.paint_row == self.paint_rows.len() - 1 {
            self.paint_swatch = self.paint_swatch.min(8);
        } else {
            let n = self.paint_rows[self.paint_row];
            if n == 0 {
                return;
            }
            self.paint_swatch = self.paint_swatch.min(n - 1);
        }
        self.apply_paint(out);
    }

    /// `useSprayCan` (E).
    pub fn use_spray_can(&mut self, out: &mut Outbox) {
        if self.painting_disabled {
            out.center_prints
                .push(("\u{E005}Painting is currently disabled.".into(), 2.0));
        } else {
            self.shift_paint_column(1, out);
        }
    }

    /// `scrollPaint` (c:4706): wheel moves within the column, wrapping.
    pub fn scroll_paint(&mut self, dir: i32, out: &mut Outbox) {
        let n = self.paint_rows[self.paint_row] as i32;
        let mut s = self.paint_swatch as i32 + dir;
        if s < 0 {
            s = n - 1;
        }
        if s >= n {
            s = 0;
        }
        self.paint_swatch = s.max(0) as usize;
        self.apply_paint(out);
    }

    /// `scrollInventory` after the zoom and dialog checks (wheel delta already
    /// mapped: `val < 0` → +1).
    pub fn scroll_inventory(&mut self, dir: i32, open_bsd_key: &str, out: &mut Outbox) {
        match self.mode {
            ScrollMode::Bricks => {
                if !self.brick_active {
                    let c = self.cur_brick.unwrap_or(0);
                    self.direct_select_inv(c, open_bsd_key, out);
                } else {
                    self.scroll_bricks(dir, out);
                }
            }
            ScrollMode::Paint => self.scroll_paint(dir, out),
            ScrollMode::Tools => self.scroll_tools(dir, out),
            ScrollMode::None => {
                if self.building_disabled {
                    self.set_scroll_mode(ScrollMode::Tools, out);
                    self.scroll_tools(1, out);
                    self.scroll_tools(-1, out);
                } else {
                    let c = self.cur_brick.unwrap_or(0);
                    if self.direct_select_inv(c, open_bsd_key, out) {
                        self.set_scroll_mode(ScrollMode::Bricks, out);
                    }
                }
            }
        }
    }

    /// Arrow keys in non-wheel schemes (`invUp`/`invDown`/`invLeft`/`invRight`).
    pub fn inv_key(&mut self, which: InvKey, key: &str, out: &mut Outbox) {
        let (vertical, val) = match which {
            InvKey::Up => (true, 1),
            InvKey::Down => (true, -1),
            InvKey::Left => (false, 1),
            InvKey::Right => (false, -1),
        };
        // scrollInventory(v) maps v<0 → +1 and v>=0 → -1.
        let dir = if val < 0 { 1 } else { -1 };
        match self.mode {
            ScrollMode::Paint if !vertical => self.shift_paint_column(-val, out),
            ScrollMode::None => {
                self.set_scroll_mode(ScrollMode::Bricks, out);
                self.scroll_inventory(dir, key, out);
            }
            _ => self.scroll_inventory(dir, key, out),
        }
    }

    /// Brick selector right-click (`BSD_RightClickIcon`).
    pub fn instant_use(&mut self, brick: BrickInfo, out: &mut Outbox) {
        out.actions.push(UiAction::InstantUseBrick {
            brick: brick.id.clone(),
        });
        self.instant_use = true;
        self.set_active_inv(None);
        self.set_scroll_mode(ScrollMode::Bricks, out);
        self.brick_name = brick.ui_name.clone();
        self.last_instant_use = Some(brick);
    }

    /// `BSD_BuyBricks` clears the HUD until the server's inventory arrives.
    pub fn clear_bricks_for_buy(&mut self) {
        self.bricks = vec![None; NUM_BRICK_SLOTS];
    }

    pub fn tick(&mut self, dt_ms: u64) {
        self.brick_slide.tick(dt_ms);
        self.paint_slide.tick(dt_ms);
        self.tool_slide.tick(dt_ms);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvKey {
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::IconRef;

    fn brick(n: &str) -> Option<BrickInfo> {
        Some(BrickInfo {
            id: n.into(),
            ui_name: n.into(),
            category: "Bricks".into(),
            subcategory: "2x".into(),
            icon: IconRef::None,
        })
    }

    fn model() -> HudModel {
        let mut h = HudModel::default();
        h.set_colorset(vec![
            PaintDivision {
                name: "Standard".into(),
                colors: vec![[1.0, 0.0, 0.0, 1.0]; 9],
            },
            PaintDivision {
                name: "Bold".into(),
                colors: vec![[0.0, 1.0, 0.0, 1.0]; 9],
            },
        ]);
        let mut b = vec![None; 10];
        b[0] = brick("2x4");
        b[3] = brick("1x1");
        h.set_bricks(b);
        h.set_tools(vec![
            Some(ToolInfo {
                id: "hammer".into(),
                name: "Hammer ".into(),
                icon: IconRef::None,
                tint: None,
            }),
            None,
            Some(ToolInfo {
                id: "wrench".into(),
                name: "Wrench".into(),
                icon: IconRef::None,
                tint: None,
            }),
        ]);
        h.reset_layout();
        h
    }

    #[test]
    fn colorset_resize_preserves_shown_and_animating_hidden_layouts() {
        let mut h = model();
        h.set_scroll_mode(ScrollMode::Paint, &mut Outbox::default());
        assert_eq!(h.paint_slide.offset, 0);
        h.set_colorset(vec![]);
        assert_eq!(h.paint_slide.offset, 0);
        h.set_scroll_mode(ScrollMode::None, &mut Outbox::default());
        assert!(h.paint_slide.moving());
        h.set_colorset(vec![PaintDivision {
            name: "new".into(),
            colors: vec![[1.0; 4]; 8],
        }]);
        assert!(h.paint_slide.moving());
        h.paint_slide.finish();
        assert_eq!(h.paint_slide.offset, h.paint_hide_dist());
        h.set_colorset(vec![]);
        assert_eq!(h.paint_slide.offset, h.paint_hide_dist());

        h.prefs.hide_paint_box = false;
        h.prefs.hide_tool_box = false;
        h.reset_layout();
        h.set_colorset(vec![PaintDivision {
            name: "visible".into(),
            colors: vec![[1.0; 4]],
        }]);
        h.set_tools(vec![None; 9]);
        assert_eq!(h.paint_slide.offset, 0);
        assert_eq!(h.tool_slide.offset, 0);
    }

    #[test]
    fn same_slot_twice_deselects() {
        let mut h = model();
        let mut o = Outbox::default();
        assert!(h.direct_select_inv(3, "B", &mut o));
        assert_eq!(h.mode, ScrollMode::Bricks);
        assert_eq!(h.brick_name, "1x1");
        assert_eq!(o.actions, vec![UiAction::UseBrickSlot { slot: 3 }]);
        o = Outbox::default();
        h.direct_select_inv(3, "B", &mut o);
        assert_eq!(h.mode, ScrollMode::None);
        assert!(!h.brick_active);
        assert_eq!(o.actions, vec![UiAction::UnUseTool]);
    }

    #[test]
    fn use_bricks_always_selects_the_first_brick() {
        // v20 re-selected the current slot, so after using slot 4 key 1
        // never returned to the first brick (and a second press put the
        // brick away instead).
        let mut h = model();
        let mut o = Outbox::default();
        h.direct_select_inv(3, "B", &mut o);
        o = Outbox::default();
        h.use_bricks("B", &mut o);
        assert_eq!(h.cur_brick, Some(0));
        assert_eq!(h.mode, ScrollMode::Bricks);
        assert_eq!(h.brick_name, "2x4");
        assert_eq!(o.actions, vec![UiAction::UseBrickSlot { slot: 0 }]);

        // From tool mode with slot 4 remembered, 1 still means slot 1.
        h.direct_select_inv(3, "B", &mut o);
        h.use_tools(&mut o);
        o = Outbox::default();
        h.use_bricks("B", &mut o);
        assert_eq!(h.cur_brick, Some(0));
        assert!(o.actions.contains(&UiAction::UseBrickSlot { slot: 0 }));

        // Pressing it again deselects, like the other slot keys.
        o = Outbox::default();
        h.use_bricks("B", &mut o);
        assert_eq!(h.mode, ScrollMode::None);
        assert_eq!(o.actions, vec![UiAction::UnUseTool]);

        // An empty first slot is skipped: the first brick in the bar wins,
        // not the slot after the remembered one.
        let mut b = vec![None; 10];
        b[2] = brick("1x2");
        b[6] = brick("1x6");
        h.set_bricks(b);
        h.direct_select_inv(6, "B", &mut Outbox::default());
        o = Outbox::default();
        h.use_bricks("B", &mut o);
        assert_eq!(h.cur_brick, Some(2));
        assert_eq!(o.actions, vec![UiAction::UseBrickSlot { slot: 2 }]);
    }

    #[test]
    fn empty_slot_walks_forward_and_empty_inventory_prints() {
        let mut h = model();
        let mut o = Outbox::default();
        h.cur_brick = Some(1);
        h.direct_select_inv(5, "B", &mut o);
        assert_eq!(h.cur_brick, Some(3));
        let mut empty = HudModel::default();
        let mut o = Outbox::default();
        assert!(!empty.direct_select_inv(4, "B", &mut o));
        assert!(
            o.center_prints[0]
                .0
                .contains("Press B to open the brick selector.")
        );
    }

    #[test]
    fn the_host_confirming_no_tool_keeps_paint_so_e_shifts_column() {
        // E from a held tool sends UnUseTool then the can. The host then
        // reports no tool slot, which is what a can in hand is; the next E
        // must shift column, not merely re-enter PAINT.
        let mut h = model();
        let mut o = Outbox::default();
        h.use_spray_can(&mut o);
        h.apply_active_tool(None);
        assert_eq!(h.mode, ScrollMode::Paint);
        h.use_spray_can(&mut o);
        assert_eq!(h.paint_row, 1);
        assert_eq!(h.paint_name, "Bold - 1");
    }

    #[test]
    fn paint_columns_and_fx() {
        let mut h = model();
        let mut o = Outbox::default();
        h.use_spray_can(&mut o);
        assert_eq!(h.mode, ScrollMode::Paint);
        assert_eq!(h.paint_name, "Standard - 1");
        assert_eq!(o.actions, vec![UiAction::UseSprayCan { color: 0 }]);
        h.scroll_paint(1, &mut o);
        h.scroll_paint(1, &mut o);
        assert_eq!(h.paint_name, "Standard - 3");
        h.use_spray_can(&mut o); // next column keeps swatch index
        assert_eq!(h.paint_name, "Bold - 3");
        assert_eq!(h.spray_index, 11);
        h.use_spray_can(&mut o);
        assert_eq!(h.paint_name, "FX - Chrome");
        assert_eq!(o.actions.last(), Some(&UiAction::UseFxCan { fx: 2 }));
        h.use_spray_can(&mut o); // wraps to first column
        assert_eq!(h.paint_row, 0);
        // Leaving paint sends UnUseTool; slides in instantly, out over 10 steps.
        let mut o = Outbox::default();
        assert_eq!(h.paint_slide.offset, 0);
        h.use_tools(&mut o);
        assert_eq!(o.actions[0], UiAction::UnUseTool);
        assert_eq!(o.actions[1], UiAction::UseTool { slot: 0 });
        assert_eq!(h.tool_name, "Hammer");
        assert!(h.paint_slide.moving());
        h.tick(200);
        assert_eq!(h.paint_slide.offset, h.paint_box_extent() - 100 + 5);
    }

    #[test]
    fn tools_scroll_skips_empty_and_q_toggles() {
        let mut h = model();
        let mut o = Outbox::default();
        h.use_tools(&mut o);
        h.scroll_tools(1, &mut o);
        assert_eq!(h.cur_tool, Some(2));
        h.scroll_tools(1, &mut o);
        assert_eq!(h.cur_tool, Some(0));
        h.use_tools(&mut o);
        assert_eq!(h.mode, ScrollMode::None);
    }

    #[test]
    fn slides_match_script_steps() {
        let mut s = Slide::default();
        s.start(64, 10);
        assert_eq!(s.offset, 6);
        s.tick(10);
        assert_eq!(s.offset, 12);
        s.tick(1000);
        assert_eq!(s.offset, 64);
        s.start(-64, 10);
        s.finish();
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn wheel_in_none_enters_bricks() {
        let mut h = model();
        let mut o = Outbox::default();
        h.scroll_inventory(1, "B", &mut o);
        assert_eq!(h.mode, ScrollMode::Bricks);
        assert!(h.brick_active);
        h.scroll_inventory(1, "B", &mut o);
        assert_eq!(h.cur_brick, Some(3));
    }

    #[test]
    fn authoritative_selection_is_idempotent_and_user_commands_still_emit() {
        let mut h = model();
        h.apply_active_tool(Some(2));
        assert_eq!(h.mode, ScrollMode::Tools);
        assert_eq!(h.cur_tool, Some(2));
        assert_eq!(h.tool_name, "Wrench");
        assert!(h.tool_active);
        let slide = h.tool_slide.clone();
        h.apply_active_tool(Some(2));
        assert_eq!(h.tool_slide, slide);
        h.apply_active_brick(Some(3));
        assert_eq!(h.mode, ScrollMode::Bricks);
        assert!(!h.tool_active);
        assert!(h.tool_name.is_empty());
        assert!(h.brick_active);
        assert_eq!(h.brick_name, "1x1");
        let slide = h.brick_slide.clone();
        h.apply_active_brick(Some(3));
        assert_eq!(h.brick_slide, slide);
        // Authoritative methods take no outbox. A subsequent actual user
        // command remains the sole source of the equipment requests here.
        let mut out = Outbox::default();
        h.use_tools(&mut out);
        assert_eq!(out.actions, vec![UiAction::UseTool { slot: 2 }]);
        h.apply_active_tool(None);
        assert_eq!(h.mode, ScrollMode::None);
        assert_eq!(h.cur_tool, None);
        assert!(!h.tool_active);
    }

    #[test]
    fn authoritative_invalid_slots_and_clear_preserve_unrelated_mode() {
        let mut h = model();
        for slot in [Some(1), Some(usize::MAX), None] {
            h.apply_active_tool(Some(0));
            h.apply_active_tool(slot);
            assert_eq!(h.cur_tool, None);
            assert_eq!(h.mode, ScrollMode::None);
            assert!(!h.tool_active);
            assert!(h.tool_name.is_empty());
            h.apply_active_brick(Some(3));
            h.apply_active_brick(slot);
            assert_eq!(h.cur_brick, None);
            assert_eq!(h.mode, ScrollMode::None);
            assert!(!h.brick_active);
            assert!(h.brick_name.is_empty());
        }
        h.apply_active_tool(Some(0));
        h.apply_active_brick(None);
        assert_eq!(h.mode, ScrollMode::Tools);
        assert!(h.tool_active);
        h.apply_active_brick(Some(0));
        h.apply_active_tool(None);
        assert_eq!(h.mode, ScrollMode::Bricks);
        assert!(h.brick_active);
    }

    #[test]
    fn inventory_shrink_empty_and_legacy_stale_indices_clear_safely() {
        let mut h = model();
        h.apply_active_tool(Some(2));
        h.set_tools(vec![h.tools[0].clone()]);
        assert_eq!(h.cur_tool, None);
        assert!(!h.tool_active);
        assert!(h.tool_name.is_empty());
        assert_eq!(h.mode, ScrollMode::None);
        h.apply_active_tool(Some(0));
        h.set_tools(vec![]);
        h.apply_active_tool(Some(0));
        assert_eq!(h.cur_tool, None);
        h.apply_active_brick(Some(3));
        h.set_bricks(vec![brick("2x4")]);
        assert_eq!(h.cur_brick, None);
        assert!(!h.brick_active);
        assert!(h.brick_name.is_empty());
        assert_eq!(h.mode, ScrollMode::None);
        h.cur_brick = Some(usize::MAX);
        h.cur_tool = Some(usize::MAX);
        h.set_bricks(vec![]);
        h.set_tools(vec![]);
        assert_eq!(h.cur_brick, None);
        assert_eq!(h.cur_tool, None);
        let mut out = Outbox::default();
        h.server_set_active_tool(usize::MAX, &mut out);
        h.server_set_active_brick(usize::MAX, "B", &mut out);
        assert_eq!(out, Outbox::default());
    }

    #[test]
    fn inventory_updates_refresh_active_labels_without_reselecting() {
        let mut h = model();
        h.apply_active_tool(Some(0));
        let mut tools = h.tools.clone();
        tools[0].as_mut().unwrap().name = " Hammer Updated ".into();
        h.set_tools(tools);
        assert_eq!(h.tool_name, "Hammer Updated");
        h.apply_active_brick(Some(0));
        h.set_bricks(vec![brick("2x4 Updated")]);
        assert_eq!(h.brick_name, "2x4 Updated");
        assert!(h.brick_active);
    }
}
