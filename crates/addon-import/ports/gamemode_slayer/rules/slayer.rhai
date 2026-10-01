// Slayer's team rules (Greek2me's Gamemode_Slayer), rewritten for
// Blockland ReImagined. The engine keeps the teams of each mini-game, stops
// teammates hurting each other while friendly fire is off and sends team
// chat to the team; these rules decide who joins which team and where they
// spawn, as Slayer's scripts did.
//
// Slayer set teams up in its GUI. Until a settings screen exists, the
// mini-game's owner or an admin types:
//   /teams add <colour> <name>    a team in that paint colour (0 to 63)
//   /teams remove <name>
//   /teams friendlyfire on|off
// and everyone has Slayer's own:
//   /teams join <name>, /teams leave, /teams list, /teams count,
//   /teams score, /teams listmembers <name>, /teams balance (owner)
//   /slayer reset (owner), /slayer rules
// and Slayer's short forms: /joinTeam <name>, /leaveTeam, /listTeams
// (/teamList), /teamCount, /teamScore.

// Slayer's chat colours (\c3 names, \c5 text).
fn c3() { "" }
fn c5() { "" }

fn team_spawn() { "{{namespace}}:brick/brickslyrspawnpointdata" }
fn plain_spawn() { "v20/brick/brickspawnpointdata" }

// Slayer's per-mini-game preferences, at Slayer's defaults.
fn prefs(game) {
    let games = get("games");
    let key = `${game}`;
    if key in games {
        return games[key];
    }
    #{ auto_sort: true, lock: false, notify: true, balance_new: true }
}
fn set_prefs(game, p) {
    let games = get("games");
    games[`${game}`] = p;
    set("games", games);
}

// `canEdit`: the mini-game's owner or an admin.
fn can_edit(p, g) {
    let me = player(p);
    me != () && (me.admin || g.owner == p)
}

fn team_by_name(g, name) {
    let wanted = name.to_lower();
    for t in g.teams {
        if t.name.to_lower() == wanted {
            return t;
        }
    }
    ()
}
fn team_by_id(g, id) {
    for t in g.teams {
        if t.id == id {
            return t;
        }
    }
    ()
}
fn members(g, team) {
    let out = [];
    for p in players() {
        if p.minigame == g.id && p.team == team {
            out.push(p);
        }
    }
    out
}

fn tell_game(g, text) {
    for m in g.members {
        tell(m, text);
    }
}

// How many of the game's players each team has, by team id.
fn counts(g) {
    let out = #{};
    for t in g.teams {
        out[`${t.id}`] = 0;
    }
    for p in players() {
        if p.minigame == g.id && p.team != () && `${p.team}` in out {
            out[`${p.team}`] += 1;
        }
    }
    out
}

// `Slayer_TeamHandlerSG::pickTeam`: the team farthest below its share (all
// teams weigh 1), ties broken at random. Called on the counts by team id
// (`counts.pick_team(g, p)`), which it updates, so several players sorted
// in one go spread out.
fn pick_team(g, p) {
    let least = 1000000;
    let best = [];
    for t in g.teams {
        let n = this[`${t.id}`];
        if n < least {
            least = n;
            best = [t];
        } else if n == least {
            best.push(t);
        }
    }
    if best.is_empty() {
        return ();
    }
    let i = (hash3(seed(), tick(), p, best.len()) * best.len()).to_int();
    let team = best[i];
    this[`${team.id}`] += 1;
    team
}

// `Slayer_TeamSO::addMember`: join, then come back on the new side.
fn join(p, team) {
    set_team(p, team.id);
    let me = player(p);
    if me != () && me.alive {
        respawn(p);
    }
}

// `Slayer_TeamHandlerSG::autoSort` for each player in `list` (ids).
fn auto_sort(g, list) {
    let counts = counts(g);
    for p in list {
        let team = counts.pick_team(g, p);
        if team != () {
            join(p, team);
        }
    }
}

