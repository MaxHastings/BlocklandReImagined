// The Advanced Duplicator: the Duplicator with more to it, for big builds.
// Our own tool, after the ideas of Zeblote's New Duplicator for Blockland.
// It runs on the host.
//
//   /adup (or /advdup) puts the gold wand in your hand.
//   Stack mode (the start): clicking a brick copies it and everything built
//   on it, as the classic Duplicator does.
//   Box mode (/box switches): the first click marks one corner, the second
//   the opposite one, and everything wholly inside the box that you may
//   build on is copied. A clicked brick's whole box is a corner; clicking
//   the ground marks the one plate-sized cell there.
//   The copy shows as a ghost over the original. The brick keys move and
//   turn it, /mirror flips it left to right as you face (/mirx and /mirz
//   flip it across the world's axes), and the plant key plants it.
//   /cut removes the bricks the copy was taken from (with full trust), so
//   planting the copy elsewhere moves the build; /fillcolor paints them in
//   your spray colour. Ctrl+Z undoes a planted copy, a cut (the bricks come
//   back exactly as they were) or a fill.

// Most bricks one copy may hold.
fn limit() {
    5000
}

// Longest side of a selection box, units (the engine's own limit).
fn span() {
    256.0
}

fn tool() {
    "advanced-duplicator-tool:weapon/advanced-duplicator"
}

fn cmd_adup(player) {
    give_item(player, tool(), true);
    bottom_print(player, "Advanced Duplicator: click a brick to copy it. /duphelp for more", 4);
}

fn cmd_advdup(player) {
    cmd_adup(player);
}

fn cmd_duphelp(player) {
    tell(player, "Advanced Duplicator: /adup takes it out. Click a brick to copy it and everything on it.");
    tell(player, "/box: click two corners to copy everything between them (/box again goes back).");
    tell(player, "Brick keys move and turn the copy; /mirror flips it (/mirx, /mirz across the world); plant to place.");
    tell(player, "/cut removes the original, /fillcolor paints it your spray colour. Ctrl+Z undoes any of them.");
}

fn cmd_box(player) {
    set_player(player, "corner", []);
    if get_player(player, "mode") == "box" {
        set_player(player, "mode", "stack");
        hide_box(player);
        bottom_print(player, "Stack mode: click a brick to copy it and everything on it", 3);
    } else {
        set_player(player, "mode", "box");
        bottom_print(player, "Box mode: click one corner, then the opposite corner", 3);
    }
}

fn cmd_mirror(player) {
    mirror_copy(player, "view");
}

fn cmd_mirx(player) {
    mirror_copy(player, "x");
}

fn cmd_mirz(player) {
    mirror_copy(player, "z");
}

fn cmd_cut(player) {
    cut_copy(player);
}

fn cmd_fillcolor(player) {
    let me = player(player);
    if me != () {
        paint_copy(player, me.paint);
    }
}

fn lower(a, b) {
    if a < b { a } else { b }
}

fn upper(a, b) {
    if a > b { a } else { b }
}

// A corner as [min x, y, z, max x, y, z]: the clicked brick's whole box,
// or the plate-sized cell where the click met the ground.
fn corner_of(hit) {
    if hit.brick != () {
        let b = brick_box(hit.brick);
        if b != () {
            return [b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]];
        }
    }
    let x = (hit.x / 0.5).floor() * 0.5;
    let y = (hit.y / 0.2).floor() * 0.2;
    let z = (hit.z / 0.5).floor() * 0.5;
    [x, y, z, x + 0.5, y + 0.2, z + 0.5]
}

// The Advanced Duplicator's swing (`onFire` of its image).
fn cmd_click(player) {
    let hit = aim();
    if hit == () {
        return;
    }
    if get_player(player, "mode") != "box" {
        if hit.brick == () {
            return;
        }
        hide_box(player);
        copy_build(player, hit.brick, limit(), true, tool());
        return;
    }
    let c = corner_of(hit);
    let first = get_player(player, "corner");
    if first.len() != 6 {
        set_player(player, "corner", c);
        show_box(player, [c[0], c[1], c[2]], [c[3], c[4], c[5]], tool());
        bottom_print(player, "Now click the opposite corner", 3);
        return;
    }
    set_player(player, "corner", []);
    let low = [lower(first[0], c[0]), lower(first[1], c[1]), lower(first[2], c[2])];
    let high = [upper(first[3], c[3]), upper(first[4], c[4]), upper(first[5], c[5])];
    for axis in 0..3 {
        if high[axis] - low[axis] > span() {
            hide_box(player);
            center_print(player, "That box is too big: at most 512 studs on a side", 3);
            return;
        }
    }
    show_box(player, low, high, tool());
    copy_box(player, low, high, limit(), tool());
}
