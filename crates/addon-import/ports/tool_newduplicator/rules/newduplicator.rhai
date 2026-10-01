// The New Duplicator (Zeblote's Tool_NewDuplicator), ported: what its
// server scripts did, on the player's own imported copy. Runs on the host.
//
//   /newduplicator (/duplicator, /dup, /d and the rest) puts it in your hand.
//   Stack mode: clicking a brick selects it and everything built on it, up
//   or down from it ([Next Seat]), limited to its side or not
//   ([Prev Seat]), and lights the selection up; holding Ctrl adds to the
//   selection. [Light] switches to box mode (turning a selection
//   into a box round it): clicking a brick puts a box around it, the brick
//   keys move its corner ([Rotate Brick] picks the other corner), and
//   [Plant Brick] selects what is in it; with Ctrl held a click grows the
//   box to the plate clicked, and the keys move and turn the whole box.
//   With a selection, the brick keys take it up as a ghost (plant mode):
//   they move it, clicking puts it against what you click, [Prev Seat]
//   turns it about the whole selection or its start brick, [Plant Brick]
//   plants it brick by brick and says what did not fit. [Cancel Brick]
//   drops the selection, then the box. Ctrl C copies, Ctrl V pastes and
//   Ctrl X cuts.
//   With a selection, a paint can is fill colour: [Plant Brick] paints it.
//   /FillWrench wrenches it all, /SuperCut cuts the box out and puts plain
//   bricks back over what stuck out, /FillBricks then fills the box,
//   /ForcePlant plants a ghost that floats, /MirrorX, /MirrorY and
//   /MirrorZ mirror it (/MirErrors names the bricks that had no mirror
//   image), /PlantAs plants into another player's bricks, /Cut cuts the
//   originals away, /SaveDup and /LoadDup keep and load selections by
//   name, /AllDups lists them, /DupVersion and /DupClients show versions,
//   and /ClearDups (admins) puts every duplicator away.

// Its preferences (`ndRegisterPrefs`): the server's New Duplicator
// settings, which the host changes in the Admin menu's Add-On Settings,
// read by their v20 globals. Their defaults are `ndApplyDefaultPrefValues`'.
fn nd_pref(name) {
    pref("$Pref::Server::ND::" + name)
}
fn admin_only() {
    nd_pref("AdminOnly")
}
fn save_admin_only() {
    nd_pref("SaveAdminOnly")
}
fn load_admin_only() {
    nd_pref("LoadAdminOnly")
}
// "Trust Limit": the trust a selection needs in a brick's owner.
fn trust() {
    switch nd_pref("TrustLimit") {
        0 => "none",
        1 => "build",
        3 => "self",
        _ => "full",
    }
}
// "Admin Trust Bypass" when selecting.
fn admin_bypass() {
    nd_pref("AdminTrustBypass1")
}
fn select_public() {
    nd_pref("SelectPublicBricks")
}
// "Max Bricks", up to the most one copy holds (the original's own admin
// limit). A big selection, plant, cut, paint, wrench, load or undo goes a
// slice each tick, as the original's did, with its progress below.
fn max_bricks(admin) {
    let n = nd_pref(if admin { "MaxBricksAdmin" } else { "MaxBricksPlayer" });
    if n > 1000000 { 1000000 } else { n }
}
// "Max Box Size", units on a side, as large as the original's own limit.
fn max_box(admin) {
    let n = nd_pref(if admin { "MaxBoxSizeAdmin" } else { "MaxBoxSizePlayer" });
    if n > 1024 { 1024.0 } else { n.to_float() }
}
// "Select Timeout", non-admins only.
fn select_wait() {
    nd_pref("SelectTimeoutMS") * second() / 1000
}
// "Planting Timeout", non-admins only, in seconds.
fn plant_wait_seconds(admin) {
    if admin { 0.0 } else { nd_pref("PlantTimeoutMS") / 1000.0 }
}
// "Admin Trust Bypass" when planting as another.
fn admin_plant_bypass() {
    nd_pref("AdminTrustBypass2")
}
// `$ND::Version`.
fn version() {
    "{{nd_version}}"
}
// "Enable Menu Sounds".
fn menu_sounds() {
    nd_pref("PlayMenuSounds")
}
// "Process Per Tick": a plant of more than ten ticks' worth sounds.
fn process_per_tick() {
    300
}
// Longest save name.
fn name_length() {
    {{name_length}}
}
// Admin-only features.
fn paint_admin_only() {
    nd_pref("PaintAdminOnly")
}
fn paint_fx_admin_only() {
    nd_pref("PaintFxAdminOnly")
}
fn wrench_admin_only() {
    nd_pref("WrenchAdminOnly")
}
fn float_admin_only() {
    nd_pref("FloatAdminOnly")
}
fn fill_bricks_admin_only() {
    nd_pref("FillBricksAdminOnly")
}

// Simulation ticks per second.
fn second() {
    120
}

fn tool() {
    "{{namespace}}:weapon/nd_item"
}

// The duplicator's look in each mode: gold to select, the spinning box
// for a box, blue to plant; fill colour keeps the one in hand.
fn image_of(mode) {
    switch mode {
        "box" => "{{namespace}}:image/nd_image_box",
        "plant" => "{{namespace}}:image/nd_image_blue",
        "fillcolor" => (),
        _ => "{{namespace}}:image/nd_image",
    }
}

fn s(count) {
    if count == 1 { "" } else { "s" }
}

fn coded(text) {
    text.starts_with("\\c")
}

// `ndFormatMessage`: a title over rows of text on the left and right.
fn format_message(title, rows) {
    let message = "<font:Arial:22>";
    if !coded(title) {
        message += `\c6`;
    }
    message += title + "\n<font:Verdana:16>";
    let right = false;
    for row in rows {
        let l = row[0];
        let r = row[1];
        if l == "" && r == "" {
            break;
        }
        if l != "" {
            if right {
                message += "<just:left>";
            }
            if !coded(l) {
                message += `\c6`;
            }
            message += l;
            right = false;
        }
        if r != "" {
            if !right {
                message += "<just:right>";
            }
            if !coded(r) {
                message += `\c6`;
            }
            message += r + " ";
            right = true;
        }
        message += "\n";
    }
    message + " "
}

fn limited_text(p) {
    if get_player(p, "limited") { `Limited: \c3Yes \c6[Prev Seat]` } else { `Limited: \c0No \c6[Prev Seat]` }
}

// The box's two corners, `[x1, y1, z1, x2, y2, z2]`, as lowest and
// highest.
fn lower(a, b) {
    if a < b { a } else { b }
}
fn upper(a, b) {
    if a > b { a } else { b }
}
fn box_min(b) {
    [lower(b[0], b[3]), lower(b[1], b[4]), lower(b[2], b[5])]
}
fn box_max(b) {
    [upper(b[0], b[3]), upper(b[1], b[4]), upper(b[2], b[5])]
}

// One of a box's twelve edges (`ND_SelectionBoxBorder`, 0.05 across at
// scale 1): along `axis`, through corner `p1` or `p2` on each other axis
// as `ND_SelectionBox::setSize` numbers them (0 through `p1`, 2 through
// `p2`), `width` its scale and `grow` added to its length.
fn edge(p1, p2, axis, k, width, grow, color) {
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let at = [0.0, 0.0, 0.0];
    at[u] = if k == 1 || k == 2 { p2[u] } else { p1[u] };
    at[v] = if k == 2 || k == 3 { p2[v] } else { p1[v] };
    let half = width * 0.025;
    let min = [at[0] - half, at[1] - half, at[2] - half];
    let max = [at[0] + half, at[1] + half, at[2] + half];
    min[axis] = lower(p1[axis], p2[axis]) - grow * 0.5;
    max[axis] = upper(p1[axis], p2[axis]) + grow * 0.5;
    #{ min: min, max: max, color: color }
}
// A cube `size` across at `at`, its faces across x, y and z shaded.
fn cube(at, size, sides) {
    let h = size * 0.5;
    #{ min: [at[0] - h, at[1] - h, at[2] - h], max: [at[0] + h, at[1] + h, at[2] + h], sides: sides }
}
fn longest(len) {
    upper(upper(len[0], len[1]), len[2])
}

