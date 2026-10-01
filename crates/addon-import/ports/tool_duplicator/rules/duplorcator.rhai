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
//   Taking the wand out with nothing selected says Normal Mode; the cancel
//   key lets the selection go.

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

// "Planting Timeout (Admin)" and "(Non-Admin)", seconds, and "Max Flood
// Bypass": copies of up to this many bricks plant at will.
fn admin_plant_wait() {
    1
}
fn player_plant_wait() {
    3
}
fn flood_ignore() {
    50
}

// "Client Loading/Saving": off, as Return to Blockland defaulted it and as
// the Add-On left it without, so the host takes no duplications uploaded
// from players' computers.
fn client_loading() {
    false
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

// Whether `player` holds a copy this Add-On took (`%client.hasDup`).
fn has_dup(player) {
    let copy = player(player).copy;
    copy != () && copy.addon == "{{rules}}"
}

// `getSimTime() - lastPlantTime < PlantTimeout` for copies over the flood
// bypass: the copy now held sets the wait its next plant keeps.
fn set_plant_wait(player, bricks) {
    let wait = if bricks <= flood_ignore() {
        0
    } else if player(player).admin {
        admin_plant_wait()
    } else {
        player_plant_wait()
    };
    plant_wait(player, wait);
}

// `DuplorcatorImage::onMount`: with nothing selected, Normal Mode.
fn cmd_mount(player) {
    if !has_dup(player) {
        bottom_print(player, normal_mode(), 0);
    }
}

// `DuplorcatorImage::onUnMount`.
fn cmd_unmount(player) {
    if !has_dup(player) {
        bottom_print(player, normal_mode(), 0.5);
    }
}

// The cancel key deletes the duplication's temp brick
// (`FxDtsBrick::onRemove`): the selection goes, back to Normal Mode.
fn cmd_cancel(player) {
    if has_dup(player) {
        drop_copy(player);
        bottom_print(player, normal_mode(), 0);
    }
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
    // The swing; the engine's own onStopFire puts the arm back.
    play_thread(player, 2, "armAttack");
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
        plant_error(player, "flood");
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
    // A duplication bigger than the limit asks first (on_copy).
    load_copy(player, name, limit, tool(), #{ partial: true, whole: true });
}

// `servercmdDuplorcatorLoadAnyways`: yes to loading what fits of a
// duplication over the limit.
fn cmd_duplorcatorloadanyways(player) {
    let name = get_player(player, "asked_load");
    if name == "" {
        return;
    }
    set_player(player, "asked_load", "");
    let me = player(player);
    let limit = if me.admin { admin_limit() } else { player_limit() };
    load_copy(player, name, limit, tool(), #{ partial: true });
}

// `/clientLoad` (the Add-On's chat hook on the player's side) and
// `servercmdreloadDup`: with client loading off the host never takes an
// upload, so there is no cached duplication to load.
fn cmd_clientload(player, typed) {
    if !client_loading() {
        center_print(player, "<color:99AAAA>This server does not take duplications from your computer", 3);
    }
}
fn cmd_reloaddup(player) {
    center_print(player, "You have no cached duplication", 3);
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
        if info.error == "limit" {
            set_player(player, "asked_load", info.name);
            ask(player, "Duplication Upload", "This duplication exceeds the max bricks you can duplicate. The whole build will not be loaded, would you like to load it anyways?", "duplorcatorloadanyways");
            return;
        }
        if info.error == "missing" {
            center_print(player, `<color:99AAAA>Duplorcation save\c4 ${info.name}<color:99AAAA> does not exist`, 3);
            return;
        }
        if info.error != () {
            center_print(player, info.message, 3);
            return;
        }
        // The loaded copy stands ready to plant, with the wand in hand.
        set_plant_wait(player, info.bricks);
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
    set_plant_wait(player, info.bricks);
    // `getStack` stopped at the brick limit.
    if info.limit_reached {
        plant_error(player, "limit");
    }
}

// `servercmdPlantBrick` for a duplication.
fn on_place(player, info) {
    if info.error == "wait" {
        let left = info.wait.ceiling().to_int();
        if left < 1 {
            left = 1;
        }
        let unit = if left > 1 { "seconds" } else { "second" };
        plant_error(player, "flood");
        center_print(player, `<color:99AAAA>You must wait \c4${left} <color:99AAAA>${unit} before planting again`, 3);
        return;
    }
    if info.planted > 0 {
        center_print(player, `\c4${info.planted}<color:99AAAA>/\c4${info.bricks}<color:99AAAA> bricks duplicated successfully`, 3);
        return;
    }
    // Nothing planted: what kept the first brick out.
    if info.error == "overlap" || info.error == "float" || info.error == "stuck" || info.error == "buried" || info.error == "too_far" || info.error == "limit" {
        plant_error(player, info.error);
    } else if info.error != () {
        center_print(player, info.message, 3);
    }
}