// `Slayer_TeamSO::balanceTeam`, until no team has more than one player
// over another: the lowest scorer on the largest team moves to the
// smallest. `on` is each player's team as these rules leave it.
fn even_out(g, counts, on) {
    let scores = #{};
    for p in players() {
        scores[`${p.id}`] = p.score;
    }
    let moves = 0;
    while moves < 64 {
        let small = ();
        let big = ();
        for t in g.teams {
            let n = counts[`${t.id}`];
            if small == () || n < counts[`${small.id}`] {
                small = t;
            }
            if big == () || n > counts[`${big.id}`] {
                big = t;
            }
        }
        if small == () || counts[`${big.id}`] - counts[`${small.id}`] <= 1 {
            return;
        }
        let pick = ();
        for key in on.keys() {
            if on[key] == big.id && (pick == () || scores[key] < scores[pick]) {
                pick = key;
            }
        }
        if pick == () {
            return;
        }
        on[pick] = small.id;
        counts[`${big.id}`] -= 1;
        counts[`${small.id}`] += 1;
        join(parse_int(pick), small);
        moves += 1;
    }
}

fn on_minigame(event) {
    let g = minigame(event.game);
    if g == () || g.teams.is_empty() {
        return;
    }
    let prefs = prefs(g.id);
    if event.kind == "joined" && prefs.auto_sort {
        auto_sort(g, [event.player]);
    } else if event.kind == "teams" && prefs.balance_new {
        // `Slayer_TeamHandlerSG::onNewTeam`: everyone without a team is
        // sorted, then teams are evened out.
        let counts = counts(g);
        let on = #{};
        for p in players() {
            if p.minigame == g.id && p.team != () && `${p.team}` in counts {
                on[`${p.id}`] = p.team;
            }
        }
        for p in players() {
            if p.minigame == g.id && !(`${p.id}` in on) {
                let team = counts.pick_team(g, p.id);
                if team != () {
                    on[`${p.id}`] = team.id;
                    join(p.id, team);
                }
            }
        }
        even_out(g, counts, on);
    } else if event.kind == "team" && event.team != () {
        let team = team_by_id(g, event.team);
        let me = player(event.player);
        if team != () && me != () {
            tell(event.player, `${c5()}You have joined ${c3()}${team.name}`);
            if prefs.notify {
                for m in g.members {
                    if m != event.player {
                        tell(m, `${c3()}${me.name} ${c5()}has joined ${c3()}${team.name}`);
                    }
                }
            }
        }
    }
}

// `Slayer_MiniGameSO::pickSpawnPoint`: a team spawn of the game's in the
// team's colour, else a plain spawn brick of the game's; open ones first.
fn on_pick_spawn(p) {
    let me = player(p);
    if me == () || me.minigame == () || me.team == () {
        return ();
    }
    let g = minigame(me.minigame);
    let team = team_by_id(g, me.team);
    if team == () {
        return ();
    }
    let spawns = [];
    for b in bricks(team_spawn()) {
        if b.game == g.id && b.color == team.color {
            spawns.push(b);
        }
    }
    if spawns.is_empty() {
        for b in bricks(plain_spawn()) {
            if b.game == g.id {
                spawns.push(b);
            }
        }
    }
    if spawns.is_empty() {
        return ();
    }
    let open = [];
    for b in spawns {
        let blocked = false;
        for o in players() {
            if o.alive && o.id != p {
                let dx = o.x - b.x;
                let dz = o.z - b.z;
                if dx * dx + dz * dz < 2.25 && (o.y - b.y).abs() < 3.0 {
                    blocked = true;
                }
            }
        }
        if !blocked {
            open.push(b);
        }
    }
    let pool = if open.is_empty() { spawns } else { open };
    let i = (hash3(seed(), tick(), p, pool.len()) * pool.len()).to_int();
    pool[i].id
}

