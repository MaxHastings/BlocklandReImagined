// Slayer (Greek2me's Gamemode_Slayer), rewritten for Blockland ReImagined.
// The engine keeps the teams of each mini-game, stops teammates hurting
// each other while friendly fire is off and sends team chat to the team;
// these rules decide who joins which team, where they spawn, and how a
// round is won, as Slayer's scripts did.
//
// Slayer's settings are in the Mini-Game window's Add-On Settings: the game
// mode, lives, points and time to win, time between rounds, the pre-round
// countdown, the team rules, and each team's own (lives, auto sort, max
// players, lock, win on time up). Its chat commands work too:
//   /teams join <name>, /teams leave, /teams list, /teams count,
//   /teams score, /teams listmembers <name>, /teams balance (owner)
//   /slayer reset (owner), /slayer rules
// and Slayer's short forms: /joinTeam <name>, /leaveTeam, /listTeams
// (/teamList), /teamCount, /teamScore. The mini-game's owner or an admin
// may also type /teams add <colour> <name>, /teams remove <name>,
// /teams friendlyfire|autosort|lock on|off.

// Slayer's chat colours (\c3 names, \c5 text).
fn c3() { "" }
fn c5() { "" }
fn magenta() { "<color:ff00ff>" }
fn yellow() { "<color:ffff00>" }

fn team_spawn() { "{{namespace}}:brick/brickslyrspawnpointdata" }
fn plain_spawn() { "v20/brick/brickspawnpointdata" }

// Slayer's game modes (`server/defaults/game-modes.cs`): the first plays
// without teams; every other mode, Capture the Flag's included, uses them.
fn free_for_all() { "{{dm_class}}" }
fn uses_teams(game) { setting(game, "mode") != free_for_all() }
fn mode_name(game) {
    let mode = setting(game, "mode");
    if mode == "{{dm_class}}" {
        "{{dm_name}}"
    } else if mode == "{{tdm_class}}" {
        "{{tdm_name}}"
    } else {
        minigame(game).title
    }
}
fn is_key(keys, key) { keys.contains(`{{rules}}:${key}`) }

