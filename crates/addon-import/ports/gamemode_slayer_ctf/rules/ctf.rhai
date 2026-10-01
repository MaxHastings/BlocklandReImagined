// Slayer Capture the Flag (Greek2me's Gamemode_Slayer_CTF), rewritten for
// Blockland ReImagined. It adds Capture the Flag to Slayer's game modes and
// plays in a mini-game set to it whose builder placed Flag Spawn bricks: a
// flag stands on each, in the brick's paint colour, and belongs to the team
// of that colour (a colour no team has is a neutral flag). Its settings are
// with Slayer's in the Mini-Game window's Add-On Settings.
//
// Take an enemy flag to your own flag, or to a Flag Return Point of your
// colour, to score. The carried flag rides on your back; dying, leaving the
// game or /dropFlag drops it, and a dropped flag goes home after a while or
// when its own team touches it.

fn c3() { "" }
fn c5() { "" }
fn magenta() { "<color:ff00ff>" }

fn flag_spawn() { "{{namespace}}:brick/brickslyrctfflagdata" }
fn flag_return() { "{{namespace}}:brick/brickslyrctfflagreturndata" }
fn flag_item() { "{{namespace}}:weapon/slyrctf_flagitem" }
fn flag_image() { "{{namespace}}:image/slyrctf_flagimage" }
// `$Slayer::Server::CTF::flagImageSlot`: the image slot the flag rides in.
fn flag_slot() { {{image_slot}} }
// `slyrCTF_FlagItem::onPickUp`: a flag cannot be taken in its first
// {{pickup_guard_ms}} ms (120 ticks a second).
fn pickup_guard_ticks() { ({{pickup_guard_ms}} * 120 + 999) / 1000 }

// Capture the Flag is the game's Slayer mode.
fn ctf_mode() { "{{ctf_class}}" }
fn playing(game) { setting(game, "{{slayer_rules}}:mode") == ctf_mode() }
fn number(on) { if on { 1 } else { 0 } }

// Slayer CTF's preferences (`preferences.cs`), from the game's settings.
fn prefs_of(game) {
    #{
        flagreturnstowin: setting(game, "flag_returns_to_win"),
        flagreturnonlyatreturnbrick: setting(game, "flag_return_location"),
        returnwithoutown: number(setting(game, "return_without_own")),
        neutralflags: setting(game, "neutral_flags"),
        points_flag: setting(game, "points_flag"),
        points_flagrecovery: setting(game, "points_flag_recovery"),
        flagrecovery: setting(game, "flag_recovery"),
        flagdroppedrespawntime: setting(game, "dropped_flag_respawn"),
        flagreturnedrespawntime: setting(game, "returned_flag_respawn"),
        manualflagdrop: number(setting(game, "manual_flag_drop")),
        requireenemyplayers: number(setting(game, "require_enemy_players"))
    }
}

