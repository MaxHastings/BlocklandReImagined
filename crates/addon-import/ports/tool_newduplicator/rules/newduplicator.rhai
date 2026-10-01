// The New Duplicator (Zeblote's Tool_NewDuplicator), ported: what its
// server scripts did, on the player's own imported copy. Runs on the host.
//
//   /newduplicator (/duplicator, /dup, /d and the rest) puts it in your hand.
//   Stack mode: clicking a brick selects it and everything built on it, up
//   or down from it ([Next Seat]), limited to its side or not
//   ([Prev Seat]), and lights the selection up; holding Ctrl (crouch) adds
//   to the selection. [Light] switches to box mode (turning a selection
//   into a box round it): clicking a brick puts a box around it, the brick
//   keys move its corner ([Rotate Brick] picks the other corner), and
//   [Plant Brick] selects what is in it; with Ctrl held a click grows the
//   box to the plate clicked, and the keys move and turn the whole box.
//   With a selection, the brick keys take it up as a ghost (plant mode):
//   they move it, clicking puts it against what you click, [Plant Brick]
//   plants it brick by brick and says what did not fit. [Cancel Brick]
//   drops the selection, then the box.
//   With a selection, a paint can is fill colour: [Plant Brick] paints it.
//   /FillWrench wrenches it all, /SuperCut cuts the box out and puts plain
//   bricks back over what stuck out, /FillBricks then fills the box,
//   /ForcePlant plants a ghost that floats, /MirrorX, /MirrorY and
//   /MirrorZ mirror it, /Cut cuts the originals away, /SaveDup and
//   /LoadDup keep and load selections by name.

