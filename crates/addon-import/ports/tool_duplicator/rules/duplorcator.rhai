// The Duplicator (Plornt's Duplorcator, Tool_Duplicator), ported: what its
// server.cs did, on the player's own imported copy. Runs on the host.
//
//   /dup, /duplorcator or /duplicator puts it in your hand.
//   Clicking a brick within reach selects it and everything built on it
//   (from the clicked brick only up, from the rest every way), lights the
//   selection up for a moment, and shows the copy as a ghost.
//   The brick keys move and turn the copy; the plant key plants each brick
//   of it that fits, and Ctrl+Z takes the whole planted copy back.
//   /saveDup <name> keeps the copy on the host; /loadDup <name> takes it
//   (or a v20 duplication file of that name) out again.

// The Add-On's own settings, as it registered them with Return to
// Blockland's server preferences (its defaults there).
fn admin_only() {
    false
}
// "Max Bricks (Admin)" and "Max Bricks (Non-Admin)".
fn admin_limit() {
    5000
}
fn player_limit() {
    500
}
// "Selecting Timeout (Admin)" and "(Non-Admin)", seconds.
fn admin_wait() {
    1
}
fn player_wait() {
    3
}

// "Load Admin Only" and "Save Admin Only". The Add-On read the first for
// both and then refused everyone, admins too; here each keeps to its own
// command and lets admins through.
fn admin_load() {
    false
}
fn admin_save() {
    false
}

// Simulation ticks per second.
fn second() {
    120
}

fn tool() {
    "{{namespace}}:weapon/duplorcatoritem"
}

fn projectile() {
    "{{namespace}}:projectile/duplorcatorprojectile"
}

// `$Pref::Duplorcator::HighlightColor = getClosestPaintColor("0 1 1 1")`,
// lit for as long as `getStack` schedules `restoreOrignalColors`.
fn highlight() {
    [0.0, 1.0, 1.0, 1.0]
}
fn highlight_seconds() {
    {{highlight_ms}} / 1000.0
}

fn normal_mode() {
    "\\c4Normal <color:99AAAA>Mode<br>No bricks selected"
}

// `servercmdDuplorcator`: mount the Duplorcator.
fn cmd_duplorcator(player) {
    give_item(player, tool(), true);
    bottom_print(player, normal_mode(), 0);
}

fn cmd_dup(player) {
    cmd_duplorcator(player);
}

// The Add-On took `/duplicator` too when no other duplicator had it.
fn cmd_duplicator(player) {
    cmd_duplorcator(player);
}