fn words(line) {
    let out = [];
    for w in line.split(" ") {
        if w != "" {
            out.push(w);
        }
    }
    out
}
// A typed whole number, or -1.
fn whole(word) {
    if word == "" || word.len() > 6 {
        return -1;
    }
    for c in word.chars() {
        if c < '0' || c > '9' {
            return -1;
        }
    }
    parse_int(word)
}
fn rest(parts, from) {
    let out = "";
    let i = from;
    while i < parts.len() {
        if out != "" {
            out += " ";
        }
        out += parts[i];
        i += 1;
    }
    out
}

fn cmd_teams(p, line) {
    let me = player(p);
    if me == () || me.minigame == () {
        tell(p, `${c5()}You are not in a mini-game.`);
        return;
    }
    let g = minigame(me.minigame);
    let parts = words(line);
    let cmd = if parts.is_empty() { "" } else { parts[0].to_lower() };
    let name = rest(parts, 1);
    let mine = if me.team == () { () } else { team_by_id(g, me.team) };
    let prefs = prefs(g.id);
    if cmd == "" {
        tell(p, `${c5()}Teams: /teams join <name>, leave, list, count, score, listmembers <name>, balance.`);
        if can_edit(p, g) {
            tell(p, `${c5()}Set up: /teams add <colour> <name>, remove <name>, friendlyfire on|off, autosort on|off, lock on|off.`);
        }
    } else if cmd == "join" {
        if prefs.lock && mine != () {
            tell(p, `${c5()}Teams are locked, you cannot change teams.`);
            return;
        }
        if name == "" {
            tell(p, `${c5()}Please enter a team to join. Type ${c3()}/teams join Team Name ${c5()}to join a team. Type ${c3()}/teams list ${c5()}to view a list of teams.`);
            return;
        }
        let team = team_by_name(g, name);
        if team == () {
            tell(p, `${c5()}Couldn't find team:${c3()} ${name}`);
            return;
        }
        if mine != () && mine.id == team.id {
            tell(p, `${c5()}You're already on ${c3()}${team.name}${c5()}.`);
            return;
        }
        join(p, team);
    } else if cmd == "leave" {
        if mine == () {
            tell(p, `${c5()}You aren't on a team.`);
            return;
        }
        if (prefs.lock || prefs.auto_sort) && !can_edit(p, g) {
            tell(p, `${c5()}You cannot leave your team.`);
            return;
        }
        tell(p, `${c5()}You have left ${c3()}${mine.name}`);
        if prefs.notify {
            for m in g.members {
                if m != p {
                    tell(m, `${c3()}${me.name} ${c5()}has left ${c3()}${mine.name}`);
                }
            }
        }
        set_team(p, ());
        respawn(p);
    } else if cmd == "list" || cmd == "count" || cmd == "score" {
        if g.teams.is_empty() {
            tell(p, `${c5()}There are no teams.`);
            return;
        }
        for t in g.teams {
            let list = members(g, t.id);
            if cmd == "list" {
                tell(p, `${c3()}${t.name}`);
            } else if cmd == "count" {
                tell(p, `${c3()}(${list.len()}) ${t.name}`);
            } else {
                let score = 0;
                for m in list {
                    score += m.score;
                }
                tell(p, `${c3()}(${score}) ${t.name}`);
            }
        }
    } else if cmd == "listmembers" {
        let team = team_by_name(g, name);
        if team == () {
            tell(p, `${c5()}Couldn't find team:${c3()} ${name}`);
            return;
        }
        let list = members(g, team.id);
        tell(p, `${c3()}${team.name} (${list.len()})${c5()}:`);
        for m in list {
            tell(p, ` ${c5()}+ ${c3()}${m.name}`);
        }
    } else if !can_edit(p, g) {
        tell(p, `${c5()}Please enter a valid command.`);
    } else if cmd == "balance" {
        // Everyone is sorted afresh, from no teams.
        let counts = #{};
        for t in g.teams {
            counts[`${t.id}`] = 0;
        }
        for m in players() {
            if m.minigame == g.id {
                let team = counts.pick_team(g, m.id);
                if team != () && team.id != m.team {
                    join(m.id, team);
                }
            }
        }
        tell_game(g, `${c3()}${me.name} ${c5()}balanced ${c3()}all ${c5()}teams.`);
    } else if cmd == "add" {
        let colour = if parts.len() > 1 { whole(parts[1]) } else { -1 };
        let title = rest(parts, 2);
        if colour < 0 || colour > 63 || title == "" {
            tell(p, `${c5()}Usage: /teams add <colour 0-63> <name>`);
            return;
        }
        if team_by_name(g, title) != () {
            tell(p, `${c5()}There is already a team called ${c3()}${title}`);
            return;
        }
        let list = g.teams;
        list.push(#{ name: title, color: colour });
        set_teams(g.id, list, #{ friendly_fire: g.friendly_fire, ally_same_color: g.ally_same_color });
        tell_game(g, `${c3()}${me.name} ${c5()}added the team ${c3()}${title}${c5()}.`);
    } else if cmd == "remove" {
        let team = team_by_name(g, name);
        if team == () {
            tell(p, `${c5()}Couldn't find team:${c3()} ${name}`);
            return;
        }
        let list = [];
        for t in g.teams {
            if t.id != team.id {
                list.push(t);
            }
        }
        set_teams(g.id, list, #{ friendly_fire: g.friendly_fire, ally_same_color: g.ally_same_color });
        tell_game(g, `${c3()}${me.name} ${c5()}removed the team ${c3()}${team.name}${c5()}.`);
    } else if cmd == "friendlyfire" || cmd == "autosort" || cmd == "lock" {
        let on = parts.len() > 1 && ["on", "1", "true", "yes"].contains(parts[1].to_lower());
        if cmd == "friendlyfire" {
            set_teams(g.id, g.teams, #{ friendly_fire: on, ally_same_color: g.ally_same_color });
        } else {
            if cmd == "autosort" {
                prefs.auto_sort = on;
            } else {
                prefs.lock = on;
            }
            set_prefs(g.id, prefs);
        }
        let state = if on { "on" } else { "off" };
        tell_game(g, `${c3()}${me.name} ${c5()}turned ${c3()}${cmd} ${state}${c5()}.`);
    } else {
        tell(p, `${c5()}Please enter a valid command.`);
    }
}

// Slayer's short forms (`serverCmdJoinTeam` and the rest).
fn cmd_jointeam(p, name) { cmd_teams(p, `join ${name}`) }
fn cmd_leaveteam(p) { cmd_teams(p, "leave") }
fn cmd_listteams(p) { cmd_teams(p, "list") }
fn cmd_teamlist(p) { cmd_teams(p, "list") }
fn cmd_teamcount(p) { cmd_teams(p, "count") }
fn cmd_teamscore(p) { cmd_teams(p, "score") }

fn cmd_slayer(p, line) {
    let me = player(p);
    let parts = words(line);
    let cmd = if parts.is_empty() { "" } else { parts[0].to_lower() };
    if me == () || me.minigame == () || cmd == "" {
        tell(p, `${c5()}This server is running Slayer, by Greek2me (Blockland ID 11902).`);
        return;
    }
    let g = minigame(me.minigame);
    if cmd == "reset" {
        if can_edit(p, g) {
            reset_minigame(g.id);
        }
    } else if cmd == "rules" {
        tell(p, `${c3()}${g.title}`);
        if !g.teams.is_empty() {
            tell(p, `${c5()} + ${c3()}${g.teams.len()} ${c5()}teams. Friendly fire is ${if g.friendly_fire { "on" } else { "off" }}.`);
        }
    } else {
        tell(p, `${c5()}This server is running Slayer, by Greek2me (Blockland ID 11902).`);
    }
}