// Its preferences, as `ndApplyDefaultPrefValues` set them.
fn admin_only() {
    {{admin_only}}
}
fn save_admin_only() {
    {{save_admin_only}}
}
fn load_admin_only() {
    {{load_admin_only}}
}
// "Trust Limit": 2 is full trust, 1 build trust.
fn trust() {
    if {{trust_limit}} >= 2 { "full" } else { "build" }
}
// "Admin Trust Bypass" when selecting.
fn admin_bypass() {
    {{admin_bypass}}
}
fn select_public() {
    {{select_public}}
}
// "Max Bricks", cut to the most one copy holds here.
fn max_bricks(admin) {
    let n = if admin { {{max_bricks_admin}} } else { {{max_bricks_player}} };
    if n > 10000 { 10000 } else { n }
}
// "Max Box Size", units on a side, cut to the largest box copied here.
fn max_box(admin) {
    let n = if admin { {{max_box_admin}} } else { {{max_box_player}} };
    if n > 256 { 256.0 } else { n.to_float() }
}
// "Select Timeout", non-admins only.
fn select_wait() {
    {{select_timeout_ms}} * second() / 1000
}
// How long a selection glows.
fn glow_seconds() {
    5.0
}
// Longest save name.
fn name_length() {
    {{name_length}}
}
// Admin-only features.
fn paint_admin_only() {
    {{paint_admin_only}}
}
fn paint_fx_admin_only() {
    {{paint_fx_admin_only}}
}
fn wrench_admin_only() {
    {{wrench_admin_only}}
}
fn float_admin_only() {
    {{float_admin_only}}
}
fn fill_bricks_admin_only() {
    {{fill_bricks_admin_only}}
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

// Each mode's `getBottomPrint`.
fn bottom(p) {
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
            let multi = if player(p).crouched { "Multi-" } else { "" };
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
                `\c3Colour ${fill[1] + 1}`
            };
            format_message(title, [
                ["Select paint can to chose color", "[Plant Brick]: Paint bricks"],
                ["Color: " + color, "[Cancel Brick]: Exit mode"],
            ])
        }
        "plant" => {
            let count = get_player(p, "selected");
            let title = `Plant Mode (\c3${count}\c6 Brick` + if count == 1 { ")" } else { "s)" };
            let size = get_player(p, "size");
            let l1 = "";
            if size.len() == 3 {
                l1 = `Size: \c3${size[0]}\c6 x \c3${size[2]}\c6 x \c3${size[1]}\c6 Plates`;
            }
            format_message(title, [
                ["", "Use normal ghost brick controls"],
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
        hide_box(p);
    }
    if old == "fillcolor" {
        // The paint box goes, the duplicator's tools come back.
        scroll_mode(p, "tools");
    }
    if old == "plant" && (mode == "stack" || mode == "box") {
        // Leaving plant mode lets the selection go.
        set_player(p, "selected", 0);
        drop_copy(p);
    }
    if (old == "stack" || old == "box") && mode == "plant" {
        // Taken up as a ghost where it was selected.
        show_copy(p);
        float_copy(p, get_player(p, "force_plant"));
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
        show_box(p, low, high, tool());
    } else {
        let studs = (limit * 2.0).floor().to_int();
        center_print(p, `<font:Verdana:20>\c6Oops!` + "\n<font:Verdana:17>" + `\c6Your selection box is limited to \c3${studs} \c6studs.`, 5);
    }
}

// `ndKillMode`: the duplicator put away.
fn kill_mode(p) {
    if get_player(p, "mode") == "fillcolor" {
        scroll_mode(p, "tools");
    }
    set_player(p, "mode", "");
    set_player(p, "selected", 0);
    set_player(p, "box", []);
    set_player(p, "box_selected", false);
    hide_box(p);
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

// Ctrl, held: the original's multiselect key, which shares Ctrl with
// crouching.
fn multi(me) {
    me.crouched
}

fn copy_rule(p, me) {
    #{
        trust: trust(),
        public_bricks: select_public(),
        admin: admin_bypass(),
        partial: true,
        limited: get_player(p, "limited"),
        // Held as a selection until a brick key takes it up.
        hidden: true,
        add: get_player(p, "mode") == "stack" && multi(me),
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
        return;
    }
    // Box mode: a box around the clicked brick.
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
    if multi(me) {
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
    show_box(p, box_min(corners), box_max(corners), tool());
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
            play_sound(p, "lightOnSound");
            set_mode(p, "box");
        }
        "box" => {
            play_sound(p, "lightOffSound");
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
    } else {
        return;
    };
    let on = !get_player(p, key);
    set_player(p, key, on);
    bottom(p);
    play_sound(p, if on { "lightOnSound" } else { "lightOffSound" });
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
    if multi(me) {
        // `ND_SelectionBox::shift`: the whole box moves.
        let moved = b;
        for a in 0..3 {
            moved[a] += offset[a];
            moved[a + 3] += offset[a];
        }
        play_sound(p, "brickMoveSound");
        set_player(p, "box", moved);
        show_box(p, box_min(moved), box_max(moved), tool());
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
    show_box(p, box_min(moved), box_max(moved), tool());
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
    if multi(player(p)) {
        let turned = rotate_box(b, dir);
        play_sound(p, "brickRotateSound");
        set_player(p, "box", turned);
        show_box(p, box_min(turned), box_max(turned), tool());
        bottom(p);
        return;
    }
    set_player(p, "corner", !get_player(p, "corner"));
    play_sound(p, "brickRotateSound");
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
}

// [Cancel Brick].
fn cmd_cancel(p) {
    switch get_player(p, "mode") {
        "stack" => {
            set_player(p, "selected", 0);
            drop_copy(p);
            take_paints(p);
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
                hide_box(p);
            }
            set_player(p, "selected", 0);
            drop_copy(p);
            take_paints(p);
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

// `GameConnection::ndMirror`: only a ghost in plant mode mirrors.
fn mirror(p, axis) {
    if get_player(p, "mode") != "plant" {
        tell(p, `\c6The mirror command can only be used in plant mode or with a ghost brick.`);
        return;
    }
    mirror_copy(p, axis);
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
        paint_copy(p, #{ fx: fill[1] });
    } else {
        paint_copy(p, #{ shape: fill[1] - 7 });
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
        float_copy(p, on);
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
fn cmd_ndcut(p) {
    let mode = get_player(p, "mode");
    if mode == "" {
        return;
    }
    if !(mode == "stack" || mode == "box") || get_player(p, "selected") == 0 {
        if mode == "plant" {
            tell(p, `\c6Cut can not be used in your current duplicator mode.`);
        }
        return;
    }
    cut_copy(p);
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
    tell(p, `\c6Saving selection to "\c3${name}\c6"...`);
    save_copy(p, name);
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
    load_copy(p, name, 10000, tool(), #{ partial: true });
}
fn cmd_ld(p, typed) {
    cmd_loaddup(p, typed);
}

// `/DupHelp`, for the commands this port has (within the eight chat
// lines a second an Add-On may send).
fn cmd_duphelp(p) {
    tell(p, `\c6You can use the following commands with your new duplicator:`);
    tell(p, `\c3/Duplicator\c6 Equip a new duplicator!`);
    tell(p, `\c3/ForcePlant\c6 Plant a selection in mid air; bricks can float. \c3/ToggleForcePlant\c6 keeps it on.`);
    tell(p, `\c3/FillWrench\c6 Change settings on all selected bricks.`);
    tell(p, `\c3/MirrorX\c6, \c3/MirrorY\c6, \c3/MirrorZ\c6 Mirror your ghost selection left/right, front/back, up/down.`);
    tell(p, `\c3/SuperCut\c6 Delete everything in your selection box. \c3/FillBricks\c6 Then fill it with few bricks.`);
    tell(p, `\c3/Cut\c6 Cut your selection away. \c3/SaveDup\c6 and \c3/LoadDup\c6 [\c3name\c6] Save and load selections.`);
    tell(p, `\c6All of the commands can be shortened by just typing a \c3/\c6 and the capital letters!`);
}
fn cmd_dh(p) {
    cmd_duphelp(p);
}

// A selection, cut, save, load, paint, wrench, supercut or fill finished.
fn on_copy(p, info) {
    switch info.action {
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
                tell(p, `\c6Finished saving selection, wrote \c3${info.bricks}\c6 Brick${s(info.bricks)}!`);
                set_player(p, "last_save_tick", tick());
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
            set_player(p, "selected", info.bricks);
            set_player(p, "size", if info.size == () { [] } else { info.size });
            set_mode(p, "plant");
            return;
        }
        "cut" => {
            if info.error != () {
                oops(p, "You don't have enough trust to do that!", 5);
                return;
            }
            center_print(p, `<font:Verdana:20>\c6Cut \c3${info.bricks}\c6 Brick${s(info.bricks)}!`, 8);
            set_mode(p, "plant");
            return;
        }
    }
    // A stack or box selected.
    let mode = get_player(p, "mode");
    if info.error == "trust" || info.error == "public" {
        oops(p, "You don't have enough trust to do that!", 5);
        return;
    }
    if info.error != () && info.error != "empty" {
        oops(p, info.message, 5);
        return;
    }
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
        highlight_copy(p, (), glow_seconds());
        if mode == "box" {
            set_player(p, "box_selected", true);
        }
    }
    bottom(p);
}

fn tally(failed, key) {
    if key in failed { failed[key] } else { 0 }
}

// `ND_Selection::finishPlant`, after which the selection is planted from.
fn on_place(p, info) {
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
