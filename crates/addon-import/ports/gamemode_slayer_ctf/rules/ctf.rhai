// Slayer Capture the Flag (Greek2me's Gamemode_Slayer_CTF), rewritten for
// Blockland ReImagined. It plays in any mini-game with teams (Slayer's, set
// up with /teams) whose builder placed Flag Spawn bricks: a flag stands on
// each, in the brick's paint colour, and belongs to the team of that colour
// (a colour no team has is a neutral flag).
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

// Slayer CTF's preferences (`preferences.cs`), at their defaults, per game.
fn default_prefs() {
    #{
        flagreturnstowin: {{pref_returns_to_win}},
        flagreturnonlyatreturnbrick: {{pref_return_only_at_return}},
        returnwithoutown: {{pref_return_without_own}},
        neutralflags: {{pref_neutral_flags}},
        points_flag: {{pref_points_flag}},
        points_flagrecovery: {{pref_points_recovery}},
        flagrecovery: {{pref_flag_recovery}},
        flagdroppedrespawntime: {{pref_dropped_respawn}},
        flagreturnedrespawntime: {{pref_returned_respawn}},
        manualflagdrop: {{pref_manual_drop}},
        requireenemyplayers: {{pref_require_enemies}}
    }
}

// Per game: preferences, captures by team id, and when a won round resets.
fn game_state(game) {
    let games = get("games");
    let key = `${game}`;
    if key in games {
        let s = games[key];
        for k in default_prefs().keys() {
            if !(k in s.prefs) {
                s.prefs[k] = default_prefs()[k];
            }
        }
        return s;
    }
    #{ prefs: default_prefs(), returns: #{}, reset_at: () }
}
fn set_game_state(game, s) {
    let games = get("games");
    games[`${game}`] = s;
    set("games", games);
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

fn tell_game(g, text) {
    for m in g.members {
        tell(m, text);
    }
}

// A team game this player plays in, with their team, or ().
fn side(p) {
    let me = player(p);
    if me == () || me.minigame == () || me.team == () {
        return ();
    }
    let g = minigame(me.minigame);
    if g == () {
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
        if g == () || g.teams.is_empty() {
            continue;
        }
        if f == () {
            // `onMiniGameBrickAdded`: a new Flag Spawn gets its flag.
            flags.send_home(b);
        } else if (f.state == "dropped" || f.state == "waiting") && tick() >= f.respawn_at {
            // `Slayer_CTF::flagRespawnTick`.
            tell_game(g, `${magenta()}The ${flag_name(g, b)} ${magenta()}flag was respawned.`);
            remove_dropped(b.id);
            flags.send_home(b);
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
    // Rounds won (`endRound`) reset when their time is up.
    let games = get("games");
    for key in games.keys() {
        let s = games[key];
        if s.reset_at != () && tick() >= s.reset_at {
            s.reset_at = ();
            games[key] = s;
            reset_minigame(parse_int(key));
        }
    }
    set("games", games);
}

fn on_minigame(event) {
    if event.kind == "reset" || event.kind == "ended" {
        // `Slayer_CTF::preMinigameReset` / `onGameModeEnd`: every flag home
        // (or gone, when the game ends) and the captures cleared.
        let flags = flags();
        for b in bricks(flag_spawn()) {
            if b.game == event.game {
                remove_dropped(b.id);
                if event.kind == "reset" {
                    flags.send_home(b);
                } else {
                    set_brick_item(b.id, ());
                    flags.remove(`${b.id}`);
                }
            }
        }
        set("flags", flags);
        for p in players() {
            if p.minigame == event.game {
                set_player(p.id, "carrying", ());
            }
        }
        let s = game_state(event.game);
        s.returns = #{};
        s.reset_at = ();
        set_game_state(event.game, s);
    } else if event.kind == "left" || event.kind == "team" {
        // `onClientLeaveGame` / `onClientLeaveTeam`.
        drop_flag(event.player);
    }
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
    } else {
        remove_drop(dropped);
    }
    mount_image(p, flag_image(), flag_slot(), b.color);
    set_player(p, "carrying", b.id);
    flags[`${b.id}`] = #{ state: "carried", since: tick(), carrier: p };
    set("flags", flags);
    tell_game(s.game, `${c3()}${s.me.name} ${magenta()}picked up the ${flag_name(s.game, b)} ${c5()}flag.`);
    if game_state(s.game.id).prefs.manualflagdrop != 0 {
        bottom_print(p, `${c5()}Type ${c3()}/dropFlag ${c5()}to drop the flag.`, 4);
    }
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
    state.returns[key] = done + 1;
    if win > 0 && done + 1 >= win && state.reset_at == () {
        // `Slayer_MiniGameSO::endRound(%team)`, with Slayer's default 20
        // seconds between rounds.
        tell_game(g, `${c3()}${s.team.name} ${magenta()}won this round. Resetting in 20 seconds.`);
        state.reset_at = tick() + 20 * 120;
    }
    set_game_state(g.id, state);
}

// `/ctf`: the preferences; the game's owner or an admin changes one with
// `/ctf <preference> <value>` (Slayer's GUI did this).
fn cmd_ctf(p, line) {
    let me = player(p);
    if me == () || me.minigame == () {
        tell(p, `${c5()}You are not in a mini-game.`);
        return;
    }
    let g = minigame(me.minigame);
    let state = game_state(g.id);
    let parts = [];
    for w in line.split(" ") {
        if w != "" {
            parts.push(w);
        }
    }
    if parts.is_empty() {
        let win = state.prefs.flagreturnstowin;
        if win > 0 {
            tell(p, `${c5()} + ${c3()}${win} ${c5()}flag ${if win == 1 { "capture" } else { "captures" }} - The first team to capture ${c3()}${win} ${c5()}${if win == 1 { "flag" } else { "flags" }} wins.`);
        }
        for k in state.prefs.keys() {
            tell(p, `${c5()}${k}: ${c3()}${state.prefs[k]}`);
        }
        return;
    }
    if !(me.admin || g.owner == p) {
        tell(p, `${c5()}Only the mini-game's owner or an admin can change Capture the Flag.`);
        return;
    }
    let key = parts[0].to_lower();
    if !(key in default_prefs()) || parts.len() < 2 {
        tell(p, `${c5()}Usage: /ctf <preference> <value>. Type /ctf to see them.`);
        return;
    }
    let word = parts[1];
    let negative = word.starts_with("-");
    let digits = if negative { word.sub_string(1) } else { word };
    if digits == "" || digits.len() > 5 {
        tell(p, `${c5()}The value is a whole number.`);
        return;
    }
    for c in digits.chars() {
        if c < '0' || c > '9' {
            tell(p, `${c5()}The value is a whole number.`);
            return;
        }
    }
    let value = parse_int(digits);
    if negative {
        value = -value;
    }
    state.prefs[key] = value;
    set_game_state(g.id, state);
    tell_game(g, `${c3()}${me.name} ${c5()}set ${c3()}${key} ${c5()}to ${c3()}${value}${c5()}.`);
}