// One round of a mini-game: when its clock started (after the pre-round
// countdown), the countdown and time announcements already made, and once
// it is over, when the reset comes. `lives` is the lives setting players
// last got, so a change adds or takes the difference.
fn round_of(game) {
    let games = get("games");
    let key = `${game}`;
    if key in games {
        return games[key];
    }
    #{ clock: tick(), countdown: (), counted: (), told: (), reset_at: (),
       shown: (), lives: setting(game, "lives") }
}
fn set_round(game, r) {
    let games = get("games");
    games[`${game}`] = r;
    set("games", games);
}
fn running(game) {
    let r = round_of(game);
    r.reset_at == () && r.countdown == () && !minigame(game).round_over
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
// `Slayer_TeamSO::getScore`: its members' scores together.
fn team_score(g, team) {
    let score = 0;
    for m in members(g, team) {
        score += m.score;
    }
    score
}
fn plural(n, one, many) { if n == 1 { one } else { many } }

// `MiniGameSO::messageAll`.
fn tell_game(g, text) { tell_minigame(g.id, text); }

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

// Whether `team` takes players from auto sort (`Auto Sort`, a weight above
// 0) and has room (`Max Players`, -1 for no limit).
fn sorts_into(g, team, n) {
    let max = team_setting(g.id, team.id, "team_max_players");
    team_setting(g.id, team.id, "team_sort")
        && team_setting(g.id, team.id, "team_sort_weight") > 0
        && !(max >= 0 && n >= max)
}

// `Slayer_TeamHandlerSG::pickTeam`: of the teams that sort and have room,
// the one farthest below its share (members over its sort weight), ties
// broken at random. Called on the counts by team id
// (`counts.pick_team(g, p)`), which it updates, so several players sorted
// in one go spread out.
fn pick_team(g, p) {
    let best = [];
    let least_n = 0;
    let least_w = 1;
    for t in g.teams {
        let n = this[`${t.id}`];
        if !sorts_into(g, t, n) {
            continue;
        }
        let w = team_setting(g.id, t.id, "team_sort_weight");
        // n / w against least_n / least_w, in whole numbers.
        if best.is_empty() || n * least_w < least_n * w {
            least_n = n;
            least_w = w;
            best = [t];
        } else if n * least_w == least_n * w {
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
            if (small == () || n < counts[`${small.id}`]) && sorts_into(g, t, n) {
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

// The lives a member starts a round with: their team's, when it sets
// them, else the game's (`Slayer_MiniGameSO::reset`). 0 is no limit.
fn lives_for(g, team) {
    if team != () {
        let lives = team_setting(g.id, team, "team_lives");
        if lives >= 0 {
            return lives;
        }
    }
    setting(g.id, "lives")
}
// Out of lives: no respawn until the round resets (`setDead`).
fn knock_out(p) {
    set_player(p, "out", true);
    hold_respawn(p, true);
}
fn revive(p) {
    set_player(p, "out", false);
    hold_respawn(p, false);
}
fn uses_lives(g) {
    if setting(g.id, "lives") > 0 {
        return true;
    }
    for t in g.teams {
        if team_setting(g.id, t.id, "team_lives") > 0 {
            return true;
        }
    }
    false
}

// `Slayer_MiniGameSO::victoryCheck_Lives`: the last team, or the last
// player without a team, with anyone left in. `()` while more than one
// remains; a winner list (perhaps empty, nobody) once at most one does.
fn lives_winner(g) {
    let teams = uses_teams(g.id) && !g.teams.is_empty();
    let count = 0;
    let winner = #{ teams: [], players: [] };
    let winner_color = -1;
    let left = #{};
    for p in players() {
        if p.minigame == g.id && !get_player(p.id, "out") {
            left[`${p.id}`] = p;
        }
    }
    if teams {
        for t in g.teams {
            let alive = false;
            for key in left.keys() {
                if left[key].team == t.id {
                    alive = true;
                }
            }
            if !alive {
                continue;
            }
            if g.ally_same_color && t.color == winner_color {
                winner.teams.push(t.id);
            } else {
                winner = #{ teams: [t.id], players: [] };
                winner_color = t.color;
                count += 1;
            }
        }
        if count > 1 {
            return ();
        }
    }
    for key in left.keys() {
        let p = left[key];
        if !teams || p.team == () {
            winner = #{ teams: [], players: [p.id] };
            count += 1;
        }
    }
    if count == 1 {
        winner
    } else if count > 1 {
        ()
    } else {
        #{ teams: [], players: [] }
    }
}
fn check_lives(g) {
    if !running(g.id) || !uses_lives(g) {
        return;
    }
    let winner = lives_winner(g);
    if winner != () {
        end_round(g.id, winner);
    }
}

// `Slayer_MiniGameSO::victoryCheck_Points`: the first team, else the first
// player without a team, to reach the points to win.
fn check_points(g) {
    let points = setting(g.id, "points");
    if points <= 0 {
        return;
    }
    let teams = uses_teams(g.id) && !g.teams.is_empty();
    if teams {
        for t in g.teams {
            if team_score(g, t.id) >= points {
                end_round(g.id, #{ teams: [t.id] });
                return;
            }
        }
    }
    for p in players() {
        if p.minigame == g.id && (!teams || p.team == ()) && p.score >= points {
            end_round(g.id, #{ players: [p.id] });
            return;
        }
    }
}

// `Slayer_MiniGameSO::victoryCheck_Time`'s announcements: every ten
// minutes, each minute of the last five, 30, 20 and 10 seconds, and each
// of the last five seconds.
fn announces(seconds) {
    if seconds % 60 == 0 {
        seconds % 600 == 0 || seconds <= 300
    } else {
        seconds == 30 || seconds == 20 || seconds == 10 || seconds <= 5
    }
}
// Time up: the highest scoring team (a team that wins on time up beats
// any score), or player without a team, with a score above 0; ties share.
fn time_winner(g) {
    let least = 0;
    let winner = #{ teams: [], players: [] };
    let on_time_up = false;
    if uses_teams(g.id) && !g.teams.is_empty() {
        for t in g.teams {
            let score = team_score(g, t.id);
            let wins = team_setting(g.id, t.id, "team_win_on_time_up");
            if (score > least || wins) && !on_time_up {
                winner = #{ teams: [t.id], players: [] };
                least = score;
                if wins {
                    on_time_up = true;
                }
            } else if (score == least && least > 0 && !on_time_up) || (on_time_up && wins) {
                winner.teams.push(t.id);
            }
        }
    }
    for p in players() {
        if p.minigame != g.id || (uses_teams(g.id) && p.team != ()) {
            continue;
        }
        if p.score > least {
            winner = #{ teams: [], players: [p.id] };
            least = p.score;
        } else if p.score == least && least > 0 {
            winner.players.push(p.id);
        }
    }
    winner
}
fn check_time(g, r) {
    let minutes = setting(g.id, "time");
    if minutes <= 0 {
        return r;
    }
    let left = r.clock + minutes * 60 * 120 - tick();
    if left <= 0 {
        end_round(g.id, time_winner(g));
        return r;
    }
    let seconds = (left + 119) / 120;
    if r.told != () && seconds < r.told && announces(seconds) {
        let remain = if seconds % 60 == 0 {
            let m = seconds / 60;
            `${m} ${c5()}${plural(m, "minute", "minutes")}`
        } else {
            `${seconds} ${c5()}${plural(seconds, "second", "seconds")}`
        };
        tell_game(g, `${c3()}${remain} remaining.`);
    }
    r.told = seconds;
    r
}

// `Slayer_MiniGameSO::onReset`: everyone back in with their lives, then
// the pre-round countdown (bodies stand still, watching themselves) or the
// round at once.
fn start_round(g) {
    let r = round_of(g.id);
    r.reset_at = ();
    r.shown = ();
    r.lives = setting(g.id, "lives");
    for p in players() {
        if p.minigame == g.id {
            set_player(p.id, "out", false);
            set_player(p.id, "lives", lives_for(g, p.team));
        }
    }
    let countdown = setting(g.id, "pre_round_seconds");
    if countdown > 0 {
        r.countdown = tick() + countdown * 120;
        r.counted = ();
        for m in g.members {
            watch(m, m);
        }
    } else {
        r.countdown = ();
        r.clock = tick();
        r.told = ();
    }
    set_round(g.id, r);
    // `serverCmdSlayer(rules)` for everyone, at the game's lives.
    for line in rules(g, ()) {
        tell_game(g, line);
    }
}

// `preRoundCountdownTick`, each second, then `startRound`.
fn count_down(g, r) {
    let left = (r.countdown - tick() + 119) / 120;
    if left <= 0 {
        r.countdown = ();
        r.clock = tick();
        r.told = ();
        for m in g.members {
            if !get_player(m, "out") {
                watch(m, ());
            }
        }
        play_all(g, "{{namespace}}:sound/slayer_begin_sound");
        center_print_minigame(g.id, `<font:Arial Bold:100>${magenta()}GO!`, 2.0);
    } else if r.counted != left {
        r.counted = left;
        // A voice for each of the last ten seconds, as the original's
        // `Slayer_<n>_Seconds_Sound`s.
        if left <= 10 {
            play_all(g, `{{namespace}}:sound/slayer_${left}_seconds_sound`);
        }
        center_print_minigame(g.id, `<font:Arial Bold:100>${magenta()}${left}`, 1.5);
    }
    r
}

// `Slayer_MiniGameSO::play2dAll`: a sound every member hears.
fn play_all(g, sound) {
    for m in g.members {
        play_sound(m, sound);
    }
}

// `Slayer_MiniGameSO::endRound`, for a round any rule ended: who won (and
// how often), everyone out with their camera on the winner or their own
// body, and the reset after the time between rounds.
fn round_ended(g, teams, winners) {
    let r = round_of(g.id);
    let names = [];
    let one = ();
    for id in teams {
        let t = team_by_id(g, id);
        if t != () {
            let wins = get("wins");
            let key = `${g.id}:${id}`;
            wins[key] = if key in wins { wins[key] + 1 } else { 1 };
            set("wins", wins);
            names.push(t.name);
            one = #{ name: t.name, score: team_score(g, id), wins: wins[key], body: () };
        }
    }
    for id in winners {
        let me = player(id);
        if me != () {
            let wins = get_player(id, "wins") + 1;
            set_player(id, "wins", wins);
            names.push(`${yellow()}${me.name}`);
            one = #{ name: `${yellow()}${me.name}`, score: me.score, wins: wins,
                     body: if me.alive { id } else { () } };
        }
    }
    let seconds = setting(g.id, "time_between_rounds");
    let msg = if names.is_empty() {
        `${c5()}Nobody won this round. Resetting in ${seconds} seconds.`
    } else if names.len() > 1 {
        let list = "";
        for n in names {
            if list != "" {
                list += ", ";
            }
            list += n;
        }
        `${magenta()}${list} ${magenta()}tied this round. Resetting in ${seconds} seconds.`
    } else {
        let wins = if one.wins == 1 { "once" } else { `${one.wins} ${c5()}times` };
        if one.score > 0 {
            `${magenta()}${one.name} ${magenta()}won this round with a score of ${c3()}${one.score} ${c5()}${plural(one.score, "point", "points")}${c5()}. ${one.name} ${magenta()}has won ${c3()}${wins}${c5()}. Resetting in ${seconds} seconds.`
        } else {
            `${magenta()}${one.name} ${magenta()}won this round. ${one.name} ${magenta()}has won ${c3()}${wins}${c5()}. Resetting in ${seconds} seconds.`
        }
    };
    let camera = if names.len() == 1 && one.body != () { one.body } else { () };
    let still = !setting(g.id, "allow_move_while_resetting");
    for m in g.members {
        knock_out(m);
        if still {
            let me = player(m);
            if camera != () {
                watch(m, camera);
            } else if me != () && me.alive {
                watch(m, m);
            }
        }
    }
    tell_game(g, msg);
    // A reset waits at least the engine's 5 s between resets.
    r.countdown = ();
    r.reset_at = tick() + seconds * 120;
    r.shown = ();
    set_round(g.id, r);
}

fn on_tick() {
    for g in minigames() {
        let r = round_of(g.id);
        if r.reset_at != () {
            if tick() >= r.reset_at {
                r.reset_at = ();
                set_round(g.id, r);
                reset_minigame(g.id);
                continue;
            }
            // The "Resetting in" countdown at the bottom of the screen.
            let left = (r.reset_at - tick() + 119) / 120;
            if r.shown != left {
                r.shown = left;
                bottom_print_minigame(g.id, `<just:center>${c5()}Resetting in ${left} ${plural(left, "second", "seconds")}.`, 2.0);
                set_round(g.id, r);
            }
            continue;
        }
        if g.round_over {
            continue;
        }
        if r.countdown != () {
            r = count_down(g, r);
            set_round(g.id, r);
            continue;
        }
        check_points(g);
        r = check_time(g, r);
        set_round(g.id, r);
    }
}

// `GameConnection::onDeath`: a life gone, and out of the round at none.
fn on_death(victim, killer) {
    let me = player(victim);
    if me == () || me.minigame == () || !running(me.minigame) {
        return;
    }
    let lives = get_player(victim, "lives");
    if lives <= 0 {
        return;
    }
    lives -= 1;
    set_player(victim, "lives", lives);
    if lives <= 0 {
        knock_out(victim);
        center_print(victim, `${c5()}You have ${c3()}0 ${c5()}lives left.`, 3.0);
        check_lives(minigame(me.minigame));
    }
}

// `GameConnection::createPlayer`: the lives left, and still during the
// countdown.
fn on_spawn(p) {
    let me = player(p);
    if me == () || me.minigame == () {
        return;
    }
    let r = round_of(me.minigame);
    if r.countdown != () {
        watch(p, p);
    }
    let lives = get_player(p, "lives");
    if lives > 0 && r.reset_at == () {
        let msg = `${c5()}You have ${c3()}${lives} ${c5()}${plural(lives, "life", "lives")} left.`;
        center_print(p, msg, 3.0);
        // Everyone spawns at a reset; the rules just told them their lives.
        if lives < lives_for(minigame(me.minigame), me.team) {
            tell(p, msg);
        }
    }
}

// The teams' friendly fire and allied colours live in the engine, which
// keeps teammates from hurting each other.
fn sync_team_rules(g) {
    set_teams(g.id, g.teams, #{
        friendly_fire: setting(g.id, "friendly_fire"),
        ally_same_color: setting(g.id, "ally_same_colors")
    });
}

// `Slayer_MiniGameSO::updateLives`: the difference to every member whose
// team does not set its own; out at none, back in when they have some.
fn lives_changed(g) {
    let r = round_of(g.id);
    let now = setting(g.id, "lives");
    let old = r.lives;
    r.lives = now;
    set_round(g.id, r);
    for p in players() {
        if p.minigame != g.id || (p.team != () && team_setting(g.id, p.team, "team_lives") >= 0) {
            continue;
        }
        let out = get_player(p.id, "out");
        if now <= 0 {
            set_player(p.id, "lives", 0);
            if out {
                revive(p.id);
            }
            continue;
        }
        let lives = get_player(p.id, "lives") + now - old;
        set_player(p.id, "lives", lives);
        if lives <= 0 && !out {
            knock_out(p.id);
            if p.alive {
                watch(p.id, p.id);
                damage(p.id, 10000.0);
            }
            center_print(p.id, `${c5()}The number of lives was reduced. You are now out of lives.`, 5.0);
        } else if lives > 0 && out {
            revive(p.id);
        }
    }
    check_lives(minigame(g.id));
}

// `Slayer_MiniGameSO::setGameMode`: a mode without teams takes everyone
// off theirs; one with teams sorts whoever has none.
fn mode_changed(g) {
    if uses_teams(g.id) {
        if setting(g.id, "auto_sort") {
            let list = [];
            for p in players() {
                if p.minigame == g.id && p.team == () {
                    list.push(p.id);
                }
            }
            auto_sort(g, list);
        }
    } else {
        for p in players() {
            if p.minigame == g.id && p.team != () {
                set_team(p.id, ());
            }
        }
    }
}

fn on_minigame(event) {
    let g = minigame(event.game);
    if g == () {
        if event.kind == "ended" {
            let games = get("games");
            games.remove(`${event.game}`);
            set("games", games);
        }
        return;
    }
    if event.kind == "created" || event.kind == "reset" {
        start_round(g);
    } else if event.kind == "round_end" {
        round_ended(g, event.teams, event.players);
    } else if event.kind == "settings" {
        if is_key(event.keys, "friendly_fire") || is_key(event.keys, "ally_same_colors") {
            sync_team_rules(g);
        }
        if is_key(event.keys, "mode") {
            mode_changed(g);
        }
        if is_key(event.keys, "lives") {
            lives_changed(g);
        }
        if is_key(event.keys, "time") {
            let r = round_of(g.id);
            r.clock = tick();
            r.told = ();
            set_round(g.id, r);
        }
    } else if event.kind == "joined" {
        // `Slayer_MiniGameSO::addMember`: the game's lives, then a team.
        set_player(event.player, "lives", setting(g.id, "lives"));
        set_player(event.player, "out", false);
        set_player(event.player, "wins", 0);
        if uses_teams(g.id) && !g.teams.is_empty() && setting(g.id, "auto_sort") {
            auto_sort(g, [event.player]);
        }
    } else if event.kind == "left" {
        // `Slayer_MiniGameSO::removeMember`: maybe the last one standing.
        set_player(event.player, "out", false);
        check_lives(g);
    } else if event.kind == "teams" && uses_teams(g.id) && setting(g.id, "balance_new_teams") {
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
            // `Slayer_TeamSO::onAddMember`: the team's lives, when it sets
            // fewer (or the player still has the game's).
            let lives = team_setting(g.id, team.id, "team_lives");
            let mine = get_player(event.player, "lives");
            if lives >= 0 && (mine > lives || mine == setting(g.id, "lives")) {
                set_player(event.player, "lives", lives);
            }
            tell(event.player, `${c5()}You have joined ${c3()}${team.name}`);
            if setting(g.id, "notify_member_changes") {
                tell_minigame(g.id, `${c3()}${me.name} ${c5()}has joined ${c3()}${team.name}`, event.player);
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
    let locked = setting(g.id, "lock_teams");
    if cmd == "" {
        tell(p, `${c5()}Teams: /teams join <name>, leave, list, count, score, listmembers <name>, balance.`);
        if can_edit(p, g) {
            tell(p, `${c5()}Set up teams in the Mini-Game window's Add-On Settings, or: /teams add <colour> <name>, remove <name>, friendlyfire on|off, autosort on|off, lock on|off.`);
        }
    } else if cmd == "join" {
        if locked && mine != () {
            tell(p, `${c5()}Teams are locked, you cannot change teams.`);
            return;
        }
        if name == "" {
            tell(p, `${c5()}Please enter a team to join. Type ${c3()}/joinTeam Team Name ${c5()}to join a team. Type ${c3()}/listTeams ${c5()}to view a list of teams.`);
            return;
        }
        let team = team_by_name(g, name);
        if team == () {
            tell(p, `${c5()}Couldn't find team:${c3()} ${name}`);
            return;
        }
        if (mine != () && team_setting(g.id, mine.id, "team_lock")) || team_setting(g.id, team.id, "team_lock") {
            tell(p, `${c5()}Team is locked.`);
            return;
        }
        if mine != () && mine.id == team.id {
            tell(p, `${c5()}You're already on ${c3()}${team.name}${c5()}.`);
            return;
        }
        let max = team_setting(g.id, team.id, "team_max_players");
        let counts = counts(g);
        let there = counts[`${team.id}`];
        if max == 0 || (max > 0 && there >= max) {
            tell(p, `${c5()}That team is not accepting any new members.`);
            return;
        }
        // Slayer's regulated join: a player on a team only moves to a
        // smaller one.
        if mine != () && counts[`${mine.id}`] <= there {
            tell(p, `${c5()}You can only move to a team with fewer players.`);
            return;
        }
        join(p, team);
    } else if cmd == "leave" {
        if mine == () {
            tell(p, `${c5()}You aren't on a team.`);
            return;
        }
        if (locked || setting(g.id, "auto_sort")) && !can_edit(p, g) {
            tell(p, `${c5()}You cannot leave your team.`);
            return;
        }
        tell(p, `${c5()}You have left ${c3()}${mine.name}`);
        if setting(g.id, "notify_member_changes") {
            tell_minigame(g.id, `${c3()}${me.name} ${c5()}has left ${c3()}${mine.name}`, p);
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
                tell(p, `${c3()}(${team_score(g, t.id)}) ${t.name}`);
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
        // Teams play in a team mode: the first team turns one on.
        if !uses_teams(g.id) {
            set_setting(g.id, "mode", "{{tdm_class}}");
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
        let key = if cmd == "friendlyfire" {
            "friendly_fire"
        } else if cmd == "autosort" {
            "auto_sort"
        } else {
            "lock_teams"
        };
        set_setting(g.id, key, on);
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

// `serverCmdSlayer(rules)`: the mode and how to win it, with the lives of
// `team` (or the game's).
fn rules(g, team) {
    let count = lives_for(g, team);
    let points = setting(g.id, "points");
    let time = setting(g.id, "time");
    let person = if uses_teams(g.id) { "team" } else { "man" };
    let out = [`${c3()}${mode_name(g.id)}`];
    if count > 0 {
        let lives = `${c3()}${count} ${c5()}${plural(count, "life", "lives")}`;
        out.push(`${c5()} + ${lives} - The last ${person} standing wins.`);
    }
    if points > 0 {
        let pts = `${c3()}${points} ${c5()}${plural(points, "point", "points")}`;
        out.push(`${c5()} + ${pts} - The first ${person} to reach ${pts} wins.`);
    }
    if time > 0 {
        let t = `${c3()}${time} ${c5()}${plural(time, "minute", "minutes")}`;
        out.push(`${c5()} + ${t} - The ${person} with the highest score after ${t} wins.`);
    }
    let rule = setting(g.id, "custom_rule");
    if rule != "" {
        out.push(`${c5()} + ${rule}`);
    }
    out
}

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
        for line in rules(g, me.team) {
            tell(p, line);
        }
    } else {
        tell(p, `${c5()}This server is running Slayer, by Greek2me (Blockland ID 11902).`);
    }
}