// `DuplorcatorImage::onFire`.
fn cmd_fire(player) {
    let hit = aim();
    if hit == () {
        return;
    }
    // Where the swing lands, the wand's spark.
    fire(projectile(), hit.x, hit.y, hit.z, 0.0, 0.0, 0.0, player);
    if hit.brick == () {
        return;
    }
    let me = player(player);
    if admin_only() && !me.admin {
        center_print(player, "<color:99AAAA>You must be an \\c4Admin<color:99AAAA> to use the \\c4Duplorcator", 3);
        return;
    }
    let wait = if me.admin { admin_wait() } else { player_wait() } * second();
    let last = get_player(player, "last_select");
    if tick() - last < wait {
        let left = ((wait - (tick() - last)) + second() - 1) / second();
        let unit = if left > 1 { "seconds" } else { "second" };
        center_print(player, `<color:99AAAA>You must wait \c4${left} <color:99AAAA>${unit} before selecting again`, 3);
        return;
    }
    set_player(player, "last_select", tick());
    let limit = if me.admin { admin_limit() } else { player_limit() };
    // getTrustLevel against the Add-On's "Trust Required", Full: public
    // bricks are refused, and administrators get no say.
    copy_build(player, hit.brick, limit, "up", tool(), #{ trust: "full", public_bricks: false, admin: false, partial: true });
    highlight_copy(player, highlight(), highlight_seconds());
}

// `servercmdSaveDup`.
fn cmd_savedup(player, typed) {
    let me = player(player);
    if typed == "" {
        return;
    }
    if admin_only() && !me.admin {
        center_print(player, "<color:99AAAA>You must be an \\c4Admin<color:99AAAA> to use the \\c4Duplorcator", 3);
        return;
    }
    if admin_save() && !me.admin {
        center_print(player, "<color:99AAAA>You must be an \\c4Admin<color:99AAAA> to save duplications", 3);
        return;
    }
    let name = copy_name(typed);
    if name == () {
        center_print(player, "<color:99AAAA>Could not write to duplication path.", 3);
        return;
    }
    save_copy(player, name);
}

// `servercmdLoadDup`.
fn cmd_loaddup(player, typed) {
    let me = player(player);
    if typed == "" {
        return;
    }
    if admin_only() && !me.admin {
        center_print(player, "<color:99AAAA>You must be an \\c4Admin<color:99AAAA> to use the \\c4Duplorcator", 3);
        return;
    }
    if admin_load() && !me.admin {
        center_print(player, "<color:99AAAA>You must be an \\c4Admin<color:99AAAA> to load duplications", 3);
        return;
    }
    // The selecting timeout, which the Add-On only ever read for admins
    // here (the other's name is misspelt), so others load at will.
    let wait = if me.admin { admin_wait() * second() } else { 0 };
    let last = get_player(player, "last_load");
    if tick() - last < wait {
        let left = ((wait - (tick() - last)) + second() - 1) / second();
        let unit = if left > 1 { "seconds" } else { "second" };
        center_print(player, `<color:99AAAA>You must wait \c4${left} <color:99AAAA>${unit} before loading again`, 3);
        return;
    }
    let name = copy_name(typed);
    if name == () {
        center_print(player, `<color:99AAAA>Duplorcation save\c4 ${typed}<color:99AAAA> does not exist`, 3);
        return;
    }
    set_player(player, "last_load", tick());
    let limit = if me.admin { admin_limit() } else { player_limit() };
    // A duplication bigger than the limit asked whether to load what fits;
    // here what fits loads, as answering yes did.
    load_copy(player, name, limit, tool(), #{ partial: true });
}

// What the selection, save or load did.
fn on_copy(player, info) {
    if info.action == "save" {
        if info.error == () {
            center_print(player, `<color:99AAAA>Duplication successfully saved as '\c4${info.name}<color:99AAAA>'`, 3);
        } else if info.error == "empty" {
            // The Add-On said nothing without a selection.
        } else {
            center_print(player, "<color:99AAAA>Could not write to duplication path.", 3);
        }
        return;
    }
    if info.action == "load" {
        if info.error == "missing" {
            center_print(player, `<color:99AAAA>Duplorcation save\c4 ${info.name}<color:99AAAA> does not exist`, 3);
            return;
        }
        if info.error != () {
            center_print(player, info.message, 3);
            return;
        }
        // The loaded copy stands ready to plant, with the wand in hand.
        give_item(player, tool(), true);
        center_print(player, `<color:99AAAA>Loaded duplication \c4${info.name}<color:99AAAA> (\c4${info.bricks}<color:99AAAA>/\c4${info.total}<color:99AAAA>)`, 3);
        let bricks = if info.bricks > 1 { "bricks" } else { "brick" };
        bottom_print(player, `\c4Duplication <color:99AAAA>Mode<br>${info.bricks} ${bricks} selected`, 0);
        return;
    }
    if info.error == "public" {
        center_print(player, "You cannot duplicate public bricks!", 3);
        return;
    }
    if info.error != () {
        center_print(player, info.message, 3);
        return;
    }
    let bricks = if info.bricks > 1 { "bricks" } else { "brick" };
    bottom_print(player, `\c4Duplication <color:99AAAA>Mode<br>${info.bricks} ${bricks} selected`, 0);
}

// `servercmdPlantBrick` for a duplication.
fn on_place(player, info) {
    if info.planted > 0 {
        center_print(player, `\c4${info.planted}<color:99AAAA>/\c4${info.bricks}<color:99AAAA> bricks duplicated successfully`, 3);
    }
}