// `ND_SelectionBox`, seen by everyone: see-through faces, gold edges (the
// selected corner's in blue), shaded corner cubes and "Name's Selection
// Box" over it; once it has selected, grey edges alone
// (`setDisabledMode`). Edges and corners grow with the box.
fn draw_selection_box(p) {
    let b = get_player(p, "box");
    if b.len() != 6 {
        hide_shapes(p, "box");
        return;
    }
    let p1 = [b[0], b[1], b[2]];
    let p2 = [b[3], b[4], b[5]];
    let lo = box_min(b);
    let hi = box_max(b);
    let len = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let shapes = [];
    if get_player(p, "box_selected") {
        let width = (21.0 / 5120.0) * longest(len) + 1.0;
        let grey = [0.1, 0.1, 0.1, 0.4];
        for axis in 0..3 {
            // Upright edges a little longer, the rest shorter, so their
            // ends do not fight.
            let grow = if axis == 1 { width * 0.05 } else { -width * 0.05 };
            for k in 0..4 {
                shapes.push(edge(p1, p2, axis, k, width, grow, grey));
            }
        }
        show_shapes(p, "box", shapes);
        return;
    }
    let gold = [1.0, 0.84, 0.0, 0.99];
    let blue = [0.0, 0.0, 1.0, 0.99];
    // The second corner selected (`selectedCorner`), else the first.
    let second = get_player(p, "corner");
    if len[0] > 0.02 && len[1] > 0.02 && len[2] > 0.02 {
        shapes.push(#{ min: [lo[0] + 0.01, lo[1] + 0.01, lo[2] + 0.01], max: [hi[0] - 0.01, hi[1] - 0.01, hi[2] - 0.01], color: [0.0, 0.0, 0.0, 0.0], inside: [0.0, 0.0, 0.0, 0.6] });
    }
    // `hasVolume`: a flat box has no outer faces.
    if len[0] >= 0.05 && len[1] >= 0.05 && len[2] >= 0.05 {
        shapes.push(#{ min: [lo[0] - 0.01, lo[1] - 0.01, lo[2] - 0.01], max: [hi[0] + 0.01, hi[1] + 0.01, hi[2] + 0.01], color: [0.0, 0.0, 0.0, 0.35] });
    }
    let width = (7.0 / 1024.0) * longest(len) + 1.0;
    // The edges through the corners a little thicker, the selected one's
    // more, against the highlight box over the same place.
    let width1 = if second { width } else { width + 0.02 };
    let width2 = if second { width + 0.02 } else { width };
    for axis in 0..3 {
        for k in 0..4 {
            let shape = if k == 0 {
                edge(p1, p2, axis, k, width1, -width * 0.05, if second { gold } else { blue })
            } else if k == 2 {
                edge(p1, p2, axis, k, width2, -width * 0.05, if second { blue } else { gold })
            } else {
                edge(p1, p2, axis, k, width, width * 0.05, gold)
            };
            shapes.push(shape);
        }
    }
    // Shaded across Torque's x, y and z: here x, up and z.
    let plain = [gold, [1.0, 0.94, 0.1, 0.99], [0.8, 0.74, 0.0, 0.99]];
    let picked = [blue, [0.0, 0.1, 0.9, 0.99], [0.0, 0.2, 1.0, 0.99]];
    shapes.push(cube(p1, 0.35 * width1, if second { plain } else { picked }));
    shapes.push(cube(p2, 0.35 * width2, if second { picked } else { plain }));
    let name = player(p).name;
    let label = if name.ends_with("s") || name.ends_with("S") { name + "' Selection Box" } else { name + "'s Selection Box" };
    let top = [(lo[0] + hi[0]) * 0.5, hi[1], (lo[2] + hi[2]) * 0.5];
    shapes.push(#{ min: top, max: top, color: [1.0, 0.84, 0.0, 0.0], label: label });
    show_shapes(p, "box", shapes);
}

// `ND_HighlightBox`, seen by everyone: gold edges round a selection, as
// wide as the selection is big; blue round the ghost in plant mode
// (`spawnGhostBricks`, `getGhostWorldBox`), following it as it moves.
fn draw_highlight_box(p) {
    let mode = get_player(p, "mode");
    let area = if mode == "plant" { get_player(p, "ghost_box") } else { get_player(p, "selection_box") };
    let held = mode == "plant" || ((mode == "stack" || mode == "box") && get_player(p, "selected") > 0);
    if !held || area.len() != 6 {
        hide_shapes(p, "highlight");
        return;
    }
    let p1 = [area[0], area[1], area[2]];
    let p2 = [area[3], area[4], area[5]];
    let len = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
    let width = (7.0 / 1280.0) * longest(len) + 1.0;
    // `ND_HighlightBox::applyColors`.
    let color = if mode == "plant" { [0.2, 0.2, 1.0, 0.99] } else { [1.0, 0.84, 0.0, 0.99] };
    let shapes = [];
    for axis in 0..3 {
        for k in 0..4 {
            shapes.push(edge(p1, p2, axis, k, width, width * 0.05, color));
        }
    }
    show_shapes(p, "highlight", shapes);
}

// The ghost moved, turned, mirrored or flipped, or went away: the box
// round it (`ND_Selection::updateHighlightBox`).
fn on_copy_ghost(p, info) {
    set_player(p, "ghost_box", if info.box == () { [] } else { info.box.min + info.box.max });
    draw_highlight_box(p);
}

fn draw_boxes(p) {
    draw_selection_box(p);
    draw_highlight_box(p);
}

// The progress modes' `getBottomPrint`: a big job goes a slice a tick.
fn progress_text(p) {
    let progress = get_player(p, "progress");
    let done = progress[0];
    let total = progress[1];
    let percent = if total > 0 { done * 100 / total } else { 0 };
    let queued = progress[4];
    let searched = progress[5];
    let lines = switch get_player(p, "working") {
        "select" => {
            if total > 0 {
                [`Processing... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel selection"]
            } else if searched >= 0 {
                // `NDM_BoxSelectProgress`'s.
                [`Searching... (\c3${searched}%\c6, \c3${done}\c6 Bricks)`, "[Cancel Brick]: Cancel selection"]
            } else {
                // `NDM_StackSelectProgress`'s.
                [`Selecting... (\c3${done}\c6 Bricks, \c3${queued}\c6 in Queue)`, "[Cancel Brick]: Cancel selection"]
            }
        }
        // `NDM_PlantCopyProgress`'s: a later pass looks for bricks that
        // now have something under them.
        "plant" => {
            let planted = progress[2];
            if searched >= 0 {
                [`Finding Next Brick... (\c3${searched}%\c6, \c3${planted}\c6 planted)`, "[Cancel Brick]: Cancel planting"]
            } else {
                let failed = progress[3];
                let percent = if total > 0 { planted * 100 / total } else { 0 };
                [`Planting... (\c3${percent}%\c6, \c3${failed}\c6 failed)`, "[Cancel Brick]: Cancel planting"]
            }
        }
        "cut" => [`Cutting... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel cut"],
        "paint" => [`Painting... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel painting"],
        "wrench" => [`Applying... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel"],
        "load" => [`Loading Bricks... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel loading"],
        // `NDM_SuperCutProgress`'s.
        "supercut" => {
            let deleted = done - progress[3];
            let planted = progress[2];
            [`Supercut in progress... (\c3${percent}%\c6, \c3${deleted}\c6 deleted, \c3${planted}\c6 planted)`, "[Cancel Brick]: Cancel supercut"]
        }
        // The original filled at once, with no progress line: ours.
        "fill" => [`Filling in bricks... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel fill"],
        _ => [`Working... (\c3${percent}%\c6)`, "[Cancel Brick]: Cancel"],
    };
    format_message(lines[0], [[lines[1], ""]])
}

// Each mode's `getBottomPrint`.
fn bottom(p) {
    if get_player(p, "working") != "" {
        bottom_print(p, progress_text(p), 0);
        return;
    }
    let mode = get_player(p, "mode");
    let text = switch mode {
        "stack" => {
            let count = get_player(p, "selected");
            let up = get_player(p, "direction");
            let way = if up { "Up" } else { "Down" };
            let title = "Selection Mode";
            let r0 = "Click Brick: Select stack " + way.to_lower();
            let r1 = "";
            if count > 0 {
                title = `Selection Mode (\c3${count}\c6 Brick` + if count > 1 { "s)" } else { ")" };
                r0 = "Ctrl-Click Brick: Multiselect";
                r1 = "[Plant Brick]: Duplicate";
            }
            let multi = if get_player(p, "multi") { "Multi-" } else { "" };
            format_message(title, [
                [`Type: \c3${multi}Stack \c6[Light]`, r0],
                [limited_text(p), r1],
                [`Direction: \c3${way} \c6[Next Seat]`, ""],
            ])
        }
        "box" => {
            let count = get_player(p, "selected");
            let title = "Selection Mode";
            if count > 0 {
                title = `Selection Mode (\c3${count}\c6 Brick` + if count > 1 { "s)" } else { ")" };
            }
            let b = get_player(p, "box");
            let l2 = "";
            let r0 = "Click Brick: Place selection box";
            let r1 = "";
            if b.len() == 6 {
                let x = ((b[3] - b[0]).abs() * 2.0).round().to_int();
                let y = ((b[5] - b[2]).abs() * 2.0).round().to_int();
                let z = ((b[4] - b[1]).abs() * 5.0).round().to_int();
                l2 = `Size: \c3${x}\c6 x \c3${y}\c6 x \c3${z}\c6 Plates`;
                if get_player(p, "box_selected") {
                    r0 = "[Cancel Brick]: Adjust box";
                    r1 = "[Plant Brick]: Duplicate";
                } else {
                    r0 = "[Shift Brick]: Move corner";
                    r1 = "[Rotate Brick]: Switch corner";
                }
            }
            format_message(title, [
                [`Type: \c3Box \c6[Light]`, r0],
                [limited_text(p), r1],
                [l2, ""],
            ])
        }
        "fillcolor" => {
            let count = get_player(p, "selected");
            let title = `Paint Mode (\c3${count}\c6 Brick` + if count > 1 { "s)" } else { ")" };
            let fill = get_player(p, "fill");
            let color = if fill[0] {
                `\c3Fx - ` + ["None", "Pearl", "Chrome", "Glow", "Blink", "Swirl", "Rainbow", "Stable", "Undulo"][fill[1]]
            } else {
                // `ndGetPaintColorCode`: a swatch of the colour, and how
                // see-through it is.
                let swatch = "<font:impact:20>" + paint_code(fill[1]) + `|||||<font:Verdana:16>\c3`;
                let alpha = (palette()[fill[1]][3] * 100.0).floor().to_int();
                if alpha != 100 {
                    swatch += ` ${alpha}%`;
                }
                swatch
            };
            format_message(title, [
                ["Select paint can to chose color", "[Plant Brick]: Paint bricks"],
                ["Color: " + color, "[Cancel Brick]: Exit mode"],
            ])
        }
        "plant" => {
            let count = get_player(p, "selected");
            let ghosted = get_player(p, "ghosted");
            let title = if count == 1 {
                `Plant Mode (\c31\c6 Brick)`
            } else if ghosted >= count {
                `Plant Mode (\c3${count}\c6 Bricks)`
            } else {
                `Plant Mode (\c3${count}\c6 Bricks, \c3${ghosted * 100 / count}%\c6 Ghosted)`
            };
            let size = get_player(p, "size");
            let pivot = if get_player(p, "pivot") { "Whole Selection" } else { "Start Brick" };
            let l1 = "";
            if get_player(p, "plant_as") != "" {
                l1 = `Planting as: \c3` + get_player(p, "plant_as");
            } else if size.len() == 3 {
                l1 = `Size: \c3${size[0]}\c6 x \c3${size[2]}\c6 x \c3${size[1]}\c6 Plates`;
            }
            format_message(title, [
                [`Pivot: \c3${pivot}\c6 [Prev Seat]`, "Use normal ghost brick controls"],
                [l1, "[Cancel Brick] to exit plant mode"],
            ])
        }
        _ => "",
    };
    bottom_print(p, text, 0);
}

// A smaller line under a centre print's first.
fn line(text) {
    "\n<font:Verdana:17>" + text
}

fn oops(p, text, seconds) {
    center_print(p, `<font:Verdana:20>\c6` + text, seconds);
}

// A sound of the duplicator's menus (`PlayMenuSounds`): `MsgUploadStart`
// and the rest, the light switches.
fn menu_sound(p, sound) {
    if menu_sounds() {
        play_sound(p, sound);
    }
}

// `ndTrustCheckMessage`'s refusal.
fn no_trust(p) {
    play_sound(p, "errorSound");
    oops(p, "You don't have enough trust to do that!", 5);
}

// `ndGetPaintColorCode`: palette colour `index` as a colour tag.
fn paint_code(index) {
    let c = palette()[index];
    let hex = "0123456789abcdef";
    let tag = "<color:";
    for k in 0..3 {
        let v = (c[k] * 255.0).round().to_int();
        tag += hex.sub_string(v / 16, 1) + hex.sub_string(v % 16, 1);
    }
    tag + ">"
}

// Put the image for `mode` in hand; the mount and unmount it causes are
// not the duplicator coming out or going away (`ndIgnoreNextMount`).
fn show_image(p, mode) {
    let image = image_of(mode);
    if image != () && player(p).image != image {
        set_player(p, "swapping", true);
        mount_image(p, image);
    }
}

// `ndSetMode`, with what leaving and starting each mode did.
fn set_mode(p, mode) {
    let old = get_player(p, "mode");
    if old == mode {
        bottom(p);
        return;
    }
    set_player(p, "mode", mode);
    if old == "box" && (mode == "stack" || mode == "plant") {
        set_player(p, "box", []);
        draw_boxes(p);
    }
    if old == "fillcolor" {
        // The paint box goes, the duplicator's tools come back.
        scroll_mode(p, "tools");
    }
    if mode == "fillcolor" {
        // Leaving a selection mode for paint, the glow ends.
        highlight_copy(p, (), 0.0);
    }
    if old == "plant" && (mode == "stack" || mode == "box") {
        // Leaving plant mode lets the selection go.
        set_player(p, "selected", 0);
        drop_copy(p);
    }
    if (old == "stack" || old == "box") && mode == "plant" {
        // Taken up as a ghost where it was selected.
        show_copy(p);
        float_copy(p, get_player(p, "force_plant"), #{ admin_only: float_admin_only() });
        set_player(p, "plant_as", "");
    }
    if mode == "plant" {
        pivot_copy(p, if get_player(p, "pivot") { "whole" } else { "start" });
        plant_wait(p, plant_wait_seconds(player(p).admin));
    }
    if mode == "stack" || mode == "box" {
        set_player(p, "last_select", mode);
        if old == "stack" || old == "box" {
            set_player(p, "box_selected", false);
            if mode == "box" {
                box_from_selection(p);
            }
            // A selection made in the other mode is let go.
            set_player(p, "selected", 0);
            drop_copy(p);
        }
    }
    show_image(p, mode);
    take_paints(p);
    draw_boxes(p);
    bottom(p);
}

// While a selection is held in a selection mode, the paint cans pick its
// fill colour instead of coming out (`serverCmdUseSprayCan`).
fn take_paints(p) {
    let mode = get_player(p, "mode");
    let take = (mode == "stack" || mode == "box" || mode == "fillcolor")
        && get_player(p, "selected") > 0;
    if take != get_player(p, "paints") {
        set_player(p, "paints", take);
        take_paint(p, take);
    }
}

// `NDM_BoxSelect::onStartMode` from stack mode: a box round the selection,
// if the box may be that big.
fn box_from_selection(p) {
    let area = get_player(p, "selection_box");
    if get_player(p, "selected") == 0 || area.len() != 6 {
        return;
    }
    let me = player(p);
    let limit = max_box(me.admin);
    let low = [area[0], area[1], area[2]];
    let high = [area[3], area[4], area[5]];
    if high[0] - low[0] <= limit && high[1] - low[1] <= limit && high[2] - low[2] <= limit {
        set_player(p, "box", aligned(me, low, high));
        set_player(p, "corner", true);
        draw_boxes(p);
    } else {
        let studs = (limit * 2.0).floor().to_int();
        center_print(p, `<font:Verdana:20>\c6Oops!` + "\n<font:Verdana:17>" + `\c6Your selection box is limited to \c3${studs} \c6studs.`, 5);
    }
}

// A job started or ended: the progress modes' `spin`, which `ndSetMode`
// gave the image (`setImageLoaded(0, !%newMode.spin)`), so the duplicator
// spins while it works.
fn set_working(p, action) {
    if get_player(p, "working") != action {
        set_player(p, "working", action);
        set_image_loaded(p, action == "");
    }
}

// `ndKillMode`: the duplicator put away. A job under way stops with it
// (each progress mode's `onKillMode`), and what it did stays done.
fn kill_mode(p) {
    if busy(p) {
        set_working(p, "");
        cancel_copy(p);
    }
    if get_player(p, "mode") == "fillcolor" {
        scroll_mode(p, "tools");
    }
    set_player(p, "mode", "");
    set_player(p, "selected", 0);
    set_player(p, "box", []);
    set_player(p, "box_selected", false);
    draw_boxes(p);
    drop_copy(p);
    take_paints(p);
    bottom_print(p, "", 0);
}

// `serverCmdNewDuplicator`.
fn cmd_newduplicator(p) {
    let me = player(p);
    if admin_only() && !me.admin {
        tell(p, `\c6The new duplicator is admin only. Ask an admin for help.`);
        return;
    }
    if !me.alive {
        tell(p, `\c6You must be spawned to equip the new duplicator.`);
        return;
    }
    give_item(p, tool(), true);
}

// `/duplicator` down to `/d`, which it took over from older duplicators.
fn cmd_duplicator(p) {
    cmd_newduplicator(p);
}
fn cmd_duplicato(p) {
    cmd_newduplicator(p);
}
fn cmd_duplicat(p) {
    cmd_newduplicator(p);
}
fn cmd_duplica(p) {
    cmd_newduplicator(p);
}
fn cmd_duplic(p) {
    cmd_newduplicator(p);
}
fn cmd_dupli(p) {
    cmd_newduplicator(p);
}
fn cmd_dupl(p) {
    cmd_newduplicator(p);
}
fn cmd_dup(p) {
    cmd_newduplicator(p);
}
fn cmd_du(p) {
    cmd_newduplicator(p);
}
fn cmd_d(p) {
    cmd_newduplicator(p);
}

// `ND_Image::onMount` and `Player::ndEquipped`.
fn cmd_mount(p) {
    if get_player(p, "swapping") {
        set_player(p, "swapping", false);
        return;
    }
    let me = player(p);
    if admin_only() && !me.admin {
        oops(p, "Oops! The duplicator is admin only.", 5);
        return;
    }
    if get_player(p, "mode") == "" {
        set_mode(p, get_player(p, "last_select"));
    }
}

// `ND_Image::onUnMount` and `Player::ndUnEquipped`.
fn cmd_unmount(p) {
    if get_player(p, "swapping") {
        return;
    }
    if get_player(p, "mode") != "" {
        kill_mode(p);
    }
}

// The spark where the swing lands, blue in plant mode.
fn spark(p, x, y, z) {
    let projectile = if player(p).image == image_of("plant") {
        "{{namespace}}:projectile/nd_hitprojectile_blue"
    } else {
        "{{namespace}}:projectile/nd_hitprojectile"
    };
    fire(projectile, x, y, z, 0.0, 0.0, 0.0, p);
}

// The select timeout (`ndLastSelectTime`), for non-admins.
fn waited(p, me) {
    let last = get_player(p, "last_select_tick");
    if !me.admin && tick() - last < select_wait() {
        let left = (select_wait() - (tick() - last) + second() - 1) / second();
        oops(p, `You need to wait\c3 ${left}\c6 second${s(left)} before selecting again!`, 5);
        return false;
    }
    set_player(p, "last_select_tick", tick());
    true
}

// Ctrl, held: the original's multiselect key (`/NdMultiSelect`).
fn multi(p) {
    get_player(p, "multi")
}

fn copy_rule(p, me) {
    #{
        trust: trust(),
        public_bricks: select_public(),
        admin: admin_bypass(),
        // `ndTrustCheckSelect`: trust in the owner of the brick's stack
        // counts too.
        stack: true,
        partial: true,
        limited: get_player(p, "limited"),
        // Held as a selection until a brick key takes it up.
        hidden: true,
        add: get_player(p, "mode") == "stack" && multi(p),
    }
}

// The way the player faces, along x or z, and to their right.
fn facing(me) {
    if me.lx.abs() > me.lz.abs() {
        [if me.lx < 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0]
    } else {
        [0.0, 0.0, if me.lz < 0.0 { -1.0 } else { 1.0 }]
    }
}

// `ND_SelectionBox::setSizeAligned`: the second corner, the one moved
// first, is the top one nearest the player on their right.
fn aligned(me, low, high) {
    let f = facing(me);
    // Right of facing f is (-f.z, 0, f.x).
    let right = [-f[2], 0.0, f[0]];
    let near = [low[0], high[1], low[2]];
    let far = [high[0], low[1], high[2]];
    for a in [0, 2] {
        let pick_high = if f[a] != 0.0 { f[a] < 0.0 } else { right[a] > 0.0 };
        near[a] = if pick_high { high[a] } else { low[a] };
        far[a] = if pick_high { low[a] } else { high[a] };
    }
    [far[0], far[1], far[2], near[0], near[1], near[2]]
}

// The duplicator's swing (`Player::ndFired` and each mode's
// `onSelectObject`).
fn cmd_fire(p) {
    let mode = get_player(p, "mode");
    if mode == "" {
        return;
    }
    let me = player(p);
    if mode == "fillcolor" {
        return;
    }
    if mode == "plant" {
        // Put the selection against what was clicked.
        let hit = raycast([me.ex, me.ey, me.ez], [me.lx, me.ly, me.lz], {{reach}} * 1.0, p);
        if hit == () {
            return;
        }
        spark(p, hit.x, hit.y, hit.z);
        move_copy(p, [hit.x, hit.y, hit.z], [hit.nx, hit.ny, hit.nz]);
        return;
    }
    let hit = aim();
    if hit == () {
        return;
    }
    spark(p, hit.x, hit.y, hit.z);
    if hit.brick == () {
        return;
    }
    if mode == "stack" {
        if !waited(p, me) {
            return;
        }
        let way = if get_player(p, "direction") { "up" } else { "down" };
        copy_build(p, hit.brick, max_bricks(me.admin), way, tool(), copy_rule(p, me));
        menu_sound(p, "uploadStartSound");
        return;
    }
    // Box mode: a box around the clicked brick, one the player may select
    // (`ndTrustCheckMessage`).
    if !may_copy(p, hit.brick, copy_rule(p, me)) {
        no_trust(p);
        return;
    }
    if get_player(p, "box_selected") {
        oops(p, "Selection canceled! You can now edit the box again.", 5);
        set_player(p, "box_selected", false);
        set_player(p, "selected", 0);
        drop_copy(p);
        take_paints(p);
    }
    let b = brick_box(hit.brick);
    if b == () {
        return;
    }
    let corners = get_player(p, "box");
    if multi(p) {
        // Grown to take in the plate clicked (`ndGetPlateBoxFromRayCast`).
        b = plate_box(hit);
        if corners.len() == 6 {
            let low = box_min(corners);
            let high = box_max(corners);
            for a in 0..3 {
                b.min[a] = lower(b.min[a], low[a]);
                b.max[a] = upper(b.max[a], high[a]);
            }
        }
    }
    set_player(p, "box", aligned(me, b.min, b.max));
    set_player(p, "corner", true);
    let corners = get_player(p, "box");
    draw_boxes(p);
    bottom(p);
}

// The plate the ray hit, one step into what it hit.
fn plate_box(hit) {
    let at = [hit.x, hit.y, hit.z];
    let normal = [hit.nx, hit.ny, hit.nz];
    let cell = [0.5, 0.2, 0.5];
    let min = [0.0, 0.0, 0.0];
    let max = [0.0, 0.0, 0.0];
    for a in 0..3 {
        let v = at[a] - cell[a] * 0.5 * normal[a].round();
        min[a] = (v / cell[a]).floor() * cell[a];
        max[a] = (v / cell[a]).ceiling() * cell[a];
        if max[a] - min[a] < 0.01 {
            max[a] = min[a] + cell[a];
        }
    }
    #{ min: min, max: max }
}

// A step along the floor turned as the player faces.
fn turned_step(me, x, y) {
    let f = facing(me);
    let left = [f[2], 0.0, -f[0]];
    [(f[0] * x + left[0] * y) * 0.5, (f[2] * x + left[2] * y) * 0.5]
}

// [Light]: stack and box mode.
fn cmd_light(p) {
    switch get_player(p, "mode") {
        "stack" => {
            menu_sound(p, "lightOnSound");
            set_mode(p, "box");
        }
        "box" => {
            menu_sound(p, "lightOffSound");
            set_mode(p, "stack");
        }
    }
}

// [Next Seat] and [Prev Seat].
fn cmd_seat(p, step) {
    let mode = get_player(p, "mode");
    let key = if step > 0 && mode == "stack" {
        "direction"
    } else if step < 0 && (mode == "stack" || mode == "box") {
        "limited"
    } else if step < 0 && mode == "plant" {
        "pivot"
    } else {
        return;
    };
    let on = !get_player(p, key);
    set_player(p, key, on);
    if key == "pivot" {
        pivot_copy(p, if on { "whole" } else { "start" });
    }
    bottom(p);
    menu_sound(p, if on { "lightOnSound" } else { "lightOffSound" });
}

// A selection held in stack or box mode, taken up by a brick key.
fn selection_held(p) {
    let mode = get_player(p, "mode");
    (mode == "stack" || mode == "box") && get_player(p, "selected") > 0
        && (mode == "stack" || get_player(p, "box_selected"))
}

// [Shift Brick] and [Super Shift Brick]: with a selection, take it up and
// move it; on the box, move its corner (or, Ctrl held, the whole box).
fn cmd_shift(p, x, y, z, super_shift) {
    if selection_held(p) {
        set_mode(p, "plant");
        shift_copy(p, x, y, z, super_shift);
        return;
    }
    if get_player(p, "mode") != "box" || get_player(p, "box_selected") {
        return;
    }
    let b = get_player(p, "box");
    if b.len() != 6 {
        return;
    }
    let me = player(p);
    if super_shift {
        x *= {{super_studs}};
        y *= {{super_studs}};
        z *= {{super_plates}};
    }
    // Studs away and to the left as the player faces, plates up.
    let step = turned_step(me, x, y);
    let offset = [step[0], z.to_float() * 0.2, step[1]];
    if multi(p) {
        // `ND_SelectionBox::shift`: the whole box moves.
        let moved = b;
        for a in 0..3 {
            moved[a] += offset[a];
            moved[a + 3] += offset[a];
        }
        play_sound(p, "brickMoveSound");
        set_player(p, "box", moved);
        draw_boxes(p);
        return;
    }
    let limit = max_box(me.admin);
    let reached = false;
    let moved = b;
    let second = get_player(p, "corner");
    for a in 0..3 {
        if second {
            let size = moved[a + 3] - moved[a] + offset[a];
            moved[a + 3] += offset[a];
            if size.abs() > limit {
                reached = true;
                moved[a + 3] -= size - limit * size.abs() / size;
            }
        } else {
            let size = moved[a + 3] - moved[a] - offset[a];
            moved[a] += offset[a];
            if size.abs() > limit {
                reached = true;
                moved[a] += size - limit * size.abs() / size;
            }
        }
    }
    play_sound(p, if moved == b { "errorSound" } else { "brickMoveSound" });
    set_player(p, "box", moved);
    draw_boxes(p);
    if reached {
        let studs = (limit * 2.0).floor().to_int();
        center_print(p, `<font:Verdana:20>\c6Oops!` + "\n<font:Verdana:17>" + `\c6Your selection box is limited to \c3${studs} \c6studs.`, 5);
    }
    bottom(p);
}

// [Rotate Brick]: with a selection, take it up and turn it; on the box,
// the other corner (or, Ctrl held, the whole box turned).
fn cmd_rotate(p, dir) {
    if selection_held(p) {
        set_mode(p, "plant");
        rotate_copy(p, dir);
        return;
    }
    if get_player(p, "mode") != "box" || get_player(p, "box_selected") {
        return;
    }
    let b = get_player(p, "box");
    if b.len() != 6 {
        return;
    }
    if multi(p) {
        let turned = rotate_box(b, dir);
        play_sound(p, "brickRotateSound");
        set_player(p, "box", turned);
        draw_boxes(p);
        bottom(p);
        return;
    }
    set_player(p, "corner", !get_player(p, "corner"));
    play_sound(p, "brickRotateSound");
    draw_selection_box(p);
}

// `ND_SelectionBox::rotate`: the box turned a quarter about its middle,
// kept on the stud grid.
fn rotate_box(b, dir) {
    let mid = [(b[0] + b[3]) * 0.5, (b[2] + b[5]) * 0.5];
    let sx = ((b[3] - b[0]) * 2.0).abs().round().to_int();
    let sz = ((b[5] - b[2]) * 2.0).abs().round().to_int();
    let correct = if sx % 2 == sz % 2 { 0.0 } else if sx % 2 == 1 { -0.25 } else { 0.25 };
    let out = b;
    for c in [0, 3] {
        let dx = b[c] - mid[0];
        let dz = b[c + 2] - mid[1];
        // A quarter turn clockwise seen from above, or back.
        let turned = if dir > 0 { [-dz, dx] } else { [dz, -dx] };
        out[c] = mid[0] + turned[0] + correct;
        out[c + 2] = mid[1] + turned[1] + correct;
    }
    out
}

// [Plant Brick]: with a selection, take it up as a ghost; on the box,
// select what is in it; in fill colour, paint the selection.
fn cmd_plant(p) {
    if selection_held(p) {
        set_mode(p, "plant");
        return;
    }
    if get_player(p, "mode") == "fillcolor" {
        fill_color(p);
        return;
    }
    if get_player(p, "mode") != "box" || get_player(p, "box_selected") {
        return;
    }
    let b = get_player(p, "box");
    if b.len() != 6 {
        return;
    }
    let me = player(p);
    if !waited(p, me) {
        return;
    }
    copy_box(p, box_min(b), box_max(b), max_bricks(me.admin), tool(), copy_rule(p, me));
    menu_sound(p, "uploadStartSound");
}

// [Cancel Brick].
fn cmd_cancel(p) {
    // A progress mode's `onCancelBrick`.
    if get_player(p, "working") != "" || player(p).copy_working {
        cancel_copy(p);
        return;
    }
    switch get_player(p, "mode") {
        "stack" => {
            set_player(p, "selected", 0);
            drop_copy(p);
            take_paints(p);
            draw_boxes(p);
            bottom(p);
        }
        "fillcolor" => {
            set_mode(p, get_player(p, "last_select"));
        }
        "box" => {
            if get_player(p, "box").len() != 6 {
                return;
            }
            if get_player(p, "box_selected") {
                oops(p, "Selection canceled! You can now edit the box again.", 5);
                set_player(p, "box_selected", false);
            } else {
                set_player(p, "box", []);
                draw_boxes(p);
            }
            set_player(p, "selected", 0);
            drop_copy(p);
            take_paints(p);
            draw_boxes(p);
            bottom(p);
        }
        "plant" => {
            set_mode(p, get_player(p, "last_select"));
        }
    }
}

fn holds_selection(p) {
    get_player(p, "mode") == "plant" || get_player(p, "selected") > 0
}

// `GameConnection::ndMirror`: the selection in plant mode, else the
// player's ghost brick (`FxDtsBrick::ndMirrorGhost`).
fn mirror(p, axis) {
    if get_player(p, "mode") == "plant" {
        mirror_copy(p, axis);
        return;
    }
    if player(p).ghost {
        let asymmetric = if axis == "y" {
            `\c6Sorry, your ghost brick is not vertically symmetric and cannot be mirrored.`
        } else {
            `\c6Sorry, your ghost brick is asymmetric and cannot be mirrored.`
        };
        mirror_ghost(p, axis, asymmetric);
        return;
    }
    tell(p, `\c6The mirror command can only be used in plant mode or with a ghost brick.`);
}
// `/MirrorX`: left and right as the player sees it.
fn cmd_mirrorx(p) {
    mirror(p, "view");
}
// `/MirrorY`: front and back as the player sees it.
fn cmd_mirrory(p) {
    let f = facing(player(p));
    mirror(p, if f[0] != 0.0 { "x" } else { "z" });
}
// `/MirrorZ`: up and down.
fn cmd_mirrorz(p) {
    mirror(p, "y");
}
fn cmd_mx(p) {
    cmd_mirrorx(p);
}
fn cmd_my(p) {
    cmd_mirrory(p);
}
fn cmd_mz(p) {
    cmd_mirrorz(p);
}

// A paint or FX can picked while the selection takes them
// (`serverCmdUseSprayCan`, `serverCmdUseFxCan`): fill colour mode with it.
fn cmd_paint(p, fx, index) {
    let mode = get_player(p, "mode");
    if !(mode == "stack" || mode == "box" || mode == "fillcolor") || get_player(p, "selected") == 0 {
        return;
    }
    set_player(p, "fill", [fx, index]);
    if mode == "fillcolor" {
        bottom(p);
    } else {
        set_mode(p, "fillcolor");
    }
}

// `NDM_FillColor::onPlantBrick`: paint the selection.
fn fill_color(p) {
    let me = player(p);
    if paint_admin_only() && !me.admin {
        tell(p, `\c6Paint Mode is admin only. Ask an admin for help.`);
        return;
    }
    let fill = get_player(p, "fill");
    if !fill[0] {
        paint_copy(p, #{ color: fill[1] });
        return;
    }
    if paint_fx_admin_only() && !me.admin {
        tell(p, `\c6Paint Fx Mode is admin only. Ask an admin for help.`);
        return;
    }
    if fill[1] < 7 {
        paint_copy(p, #{ color_effect: fill[1] });
    } else {
        paint_copy(p, #{ shape_effect: fill[1] - 7 });
    }
}

// `/FillWrench` (`/FW`): the wrench on every selected brick.
fn cmd_fillwrench(p) {
    if wrench_admin_only() && !player(p).admin {
        tell(p, `\c6Fill Wrench is admin only. Ask an admin for help.`);
        return;
    }
    let mode = get_player(p, "mode");
    if mode != "stack" && mode != "box" {
        tell(p, `\c6Fill Wrench can only be used in Selection Mode.`);
        return;
    }
    if get_player(p, "selected") == 0 {
        tell(p, `\c6Fill Wrench can only be used with a selection.`);
        return;
    }
    wrench_copy(p);
}
fn cmd_fw(p) {
    cmd_fillwrench(p);
}

// Why the box may not be supercut or filled now, or ().
fn box_refusal(p, what) {
    if get_player(p, "box").len() != 6 {
        return `\c6${what} can only be used with a selection box.`;
    }
    if get_player(p, "box_selected") {
        return `\c6${what} can not be used with any bricks selected.`;
    }
    ()
}

// `/SuperCut` (`/SC`): asks first.
fn cmd_supercut(p) {
    if get_player(p, "mode") != "box" {
        tell(p, `\c6Supercut can only be used on box selection mode.`);
        return;
    }
    let refused = box_refusal(p, "Supercut");
    if refused != () {
        tell(p, refused);
        return;
    }
    ask(p, "New Duplicator | Supercut", "Supercut deletes every brick reaching into the box\nand puts plain bricks back over what stuck out.\n\nPlease make sure the box is correct,\nthen press OK below.", "ndconfirmsupercut");
}
fn cmd_sc(p) {
    cmd_supercut(p);
}
fn cmd_ndconfirmsupercut(p) {
    if get_player(p, "mode") != "box" {
        tell(p, `\c6Supercut can only be used on box selection mode.`);
        return;
    }
    let refused = box_refusal(p, "Supercut");
    if refused != () {
        tell(p, refused);
        return;
    }
    set_player(p, "fill_after_cut", false);
    super_cut_box(p);
}

fn super_cut_box(p) {
    let b = get_player(p, "box");
    super_cut(p, box_min(b), box_max(b));
    menu_sound(p, "uploadStartSound");
}

// `/FillBricks` (`/FB`): a supercut, then the box filled with plain bricks
// in the player's paint colour. Asks first.
fn fill_bricks_refusal(p) {
    if fill_bricks_admin_only() && !player(p).admin {
        return `\c6Fill Bricks is admin only. Ask an admin for help.`;
    }
    if get_player(p, "mode") != "box" {
        return `\c6The fillBricks command can only be used with a selection box.`;
    }
    let refused = box_refusal(p, "The fillBricks command");
    if refused != () {
        return refused;
    }
    let b = get_player(p, "box");
    for a in 0..3 {
        if (b[a + 3] - b[a]).abs() < 0.05 {
            return `\c6The fillBricks command can only be used with a selection box that has a volume.`;
        }
    }
    ()
}
fn cmd_fillbricks(p) {
    let refused = fill_bricks_refusal(p);
    if refused != () {
        tell(p, refused);
        return;
    }
    ask(p, "New Duplicator | /FillBricks", "/FillBricks will first do a Supercut\nbefore placing bricks, to fix overlap.\n\nPlease make sure the box is correct,\nthen press OK below to continue.", "ndconfirmfillbricks");
}
fn cmd_fb(p) {
    cmd_fillbricks(p);
}
fn cmd_ndconfirmfillbricks(p) {
    let refused = fill_bricks_refusal(p);
    if refused != () {
        tell(p, refused);
        return;
    }
    set_player(p, "fill_after_cut", true);
    super_cut_box(p);
}

// `/ForcePlant` (`/FP`): plant the ghost once, floating bricks and all.
fn cmd_forceplant(p) {
    if get_player(p, "mode") != "plant" {
        tell(p, `\c6Force Plant can only be used in Plant Mode.`);
        return;
    }
    if float_admin_only() && !player(p).admin {
        tell(p, `\c6Force Plant is admin only. Ask an admin for help.`);
        return;
    }
    plant_copy(p, #{ float: true });
}
fn cmd_fp(p) {
    cmd_forceplant(p);
}

// `/ToggleForcePlant` (`/TFP`): every plant floats until toggled back.
fn cmd_toggleforceplant(p) {
    if float_admin_only() && !player(p).admin {
        tell(p, `\c6Force Plant is admin only. Ask an admin for help.`);
        return;
    }
    let on = !get_player(p, "force_plant");
    set_player(p, "force_plant", on);
    if get_player(p, "mode") == "plant" {
        float_copy(p, on, #{ admin_only: float_admin_only() });
    }
    if on {
        tell(p, `\c6Force Plant has been enabled. Use \c3/toggleForcePlant\c6 to disable it.`);
    } else {
        tell(p, `\c6Force Plant has been disabled. Use \c3/toggleForcePlant\c6 to enable it again.`);
    }
}
fn cmd_tfp(p) {
    cmd_toggleforceplant(p);
}

// `/Cut` (`/NdCut`, Ctrl+X): the selected bricks cut away, held to plant.
// A box with nothing selected yet selects first (`NDM_BoxSelect::onCut`).
fn cmd_ndcut(p) {
    let mode = get_player(p, "mode");
    if mode == "" {
        return;
    }
    if busy(p) || !(mode == "stack" || mode == "box") {
        not_here(p, "Cut");
        return;
    }
    if mode == "box" {
        // `NDM_BoxSelect::onCut`: a box not yet selected selects first.
        if get_player(p, "box").len() != 6 {
            return;
        }
        if !get_player(p, "box_selected") {
            cmd_plant(p);
            return;
        }
    }
    if get_player(p, "selected") == 0 {
        return;
    }
    // `ND_Selection::startCutting`: each brick the player may change goes,
    // the rest are counted.
    cut_copy(p, #{ each: true });
    menu_sound(p, "uploadStartSound");
}

// `NewDuplicatorMode::onCopy`, `onPaste` and `onCut`: a mode without them.
fn not_here(p, what) {
    tell(p, `\c6${what} can not be used in your current duplicator mode.`);
}
// A progress mode: copy work of theirs is running.
fn busy(p) {
    get_player(p, "working") != "" || player(p).copy_working
}

// Ctrl+C (`/NdCopy`): in a selection mode, as [Plant Brick] there.
fn cmd_ndcopy(p) {
    let mode = get_player(p, "mode");
    if mode == "" {
        return;
    }
    if busy(p) || !(mode == "stack" || mode == "box") {
        not_here(p, "Copy");
        return;
    }
    cmd_plant(p);
}

// Ctrl+V (`/NdPaste`): in plant mode, plant the ghost.
fn cmd_ndpaste(p) {
    let mode = get_player(p, "mode");
    if mode == "" {
        return;
    }
    if busy(p) || mode != "plant" {
        not_here(p, "Paste");
        return;
    }
    plant_copy(p);
}

// Ctrl, held and let go (`/NdMultiSelect`).
fn cmd_ndmultiselect(p, on) {
    set_player(p, "multi", on);
    let mode = get_player(p, "mode");
    if mode == "stack" || mode == "box" {
        bottom(p);
    }
}
fn cmd_cut(p) {
    cmd_ndcut(p);
}

// The save name rule both `/SaveDup` and `/LoadDup` kept.
fn save_name(p, typed) {
    let name = typed;
    name.trim();
    let allowed = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ._-()";
    let bad = name == "" || name.len() > name_length();
    for c in name.chars() {
        if !allowed.contains(c) {
            bad = true;
        }
    }
    let file = if bad { () } else { copy_name(name) };
    if file == () {
        tell(p, `\c6Bad save name "\c3${name}\c6", please try again.`);
        tell(p, `\c6Only \c3a-z A-Z 0-9 ._-()\c6 and \c3space\c6 are allowed, with a max length of ${name_length()} characters.`);
    }
    file
}

// `/SaveDup <name>`.
fn cmd_savedup(p, typed) {
    let me = player(p);
    let last = get_player(p, "last_save_tick");
    if !me.admin && tick() - last < 10 * second() {
        let left = (10 * second() - (tick() - last) + second() - 1) / second();
        tell(p, `\c6Please wait\c3 ${left}\c6 second${s(left)} before saving again!`);
        return;
    }
    if save_admin_only() && !me.admin {
        tell(p, `\c6Saving duplications is admin only. Ask an admin for help.`);
        return;
    }
    if !holds_selection(p) {
        tell(p, `\c6Saving duplications can only be used in Plant Mode.`);
        return;
    }
    let name = save_name(p, typed);
    if name == () {
        return;
    }
    // A save already there is kept unless the command comes again.
    let again = get_player(p, "save_overwrite") == name;
    set_player(p, "save_overwrite", name);
    save_copy(p, name, #{ overwrite: again });
    menu_sound(p, "uploadStartSound");
}
fn cmd_sd(p, typed) {
    cmd_savedup(p, typed);
}

// `/LoadDup <name>`.
fn cmd_loaddup(p, typed) {
    let me = player(p);
    let last = get_player(p, "last_load_tick");
    if !me.admin && tick() - last < 5 * second() {
        let left = (5 * second() - (tick() - last) + second() - 1) / second();
        tell(p, `\c6Please wait\c3 ${left}\c6 second${s(left)} before loading again!`);
        return;
    }
    if load_admin_only() && !me.admin {
        tell(p, `\c6Loading duplications is admin only. Ask an admin for help.`);
        return;
    }
    if get_player(p, "mode") == "" {
        cmd_newduplicator(p);
    }
    let name = save_name(p, typed);
    if name == () {
        return;
    }
    tell(p, `\c6Loading selection from "\c3${name}\c6"...`);
    load_copy(p, name, max_bricks(player(p).admin), tool(), #{ partial: true });
    menu_sound(p, "uploadStartSound");
}
fn cmd_ld(p, typed) {
    cmd_loaddup(p, typed);
}

// `/DupHelp`. Chat here shows plain text, so the original's tab stops
// and small spacer lines are left out.
fn cmd_duphelp(p) {
    let rule = `\c7--------------------------------------------------------------------------------`;
    for line in [
        " ",
        `\c6You can use the following commands with your new duplicator:`,
        rule,
        `\c3/Duplicator\c6 Equip a new duplicator!`,
        `\c3/ForcePlant\c6 Plant a selection in mid air; bricks can float.`,
        `\c3/ToggleForcePlant\c6 Enable force plant for normal planting, so you dont have to type it all the time.`,
        `\c3/PlantAs\c6 [\c3target\c6] Plant bricks in a different brick group. Target can be a name or blid.`,
        `\c3/FillWrench\c6 Open the fill wrench gui to change settings on all selected bricks.`,
        `\c3/MirrorX\c6 Mirror your ghost selection left/right on screen.`,
        `\c3/MirrorY\c6 Mirror your ghost selection front/back on screen.`,
        `\c3/MirrorZ\c6 Mirror your ghost selection up/down on screen.`,
        `\c3/MirErrors\c6 List potential mirror errors after planting a mirrored ghost selection.`,
        `\c3/SuperCut\c6 Delete everything in your selection box, cutting bricks in half on its sides!`,
        `\c3/FillBricks\c6 First supercut, then completely fill your selection box with few bricks.`,
        `\c3/SaveDup\c6 [\c3name\c6] Save your current selection to a file.`,
        `\c3/LoadDup\c6 [\c3name\c6] Load a selection from a file. Your current selection will be deleted.`,
        `\c3/AllDups\c6 [\c3filter\c6] Show all known saved duplications that match the filter. Leave blank to show all.`,
        `\c3/DupVersion\c6 Show the duplicator and blockland versions running on the server.`,
        `\c3/DupClients\c6 Show the duplicator versions of other clients on the server.`,
        rule,
        `\c6All of the commands can be shortened by just typing a \c3/\c6 and the capital letters!`,
        `\c6You might have to use \c3PageUp\c6/\c3PageDown\c6 to see all of them.`,
        " ",
    ] {
        tell(p, line);
    }
}
fn cmd_dh(p) {
    cmd_duphelp(p);
}

// `/AllDups` (`/AD`) [filter]: the saved duplications whose names
// contain the filter, in order.
fn cmd_alldups(p, typed) {
    if load_admin_only() && !player(p).admin {
        tell(p, `\c6Loading duplications is admin only. Ask an admin for help.`);
        return;
    }
    let filter = typed;
    filter.trim();
    let allowed = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-()";
    for c in filter.chars() {
        if !allowed.contains(c) {
            tell(p, `\c6Bad pattern "\c3${filter}\c6", please try again.`);
            tell(p, `\c6Only \c3a-z A-Z 0-9 ._-()\c6 are allowed.`);
            return;
        }
    }
    set_player(p, "list_filter", filter);
    list_copies(p, filter);
}
fn cmd_ad(p, typed) {
    cmd_alldups(p, typed);
}

// `/DupVersion` (`/DV`). Every player's game has the duplicator's own
// part (its keys), so theirs is the server's.
fn cmd_dupversion(p) {
    tell(p, `\c6Blockland version: \c3r` + game_version());
    tell(p, `\c6New duplicator version: \c3` + version());
    tell(p, `\c6Your new duplicator version: \c3` + version());
}
fn cmd_dv(p) {
    cmd_dupversion(p);
}

// `/DupClients` (`/DC`).
fn cmd_dupclients(p) {
    tell(p, `\c6New duplicator versions:`);
    for other in players() {
        tell(p, `\c3${other.name}\c6 has \c3` + version());
    }
}
fn cmd_dc(p) {
    cmd_dupclients(p);
}

// `/MirErrors` (`/ME`): the bricks the last mirrored plant had no exact
// mirror image for.
fn cmd_mirerrors(p) {
    let errors = get_player(p, "mirror_errors");
    let side = errors[0];
    let upside = errors[1];
    if side.len() > 0 {
        tell(p, " ");
        tell(p, `\c6These bricks are asymmetric and probably mirrored incorrectly:`);
        let i = 0;
        for name in side {
            i += 1;
            tell(p, `\c7 -${i}- \c6` + name);
        }
    }
    if upside.len() > 0 {
        tell(p, " ");
        tell(p, `\c6These bricks are not vertically symmetric and probably incorrect:`);
        let i = 0;
        for name in upside {
            i += 1;
            tell(p, `\c7 -${i}- \c6` + name);
        }
    }
    if side.len() == 0 && upside.len() == 0 {
        tell(p, `\c6There were no mirror errors in your last plant attempt.`);
    }
}
fn cmd_me(p) {
    cmd_mirerrors(p);
}

// `/ClearDups` (admins): every duplicator put away.
fn cmd_cleardups(p) {
    if !player(p).admin {
        tell(p, `\c6Canceling all duplicators is admin only. Ask an admin for help.`);
        return;
    }
    broadcast(`\c3${player(p).name}\c0 canceled all duplicators.`);
    for other in players() {
        if get_player(other.id, "mode") != "" {
            kill_mode(other.id);
        }
    }
}

// `/PlantAs` (`/PA`) [target]: plant into another brick group, named or
// by BL_ID; nothing plants into the player's own again.
fn cmd_plantas(p, typed) {
    if get_player(p, "mode") != "plant" {
        tell(p, `\c6This command can only be used in Plant Mode.`);
        return;
    }
    plant_as(p, typed, admin_plant_bypass());
}
fn cmd_pa(p, typed) {
    cmd_plantas(p, typed);
}

// A selection, cut, save, load, paint, wrench, supercut or fill finished.
fn on_copy(p, info) {
    if info.working {
        if info.action == "undo" {
            // `ndTickUndo`'s message.
            let percent = if info.total > 0 { info.bricks * 100 / info.total } else { 0 };
            center_print(p, `<font:Verdana:20>\c6Undo in progress...` + line(`\c3${percent}%\c6 finished.`), 10);
            return;
        }
        let was = get_player(p, "working");
        set_working(p, info.action);
        set_player(p, "progress", [info.bricks, info.total, info.placed, info.refused, info.queued, info.searched]);
        if was != info.action {
            if info.action == "plant" && info.total > process_per_tick() * 10 {
                // `ND_Selection::startPlant`: a big plant sounds.
                menu_sound(p, "uploadStartSound");
            } else if info.action == "wrench" {
                // Leaving a selection mode for the wrench, the glow ends.
                highlight_copy(p, (), 0.0);
            }
        }
        bottom(p);
        return;
    }
    set_working(p, "");
    if info.error == "canceled" {
        canceled(p, info);
        return;
    }
    if info.action == "select" || info.action == "load" {
        set_player(p, "ghosted", info.ghosted);
    }
    switch info.action {
        "undone" => {
            center_print(p, `<font:Verdana:20>\c6Undo finished.`, 2);
            return;
        }
        "paint" => {
            if info.error != () {
                oops(p, info.message, 5);
                return;
            }
            let msg = `<font:Verdana:20>\c6Painted \c3${info.bricks}\c6 Brick${s(info.bricks)}!`;
            if info.refused > 0 {
                msg += line(`\c3${info.refused}\c6 missing trust.`);
            }
            center_print(p, msg, 8);
            bottom(p);
            return;
        }
        "wrench" => {
            if info.error != () {
                tell(p, `\c6Fill Wrench can only be used with a selection.`);
                return;
            }
            highlight_copy(p, (), 0.0);
            let msg = `<font:Verdana:20>\c6Applied changes to \c3${info.bricks}\c6 Brick${s(info.bricks)}!`;
            if info.refused > 0 {
                msg += line(`\c3${info.refused}\c6 missing trust.`);
            }
            center_print(p, msg, 8);
            bottom(p);
            return;
        }
        "supercut" => {
            if info.error != () {
                set_player(p, "fill_after_cut", false);
                oops(p, info.message, 5);
                return;
            }
            menu_sound(p, "uploadEndSound");
            let msg = `<font:Verdana:20>\c6Deleted \c3${info.bricks}\c6 Brick${s(info.bricks)}!`;
            if info.placed > 0 {
                msg += line(`\c6Placed \c3${info.placed}\c6 new one${s(info.placed)}.`);
            }
            if info.refused > 0 {
                msg += line(`\c3${info.refused}\c6 missing trust.`);
            }
            center_print(p, msg, 12);
            if get_player(p, "fill_after_cut") {
                set_player(p, "fill_after_cut", false);
                if info.refused > 0 {
                    tell(p, `\c6Cannot run fill bricks, you do not have enough trust bricks already in the area.`);
                } else if get_player(p, "box").len() == 6 {
                    let b = get_player(p, "box");
                    fill_box(p, box_min(b), box_max(b), player(p).paint);
                }
            }
            bottom(p);
            return;
        }
        "fill" => {
            if info.error != () {
                oops(p, info.message, 5);
                return;
            }
            tell(p, `\c6Filled in \c3${info.bricks}\c6 brick${s(info.bricks)}`);
            return;
        }
        "save" => {
            if info.error == () {
                set_player(p, "save_overwrite", "");
                tell(p, `\c6Saving selection to "\c3${info.name}\c6"...`);
                tell(p, `\c6Finished saving selection, wrote \c3${info.bricks}\c6 Brick${s(info.bricks)}!`);
                set_player(p, "last_save_tick", tick());
            } else if info.error == "exists" {
                tell(p, `\c6Save "\c3${info.name}\c6" already exists. Repeat the command to overwrite.`);
            } else if info.error == "empty" {
                tell(p, `\c6Saving duplications can only be used in Plant Mode.`);
            } else {
                tell(p, `\c6Failed to write save "\c3${info.name}\c6". Ask the host for help.`);
            }
            return;
        }
        "load" => {
            if info.error == "missing" {
                tell(p, `\c6Save "\c3${info.name}\c6" does not exist, please try again.`);
                return;
            }
            if info.error != () {
                tell(p, `\c6Failed to read save "\c3${info.name}\c6". Ask the host for help.`);
                return;
            }
            tell(p, `\c6Finished loading selection, got \c3${info.bricks}\c6 Brick${s(info.bricks)}!`);
            set_player(p, "last_load_tick", tick());
            set_player(p, "plant_as", "");
            set_player(p, "selected", info.bricks);
            set_player(p, "size", if info.size == () { [] } else { info.size });
            set_mode(p, "plant");
            return;
        }
        "list" => {
            let filter = get_player(p, "list_filter");
            let count = info.names.len();
            let what = if filter == "" { "" } else { ` for filter "\c3${filter}\c6"` };
            if info.error != () {
                tell(p, `\c6Failed to list saved duplications. Ask the host for help.`);
            } else if count > 0 {
                let are = if count == 1 { " is" } else { "s are" };
                tell(p, `\c3${count}\c6 saved duplication${are} available${what}:`);
                for name in info.names {
                    tell(p, `\c6 - \c3` + name);
                }
            } else {
                tell(p, `\c6No saved duplications are available${what}.`);
            }
            tell(p, `\c6Scroll using \c3PageUp\c6 and \c3PageDown\c6 if you can't see the whole list.`);
            return;
        }
        "plant_as" => {
            let name = if info.name == () { "" } else { info.name };
            if info.error == "missing" {
                tell(p, `\c6No brick group was found for "\c3${name}\c6".`);
                tell(p, `\c6Bricks will be planted in your own group!`);
                set_player(p, "plant_as", "");
            } else if info.error == "trust" {
                tell(p, `\c6You need build trust with \c3${name}\c6 to plant bricks in their group.`);
                set_player(p, "plant_as", "");
            } else if info.error != () {
                set_player(p, "plant_as", "");
            } else if name == "" {
                tell(p, `\c6Bricks will be planted in your own group!`);
                set_player(p, "plant_as", "");
            } else {
                let their = if name.ends_with("s") { "'" } else { "'s" };
                tell(p, `\c6Bricks will now be planted in \c3${name}\c6${their} group!`);
                set_player(p, "plant_as", name);
            }
            bottom(p);
            return;
        }
        "undo" => {
            tell(p, `\c6Next undo will affect \c3${info.bricks}\c6 bricks. Press undo again to continue.`);
            return;
        }
        "cut" => {
            if info.error != () {
                no_trust(p);
                return;
            }
            // `ND_Selection::finishCutting`.
            let msg = `<font:Verdana:20>\c6Cut \c3${info.bricks}\c6 Brick${s(info.bricks)}!`;
            if info.refused > 0 {
                msg += line(`\c3${info.refused}\c6 missing trust.`);
            }
            center_print(p, msg, 8);
            set_mode(p, "plant");
            return;
        }
    }
    // A stack or box selected.
    let mode = get_player(p, "mode");
    if info.error == "trust" || info.error == "public" {
        no_trust(p);
        return;
    }
    if info.error != () && info.error != "empty" {
        oops(p, info.message, 5);
        return;
    }
    menu_sound(p, "uploadEndSound");
    let msg = `<font:Verdana:20>\c6Selected \c3${info.bricks}\c6 Brick${s(info.bricks)}!`;
    if info.limit_reached {
        msg += " (Limit Reached)";
    }
    if info.refused > 0 {
        msg += line(`\c3${info.refused}\c6 missing trust.`);
    }
    if mode == "box" {
        msg += line(`\c6Press [Cancel Brick] to adjust the box.`);
    }
    center_print(p, msg, if mode == "box" { 8 } else { 5 });
    set_player(p, "selected", info.bricks);
    set_player(p, "size", if info.size == () { [] } else { info.size });
    set_player(p, "selection_box", if info.box == () { [] } else { info.box.min + info.box.max });
    take_paints(p);
    if info.bricks > 0 {
        // Glowing until let go or taken up (`ND_Selection::deHighlight`).
        highlight_copy(p, (), -1.0);
        if mode == "box" {
            set_player(p, "box_selected", true);
        }
    }
    draw_boxes(p);
    bottom(p);
}

// A job canceled ([Cancel Brick], or put away): each progress mode's
// `onCancelBrick`.
fn canceled(p, info) {
    set_player(p, "fill_after_cut", false);
    // Put away (`ndKillMode`): nothing more is said.
    if get_player(p, "mode") == "" {
        return;
    }
    switch info.action {
        // `NDM_StackSelectProgress` and `NDM_BoxSelectProgress`: the
        // selection goes, the box stays to adjust.
        "select" => {
            oops(p, "Selection canceled!", 4);
            set_player(p, "selected", 0);
            set_player(p, "box_selected", false);
            drop_copy(p);
            take_paints(p);
            draw_boxes(p);
        }
        // `NDM_CutProgress` and `NDM_LoadProgress`: back to selecting,
        // the selection gone.
        "cut" | "load" => {
            set_player(p, "selected", 0);
            drop_copy(p);
            set_mode(p, get_player(p, "last_select"));
        }
        // `NDM_SuperCutProgress`.
        "supercut" => oops(p, "Supercut canceled!", 4),
        // Painting and the fill wrench stop where they got to.
        "paint" | "wrench" => {}
        _ => oops(p, info.message, 4),
    }
    bottom(p);
}

fn tally(failed, key) {
    if key in failed { failed[key] } else { 0 }
}

// `ND_Selection::finishPlant`, after which the selection is planted from;
// and `conditionalPlant`'s refusals before it.
fn on_place(p, info) {
    set_working(p, "");
    if info.float_refused {
        // `NDM_PlantCopy::onPlantBrick`'s check, made at each plant.
        tell(p, `\c6Force Plant has been disabled because it is admin only. Ask an admin for help.`);
        set_player(p, "force_plant", false);
    }
    if info.canceled {
        if get_player(p, "mode") != "" {
            oops(p, "Planting canceled!", 4);
            set_mode(p, "plant");
        }
        return;
    }
    if info.planted > 0 && info.bricks > process_per_tick() * 10 {
        // `ND_Selection::finishPlant`: a big plant sounds.
        menu_sound(p, "processCompleteSound");
    }
    if info.error == "wait" {
        let left = info.wait.ceiling().to_int();
        if left < 1 {
            left = 1;
        }
        play_sound(p, "errorSound");
        oops(p, `You need to wait\c3 ${left}\c6 second${s(left)} before planting again!`, 5);
        return;
    }
    if info.error == "group" {
        tell(p, `\c6You need build trust with \c3${get_player(p, "plant_as")}\c6 to plant bricks in their group.`);
        return;
    }
    // Kept for `/MirErrors`, and said once the plant is done
    // (`ND_Selection::finishPlant`).
    set_player(p, "mirror_errors", [info.mirror_errors.side, info.mirror_errors.upside_down]);
    if info.mirror_errors.side.len() + info.mirror_errors.upside_down.len() > 0 {
        tell(p, `\c6Some bricks were probably mirrored incorrectly. Say \c3/mirErrors\c6 to find out more.`);
    }
    let failed = info.failed;
    if tally(failed, "too_far") == info.bricks && info.bricks > 0 {
        oops(p, "You can't plant so far away!", 5);
        return;
    }
    let trusted = tally(failed, "forbidden");
    let blocked = tally(failed, "overlap") + tally(failed, "buried") + tally(failed, "stuck");
    let floating = info.bricks - info.planted - trusted - blocked;
    let message = `<font:Verdana:20>\c6Planted \c3${info.planted}\c6 / \c3${info.bricks}\c6 Brick${s(info.planted)}!`;
    if trusted > 0 {
        message += line(`\c3${trusted}\c6 missing trust.`);
    }
    if blocked > 0 {
        message += line(`\c3${blocked}\c6 blocked.`);
    }
    if floating > 0 {
        message += line(`\c3${floating}\c6 floating.`);
    }
    center_print(p, message, 4);
    set_mode(p, "plant");
}