// Per game: the preferences, the captures by team id this round, and the
// report's tallies (`CTF_numFlagPickups`, `CTF_numFlagReturns`) by row:
// `team:<id>` and `player:<id>`.
fn game_state(game) {
    let games = get("games");
    let key = `${game}`;
    let kept = if key in games { games[key] } else { #{} };
    #{
        prefs: prefs_of(game),
        returns: if "returns" in kept { kept.returns } else { #{} },
        pickups_by: if "pickups_by" in kept { kept.pickups_by } else { #{} },
        returns_by: if "returns_by" in kept { kept.returns_by } else { #{} }
    }
}
fn set_game_state(game, s) {
    let games = get("games");
    games[`${game}`] = #{ returns: s.returns, pickups_by: s.pickups_by, returns_by: s.returns_by };
    set("games", games);
}
// One more for the player and their team.
fn tally(counts, p, team) {
    for key in [`player:${p}`, `team:${team}`] {
        counts[key] = if key in counts { counts[key] + 1 } else { 1 };
    }
    counts
}
// `Slayer_CTF::scoreListInit` and `scoreListAdd`: the End of Round Report
// shows flag pick-ups and returns where Slayer shows kills and deaths,
// blank for none.
fn report_columns(game) {
    let s = game_state(game);
    report_column(game, "kills", "{{pickups_title}}", s.pickups_by);
    report_column(game, "deaths", "{{returns_title}}", s.returns_by);
}
fn drop_report_columns(game) {
    report_column(game, "kills", ());
    report_column(game, "deaths", ());
}

// Flags by Flag Spawn brick: where each is (`home`, `carried`, `dropped`,
// or `waiting` to come back) and since when.
fn flags() { get("flags") }
fn flag_of(flags, brick) {
    let key = `${brick}`;
    if key in flags { flags[key] } else { () }
}

fn team_by_id(g, id) {
    for t in g.teams {
        if t.id == id {
            return t;
        }
    }
    ()
}
// `%brick.isLocked[color]`, which Slayer's setTeamControlLocked output
// keeps on the brick.
fn locked(brick, color) {
    let l = brick_field(brick, "{{slayer_rules}}:locked");
    l != () && l.contains(color)
}
fn teams_of_color(g, color) {
    let out = [];
    for t in g.teams {
        if t.color == color {
            out.push(t);
        }
    }
    out
}

// `Slayer_CTF::getFlagDisplayName`: the teams of the flag's colour, else
// the brick's name, else "Neutral".
fn flag_name(g, b) {
    let names = "";
    for t in teams_of_color(g, b.color) {
        if names != "" {
            names += ", ";
        }
        names += t.name;
    }
    if names != "" {
        return `${c3()}${names}`;
    }
    let n = b.name;
    n.replace("_", " ");
    n.trim();
    if n == "" { `${c3()}Neutral` } else { `${c3()}${n}` }
}

// `Slayer_Support::RgbToHex` of a paint colour.
fn hex(color) {
    let pal = palette();
    if color >= pal.len() {
        return "ffffff";
    }
    let out = "";
    for i in 0..3 {
        let n = (pal[color][i] * 255.0).round().to_int();
        if n < 16 {
            out += "0";
        }
        out += n.to_hex();
    }
    out
}
// Slayer's `slayerPrepareBrick` with `Slayer_CTF::onMiniGameBrickAdded`:
// a Flag Spawn or Flag Return Point planted or repainted by hand tells its
// builder the teams it is for in a Capture the Flag game, else that it is
// set.
fn on_brick(event, id, p, info) {
    if p == () || event != "planted" && event != "painted" {
        return;
    }
    let b = info.brick;
    let text = `<just:center>${magenta()}${b.ui_name} set.`;
    for g in minigames() {
        if !playing(g.id) {
            continue;
        }
        let teams = "";
        for t in teams_of_color(g, b.color) {
            if teams != "" {
                teams += ",  ";
            }
            teams += `<sPush><color:${hex(t.color)}>${t.name}<sPop>`;
        }
        if teams != "" {
            text = `<just:center>${c5()}${b.ui_name} set for ${teams}${c5()}.`;
        }
    }
    bottom_print(p, text, 4);
}

// `MiniGameSO::messageAll`.
fn tell_game(g, text) { tell_minigame(g.id, text); }

// A Capture the Flag game this player plays in, with their team, or ().
// Nothing happens to flags between rounds.
fn side(p) {
    let me = player(p);
    if me == () || me.minigame == () || me.team == () {
        return ();
    }
    let g = minigame(me.minigame);
    if g == () || g.round_over || !playing(g.id) {
        return ();
    }
    let team = team_by_id(g, me.team);
    if team == () {
        return ();
    }
    #{ me: me, game: g, team: team }
}

// `FxDtsBrick::resetFlag`: the flag stands on its brick again. Called on
// the flags map (`flags.send_home(b)`), which it updates.
fn send_home(b) {
    set_brick_item(b.id, flag_item());
    this[`${b.id}`] = #{ state: "home", since: tick() };
}

// Remove the dropped flag that belongs to brick `brick`, if it lies around.
fn remove_dropped(brick) {
    for d in drops() {
        if d.data != () && d.data.brick == brick {
            remove_drop(d.id);
        }
    }
}

fn on_tick() {
    let flags = flags();
    let seen = #{};
    for b in bricks(flag_spawn()) {
        let key = `${b.id}`;
        seen[key] = true;
        let f = flag_of(flags, b.id);
        if b.game == () {
            continue;
        }
        let g = minigame(b.game);
        if g == () || g.teams.is_empty() || !playing(g.id) {
            // Another mode: no flags.
            if f != () {
                remove_dropped(b.id);
                set_brick_item(b.id, ());
                flags.remove(key);
            }
            continue;
        }
        if f == () {
            // `onMiniGameBrickAdded`: a new Flag Spawn gets its flag.
            flags.send_home(b);
        } else if f.state == "home" && b.item != flag_item() {
            // `serverCmdSetWrenchData`, packaged: the wrench does not take
            // a standing flag off its stand.
            flags.send_home(b);
        } else if (f.state == "dropped" || f.state == "waiting") && tick() >= f.respawn_at {
            // `Slayer_CTF::flagRespawnTick`.
            tell_game(g, `${magenta()}The ${flag_name(g, b)} ${magenta()}flag was respawned.`);
            remove_dropped(b.id);
            flags.send_home(b);
        } else if f.state == "dropped" {
            // Each second the dropped flag counts down the seconds left
            // over itself, in its colour (`setShapeName`).
            let left = (f.respawn_at - tick() + 119) / 120;
            for d in drops() {
                if d.data != () && d.data.brick == b.id {
                    name_drop(d.id, left, b.color);
                }
            }
        }
    }
    // Flag Spawns that are gone take their flags with them.
    for key in flags.keys() {
        if !(key in seen) {
            let f = flags[key];
            if f.state == "dropped" {
                remove_dropped(parse_int(key));
            }
            flags.remove(key);
        }
    }
    set("flags", flags);
}

fn on_minigame(event) {
    if event.kind == "reset" || event.kind == "ended" {
        // `Slayer_CTF::preMinigameReset` / the game gone: every flag home
        // (or gone, when the game ends) and the captures cleared.
        reset_flags(event.game, event.kind == "ended");
        // `Slayer_CTF::onClientDisplayRules`, after Slayer's rules.
        let g = minigame(event.game);
        if event.kind == "reset" && g != () && playing(g.id) {
            let line = rules_line(g.id);
            if line != () {
                tell_game(g, line);
            }
        }
    } else if event.kind == "round_end" && playing(event.game) {
        report_columns(event.game);
    } else if event.kind == "settings" && event.keys.contains("{{slayer_rules}}:mode") {
        // `Slayer_CTF::onGameModeStart`: the flags stand on their stands.
        // `onGameModeEnd`: they and the captures are gone, and Slayer's
        // report has its own columns again.
        let ending = !playing(event.game);
        reset_flags(event.game, ending);
        if ending {
            drop_report_columns(event.game);
        }
    } else if event.kind == "left" || event.kind == "team" {
        // `onClientLeaveGame` / `onClientLeaveTeam`.
        drop_flag(event.player);
    }
}

// `Slayer_CTF::resetFlags(%doNotRespawn)`: every flag of the game home, or
// with `gone` none at all, carried and dropped ones included, and this
// round's captures and tallies cleared.
fn reset_flags(game, gone) {
    // Without teams there are no flags (`on_tick`).
    let g = minigame(game);
    let gone = gone || g == () || g.teams.is_empty();
    let flags = flags();
    for b in bricks(flag_spawn()) {
        if b.game == game {
            remove_dropped(b.id);
            if gone {
                set_brick_item(b.id, ());
                flags.remove(`${b.id}`);
            } else {
                flags.send_home(b);
            }
        }
    }
    set("flags", flags);
    for p in players() {
        if p.minigame == game && get_player(p.id, "carrying") != () {
            if p.alive {
                mount_image(p.id, (), flag_slot());
            }
            set_player(p.id, "carrying", ());
        }
    }
    if g == () {
        // The game is gone, and what was kept of it with it.
        let games = get("games");
        games.remove(`${game}`);
        set("games", games);
        return;
    }
    let s = game_state(game);
    s.returns = #{};
    s.pickups_by = #{};
    s.returns_by = #{};
    set_game_state(game, s);
}

// `Slayer_CTF::prePlayerDeath`.
fn on_death(victim, killer) {
    drop_flag(victim);
}

// `Player::dropFlag`: the flag leaves the carrier's eye along their look,
// or goes straight home when dropped flags do not wait.
fn drop_flag(p) {
    let brick_id = get_player(p, "carrying");
    if brick_id == () {
        return;
    }
    set_player(p, "carrying", ());
    let me = player(p);
    if me != () && me.alive {
        mount_image(p, (), flag_slot());
    }
    let b = brick(brick_id);
    let flags = flags();
    if b == () || b.game == () {
        flags.remove(`${brick_id}`);
        set("flags", flags);
        return;
    }
    let g = minigame(b.game);
    let prefs = game_state(b.game).prefs;
    let respawn = prefs.flagdroppedrespawntime;
    if respawn <= 0 || me == () {
        flags.send_home(b);
    } else {
        // Out in front of the eye, unless a brick is in the way.
        let at = [me.ex, me.ey, me.ez];
        let ahead = {{drop_reach}};
        let hit = raycast(at, [me.lx, me.ly, me.lz], ahead, p);
        if hit == () {
            at = [me.ex + me.lx * ahead, me.ey + me.ly * ahead, me.ez + me.lz * ahead];
        }
        let fling = {{drop_throw}};
        drop_item(flag_item(), #{
            at: at,
            velocity: [me.vx + me.lx * fling, me.vy + me.ly * fling, me.vz + me.lz * fling],
            paint: b.color,
            data: #{ brick: b.id },
            seconds: respawn + 5
        });
        flags[`${b.id}`] = #{ state: "dropped", since: tick(), respawn_at: tick() + respawn * 120 };
    }
    set("flags", flags);
    // `Slayer_CTF::onFlagDrop`.
    if me != () {
        tell_game(g, `${c3()}${me.name} ${magenta()}dropped the ${flag_name(g, b)} ${c5()}flag.`);
    }
    fire_brick_input(b.id, "onFlagDropped", p);
}

// The `DropFlag` wrench output (`Player::dropFlag`): the player the row
// aims at drops the flag they carry, in a Capture the Flag game.
fn on_brick_output(output, target, params, info) {
    if output == "DropFlag" {
        let me = player(target);
        if me != () && me.minigame != () && playing(me.minigame) && me.team != () {
            drop_flag(target);
        }
    }
}

// Slayer's `serverCmdAddEvent` for this copy's restricted DropFlag
// output (`RestrictedEvent__["Player", "DropFlag"]`): with Restrict Output
// Events on, a player in the game adds it only if Slayer's rights at its
// level allow (Slayer's own outputs are its rules' to check).
fn on_event_row(p, brick, row) {
    if `${row.class}:${row.output}`.to_lower() != "player:dropflag" {
        return true;
    }
    let me = player(p);
    if me == () || me.minigame == () || !setting(me.minigame, "{{slayer_rules}}:restrict_output_events") {
        return true;
    }
    let g = minigame(me.minigame);
    let level = {{restrict_drop_flag}};
    let allowed = if level >= 0 && level <= 2 { me.admin } else { can_edit(me, g) };
    if allowed {
        true
    } else {
        `You do not have permission to use the [${row.class}, ${row.output}] event.`
    }
}
// Slayer's `canEdit`: the host, or whoever the game's Edit Rights allow
// (0 the host, 1 super admins, 2 admins, 3 the creator or someone they
// trust as themselves, 4 full trust, 5 build trust, 6 everyone).
fn can_edit(me, g) {
    if g == () {
        return false;
    }
    if me.host {
        return true;
    }
    let t = if g.owner == () { 0 } else if g.owner == me.id { 3 } else { trust(me.id, g.owner) };
    switch setting(g.id, "{{slayer_rules}}:edit_rights") {
        0 => me.host,
        1 => me.super_admin,
        2 => me.admin,
        3 => t >= 3,
        4 => t >= 2,
        5 => t >= 1,
        6 => true,
        _ => false
    }
}

// `serverCmdDropTool`, packaged: the Drop Tool key with the tools put
// away drops a carried flag, with Enable Manual Flag Dropping on.
fn on_drop_key(p) {
    let me = player(p);
    if me == () || me.minigame == () || get_player(p, "carrying") == () {
        return false;
    }
    if game_state(me.minigame).prefs.manualflagdrop == 0 {
        return false;
    }
    drop_flag(p);
    true
}

fn cmd_dropflag(p) {
    let me = player(p);
    if me == () || me.minigame == () {
        return;
    }
    if game_state(me.minigame).prefs.manualflagdrop != 0 {
        drop_flag(p);
    }
}

// `Slayer_CTF::onFlagPickup`.
fn take_flag(s, b, flags, dropped) {
    let p = s.me.id;
    if dropped == () {
        set_brick_item(b.id, ());
        // Only a flag taken from its stand counts as a pick-up.
        let state = game_state(s.game.id);
        state.pickups_by = tally(state.pickups_by, p, s.team.id);
        set_game_state(s.game.id, state);
    } else {
        remove_drop(dropped);
    }
    // Kept: no other Add-On swaps or takes off the flag while it is carried
    // (`Player::mountImage` and `unMountImage`, packaged).
    mount_image(p, flag_image(), flag_slot(), #{ paint: b.color, keep: true });
    set_player(p, "carrying", b.id);
    flags[`${b.id}`] = #{ state: "carried", since: tick(), carrier: p };
    set("flags", flags);
    tell_game(s.game, `${c3()}${s.me.name} ${magenta()}picked up the ${flag_name(s.game, b)} ${c5()}flag.`);
    if game_state(s.game.id).prefs.manualflagdrop != 0 {
        bottom_print(p, `<just:center>${c5()}Type ${c3()}/dropFlag ${c5()}to drop the flag.\n<font:Palatino Linotype:18>${c5()}(Alternatively, use the ${c3()}Drop Tool ${c5()}key when your tools are put away.)`, 4);
    }
    fire_brick_input(b.id, "onFlagPickedUp", p);
}

// `Slayer_CTF::onFlagRecovery`: a team brings its own flag home.
fn recover_flag(s, b, flags, dropped) {
    if dropped != () {
        remove_drop(dropped);
    }
    flags.send_home(b);
    set("flags", flags);
    let points = game_state(s.game.id).prefs.points_flagrecovery;
    add_score(s.me.id, points);
    if points > 0 {
        tell_game(s.game, `${c3()}${s.me.name} ${magenta()}recovered the ${flag_name(s.game, b)} ${magenta()}flag for ${c3()}${points} ${c5()}${if points == 1 { "point" } else { "points" }}.`);
    } else {
        tell_game(s.game, `${c3()}${s.me.name} ${magenta()}recovered the ${flag_name(s.game, b)} ${magenta()}flag.`);
    }
    fire_brick_input(b.id, "onFlagRecovered", s.me.id);
}

// `slyrCTF_FlagItem::onPickUp`. The flag item never goes into a tool slot:
// these rules decide what touching it does.
fn on_pickup(p, item, info) {
    if item != flag_item() {
        return;
    }
    let s = side(p);
    let brick_id = if info.drop != () {
        if info.data == () { () } else { info.data.brick }
    } else {
        info.spawner
    };
    if s == () || brick_id == () {
        return false;
    }
    let b = brick(brick_id);
    if b == () || b.kind != flag_spawn() || b.game != s.game.id {
        if b != () && b.game != () && b.game != s.game.id {
            center_print(p, "That flag is not part of your minigame.", 2);
        }
        return false;
    }
    let flags = flags();
    let f = flag_of(flags, b.id);
    if f == () || (info.drop != () && tick() - f.since < pickup_guard_ticks()) {
        return false;
    }
    let prefs = game_state(s.game.id).prefs;
    let carrying = get_player(p, "carrying") != ();
    if b.color == s.team.color {
        // Touching our own flag: only a dropped one does anything.
        if info.drop != () {
            if prefs.flagrecovery == 1 {
                recover_flag(s, b, flags, info.drop);
            } else if prefs.flagrecovery == 2 && !carrying {
                take_flag(s, b, flags, info.drop);
            }
        }
        return false;
    }
    if carrying {
        return false;
    }
    if locked(b.id, s.team.color) {
        bottom_print(p, `<just:center>${c5()}{{locked_flag}}`, 1);
        return false;
    }
    let neutral = teams_of_color(s.game, b.color).is_empty();
    if neutral && prefs.neutralflags == 0 {
        bottom_print(p, `${c5()}You may not pick up neutral flags.`, 1);
        return false;
    }
    if !neutral && prefs.neutralflags == 2 {
        bottom_print(p, `${c5()}You may only pick up neutral flags.`, 1);
        return false;
    }
    if !neutral && prefs.requireenemyplayers != 0 {
        let anyone = false;
        for o in players() {
            if o.minigame == s.game.id && o.team != () {
                let t = team_by_id(s.game, o.team);
                if t != () && t.color == b.color {
                    anyone = true;
                }
            }
        }
        if !anyone {
            bottom_print(p, `${magenta()}You cannot capture this flag because nobody is on ${flag_name(s.game, b)}${magenta()}.`, 1);
            return false;
        }
    }
    take_flag(s, b, flags, info.drop);
    false
}

// `Slayer_CTF::isFlagAtHome`: 1 when a flag of `color` stands on one of the
// game's Flag Spawns, 0 when it has Flag Spawns of that colour but none
// does, -1 when it has none.
fn flag_at_home(g, color) {
    let flags = flags();
    let stands = false;
    for b in bricks(flag_spawn()) {
        if b.game == g.id && b.color == color {
            let f = flag_of(flags, b.id);
            if f != () && f.state == "home" {
                return 1;
            }
            stands = true;
        }
    }
    if stands { 0 } else { -1 }
}

// `slyrCTF_flagReturnTriggerData::onEnterTrigger`: a carrier reaches a Flag
// Spawn or a Flag Return Point.
fn on_zone(p, brick_id, event) {
    if event != "enter" {
        return;
    }
    let carried = get_player(p, "carrying");
    if carried == () {
        return;
    }
    let s = side(p);
    let b = brick(brick_id);
    let home = brick(carried);
    if s == () || b == () || home == () || b.game != s.game.id {
        return;
    }
    let g = s.game;
    let prefs = game_state(g.id).prefs;
    let neutral = teams_of_color(g, b.color).is_empty();
    let at_return = b.kind == flag_return();
    if !(b.color == s.team.color || (neutral && at_return)) {
        return;
    }
    if locked(b.id, s.team.color) {
        let what = if at_return { "{{return_ui}}" } else { "{{flag_ui}}" };
        bottom_print(p, `<just:center>${c5()}This ${what} {{locked_return}}`, 4);
        return;
    }
    let flags = flags();
    if home.color == s.team.color {
        // Our own flag, carried back to its stand: a recovery.
        if !at_return {
            let here = flag_of(flags, b.id);
            if here == () || here.state != "home" {
                mount_image(p, (), flag_slot());
                set_player(p, "carrying", ());
                recover_flag(s, home, flags, ());
            }
        }
        return;
    }
    if prefs.flagreturnonlyatreturnbrick != 0 && !at_return {
        bottom_print(p, `${c5()}You must return the flag to a Flag Return Point.`, 4);
        return;
    }
    if home.color == b.color && neutral {
        bottom_print(p, `${c5()}You cannot return a flag to a Flag Return Point of the same color.`, 4);
        return;
    }
    if prefs.returnwithoutown == 0 {
        let missing = if at_return {
            flag_at_home(g, s.team.color) == 0
        } else {
            let here = flag_of(flags, b.id);
            here == () || here.state != "home"
        };
        if missing {
            tell(p, `${c5()}You cannot return this flag while your flag is missing.`);
            return;
        }
    }
    capture(s, home, flags, prefs);
}

// `Slayer_CTF::onFlagReturn`: a capture.
fn capture(s, home, flags, prefs) {
    let p = s.me.id;
    let g = s.game;
    let points = prefs.points_flag;
    add_score(p, points);
    let msg = if points > 0 {
        `${c3()}${s.me.name} ${magenta()}returned the ${flag_name(g, home)} ${magenta()}flag for ${c3()}${points} ${c5()}${if points == 1 { "point." } else { "points." }}`
    } else {
        `${c3()}${s.me.name} ${magenta()}returned the ${flag_name(g, home)} ${magenta()}flag.`
    };
    let state = game_state(g.id);
    let key = `${s.team.id}`;
    let done = if key in state.returns { state.returns[key] } else { 0 };
    let win = prefs.flagreturnstowin;
    if win > 0 {
        let remain = win - done - 1;
        if remain > 0 {
            msg += ` ${c3()}${s.team.name} ${c5()}needs ${c3()}${remain} ${c5()}flag ${if remain == 1 { "capture" } else { "captures" }} to win.`;
        }
    }
    let respawn = prefs.flagreturnedrespawntime;
    if respawn > 0 {
        flags[`${home.id}`] = #{ state: "waiting", since: tick(), respawn_at: tick() + respawn * 120 };
        msg += ` The flag will respawn in ${c3()}${respawn} ${c5()}${if respawn == 1 { "second." } else { "seconds." }}`;
    } else {
        flags.send_home(home);
    }
    set("flags", flags);
    tell_game(g, msg);
    mount_image(p, (), flag_slot());
    set_player(p, "carrying", ());
    // On the captured flag's own Flag Spawn, as the original.
    fire_brick_input(home.id, "onFlagReturned", p);
    state.returns[key] = done + 1;
    state.returns_by = tally(state.returns_by, p, s.team.id);
    set_game_state(g.id, state);
    if win > 0 && done + 1 >= win {
        // `Slayer_MiniGameSO::endRound(%team)`: Slayer announces it and
        // resets after its time between rounds.
        end_round(g.id, #{ teams: [s.team.id] });
    }
}

// `Slayer_CTF::onClientDisplayRules`: how many captures win, or ().
fn rules_line(game) {
    let win = setting(game, "flag_returns_to_win");
    if win > 0 {
        `${c5()} + ${c3()}${win} ${c5()}flag ${if win == 1 { "capture" } else { "captures" }} - The first team to capture ${c3()}${win} ${c5()}${if win == 1 { "flag" } else { "flags" }} wins.`
    } else {
        ()
    }
}

// `/ctf`: the rules of this game's Capture the Flag. Its settings are in
// the Mini-Game window's Add-On Settings.
fn cmd_ctf(p) {
    let me = player(p);
    if me == () || me.minigame == () || !playing(me.minigame) {
        tell(p, `${c5()}Your mini-game is not playing Capture the Flag.`);
        return;
    }
    let line = rules_line(me.minigame);
    if line != () {
        tell(p, line);
    }
    let prefs = prefs_of(me.minigame);
    for k in prefs.keys() {
        tell(p, `${c5()}${k}: ${c3()}${prefs[k]}`);
    }
}
