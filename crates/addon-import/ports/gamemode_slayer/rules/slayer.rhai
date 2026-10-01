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
//   /teams join <name> (a swap request when the team is no smaller),
//   /teams acceptSwap, /teams cancelSwap, /teams leave, /teams list,
//   /teams count, /teams score, /teams living, /teams listMembers <name>,
//   and for editors /teams balance, /teams addMember <player> <team>,
//   /teams removeMember <player> <team>, /addLives <amount> <player>
//   /slayer reset (owner), /slayer rules
// and Slayer's short forms: /joinTeam, /swapTeam, /swapTeams <name>,
// /acceptSwap, /leaveTeam, /listTeams (/teamList), /teamCount, /teamScore,
// /teamLiving. The mini-game's owner or an admin may also type
// /teams add <colour> <name>, /teams remove <name>,
// /teams friendlyfire|autosort|lock on|off.

// Slayer's chat colours (\c3 names, \c5 text): v20's colour codes.
fn c3() { "\uE003" }
fn c5() { "\uE005" }
fn c0() { "\uE000" }
fn c2() { "\uE002" }
fn c7() { "\uE007" }
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
       shown: (), lives: setting(game, "lives"), flying: (), flyers: [], extra: 0,
       custom: (), minutes: setting(game, "time") }
}
fn set_round(game, r) {
    let games = get("games");
    games[`${game}`] = r;
    set("games", games);
}
fn running(game) {
    let r = round_of(game);
    r.reset_at == () && r.countdown == () && r.flying == () && !minigame(game).round_over
}

// `getAdminLvl`: 0 the host, 1 a super admin, 2 an admin, 3 anyone.
fn admin_level(me) {
    if me.host { 0 } else if me.super_admin { 1 } else if me.admin { 2 } else { 3 }
}
// One of Slayer's rights levels (`canEdit`'s switch): 0 the host, 1 super
// admins, 2 admins, 3 the creator (or someone they trust as themselves),
// 4 full trust, 5 build trust with the creator, 6 everyone.
fn has_right(me, g, level) {
    let t = if g.owner == () { 0 } else if g.owner == me.id { 3 } else { trust(me.id, g.owner) };
    switch level {
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
// `canEdit`: the host, or whoever the game's Edit Rights allow.
fn can_edit(p, g) {
    let me = player(p);
    me != () && g != () && (me.host || has_right(me, g, setting(g.id, "edit_rights")))
}
// `canReset`: the host, the creator, or whoever Reset Rights allow.
fn can_reset(p, g) {
    let me = player(p);
    me != () && g != () && (me.host || g.owner == p || has_right(me, g, setting(g.id, "reset_rights")))
}
// `canLeave`: anyone, but from the Default Minigame only the host, the
// creator and whoever Leave Rights allow.
fn can_leave(p, g) {
    let me = player(p);
    me != () && (g == () || !g["default"] || me.host || g.owner == p
        || has_right(me, g, setting(g.id, "leave_rights")))
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
// `Slayer_AiController`s: the bots these rules added
// (`addBotToGame`). They are members as players are, except where Slayer
// counts only connections (`numMembers["GameConnection"]`): sorting,
// balancing and greetings.
fn own_bot(p) { p.bot && p.spawner == "{{rules}}" }
// Every member that plays: players and these rules' bots
// (`%mini.member[%i]`).
fn game_bots(g) {
    let out = [];
    for b in bots() {
        if own_bot(b) && b.minigame == g.id {
            out.push(b);
        }
    }
    out
}
fn everyone() {
    let out = players();
    for b in bots() {
        if own_bot(b) {
            out.push(b);
        }
    }
    out
}
fn members(g, team) {
    let out = [];
    for p in everyone() {
        if p.minigame == g.id && p.team == team {
            out.push(p);
        }
    }
    out
}
// `Slayer_TeamSO::getArtificialScore`: points the team scored itself
// (`IncScore`), apart from its members'.
fn team_points(game, team) {
    let all = get("team_points");
    let key = `${game}:${team}`;
    if key in all { all[key] } else { 0 }
}
// `Slayer_TeamSO::getScore`: its own points and its members' scores
// together.
fn team_score(g, team) {
    let score = team_points(g.id, team);
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

// A spectator team (`spectate`): it can neither hurt nor be hurt, and no
// round is won or reported for it.
fn spectating(game, team) { team != () && team_setting(game, team, "team_spectate") }
// A team's name in its colour (`Slayer_TeamSO::getColoredName`).
fn colored(t) { `<color:${hex(t.color)}>${t.name}` }
// How many of `list` are still in the round (`Slayer_TeamSO::getLiving`).
fn living(list) {
    let n = 0;
    for m in list {
        if !get_player(m.id, "out") {
            n += 1;
        }
    }
    n
}
// `findClientByName`: the first player whose name holds `name`.
fn find_player(name) {
    let wanted = name.to_lower();
    if wanted == "" {
        return ();
    }
    for p in players() {
        if p.name.to_lower() == wanted {
            return p;
        }
    }
    for p in players() {
        if p.name.to_lower().contains(wanted) {
            return p;
        }
    }
    ()
}

// `Slayer_TeamSO::addMember`: onto `team` with a reason for the greeting
// ("Team Swap", "Team Balance", "Team Shuffle"); `quiet` keeps the game's
// players from hearing of it (a shuffle). A living body respawns unless
// `stay`.
fn add_member(p, team, reason, quiet, stay) {
    set_player(p, "reason", reason);
    set_player(p, "quiet_join", quiet);
    set_team(p, team.id);
    let me = player(p);
    if !stay && me != () && me.alive {
        respawn(p);
    }
}

// Team swaps (`swapPending`, `swapOffer`, `swapClient`, `swapTime`): a
// player asking to move to a team no smaller than their own waits up to 20
// seconds for one of its members to /acceptSwap. By game and asking team:
// `#{ from, to, client, ends }`.
fn swap_key(game, team) { `${game}:${team}` }
fn swap_from(game, team) {
    let all = get("swaps");
    let key = swap_key(game, team);
    if key in all { all[key] } else { () }
}
fn swap_to(game, team) {
    let all = get("swaps");
    for key in all.keys() {
        if key.starts_with(`${game}:`) && all[key].to == team {
            return all[key];
        }
    }
    ()
}
fn set_swap(game, team, swap) {
    let all = get("swaps");
    let key = swap_key(game, team);
    if swap == () {
        all.remove(key);
    } else {
        all[key] = swap;
    }
    set("swaps", all);
}
// `Slayer_TeamHandlerSG::swapTeamMembers`.
fn swap_members(g, a, b) {
    let pa = player(a);
    let pb = player(b);
    if pa == () || pb == () || pa.team == () || pb.team == () {
        return;
    }
    let ta = team_by_id(g, pa.team);
    let tb = team_by_id(g, pb.team);
    if ta == () || tb == () {
        return;
    }
    let sa = swap_from(g.id, ta.id);
    let sb = swap_from(g.id, tb.id);
    if sa != () && sa.to == tb.id {
        set_swap(g.id, ta.id, ());
    } else if sb != () && sb.to == ta.id {
        set_swap(g.id, tb.id, ());
    }
    let stay = resetting(g.id);
    add_member(b, ta, "Team Swap", false, stay);
    add_member(a, tb, "Team Swap", false, stay);
}
// `serverCmdTeams(cancelSwap)` 20 seconds on.
fn expire_swaps() {
    let all = get("swaps");
    if all.is_empty() {
        return;
    }
    let now = tick();
    let kept = #{};
    for key in all.keys() {
        let s = all[key];
        if now >= s.ends {
            tell(s.client, `${c5()}No one swapped teams.`);
        } else {
            kept[key] = s;
        }
    }
    if kept.len() != all.len() {
        set("swaps", kept);
    }
}
// `slayerTeamCmdJoin`.
fn team_join(p, g, mine, name) {
    let me = player(p);
    if setting(g.id, "lock_teams") && mine != () {
        tell(p, `${c5()}Teams are locked, you cannot change teams.`);
        return;
    }
    if name == "" {
        tell(p, `${c5()}Please enter a team to join. Type ${c3()}/joinTeam Team Name ${c5()} to join a team. Type ${c3()}/listTeams ${c5()}to view a list of teams.`);
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
        tell(p, `${c5()}You're already on ${colored(team)}${c5()}.`);
        return;
    }
    if mine != () && swap_from(g.id, mine.id) != () {
        tell(p, `${c5()}Someone on your team is already trying to swap teams.`);
        return;
    }
    if swap_to(g.id, team.id) != () {
        tell(p, `${c5()}Someone is already trying to swap to that team.`);
        return;
    }
    let max = team_setting(g.id, team.id, "team_max_players");
    if max == 0 {
        tell(p, `${c5()}That team is not accepting any new members.`);
        return;
    }
    let counts = counts(g);
    let there = counts[`${team.id}`];
    if mine == () {
        if max < 0 || there < max {
            add_member(p, team, "", false, false);
        } else {
            tell(p, `${c5()}That team is not accepting any new members.`);
        }
        return;
    }
    let theirs = swap_from(g.id, team.id);
    if theirs != () && theirs.to == mine.id {
        swap_members(g, p, theirs.client);
        return;
    }
    let here = counts[`${mine.id}`];
    if setting(g.id, "swap_mode") == 1 || (here > there && (max < 0 || there < max)) {
        add_member(p, team, "Team Swap", false, resetting(g.id));
        return;
    }
    let asking = `<color:${hex(mine.color)}>${me.name} ${c5()}wants to swap with someone on <color:${hex(team.color)}>${team.name}${c5()}. Type ${c3()}/acceptSwap ${c5()}to swap teams.`;
    for m in members(g, team.id) {
        tell(m.id, asking);
    }
    tell(p, `${c5()}The members of <color:${hex(team.color)}>${team.name} ${c5()}have been asked if they want to swap with you.`);
    set_swap(g.id, mine.id, #{ from: mine.id, to: team.id, client: p, ends: tick() + ms_ticks(20000) });
}
// `slayerTeamCmdAddMember` and `slayerTeamCmdRemoveMember` (an editor's).
fn member_command(p, g, cmd, parts) {
    let who = if parts.len() > 1 { parts[1] } else { "" };
    if who == "" {
        tell(p, `${c5()}Incorrect usage. Please enter a player name, then a team name.`);
        return;
    }
    let victim = find_player(who);
    if victim == () {
        tell(p, `${c5()}Couldn't find player:${c3()} ${who}`);
        return;
    }
    if victim.minigame != g.id {
        tell(p, `${c3()}${victim.name} ${c5()}is not in the${c3()} ${g.title} ${c5()}minigame.`);
        return;
    }
    let name = rest(parts, 2);
    let team = team_by_name(g, name);
    if team == () {
        tell(p, `${c5()}Couldn't find team:${c3()} ${name}`);
        return;
    }
    if cmd == "addmember" {
        // Team balance stays out of it (`teams_balanceTeams = 0`).
        set_player(victim.id, "no_balance", true);
        add_member(victim.id, team, "", false, false);
    } else if victim.team == team.id {
        set_team(victim.id, ());
        respawn(victim.id);
    }
}

// Continuous team balance (`Slayer_TeamSO::balanceTeam`), by game and the
// team giving a member: the team waiting for one. A team that gives one
// hands over a balance candidate when one of them dies.
fn balance_links(game) {
    let all = get("balance");
    let key = `${game}`;
    if key in all { all[key] } else { #{} }
}
fn set_balance_links(game, links) {
    let all = get("balance");
    if links.is_empty() {
        all.remove(`${game}`);
    } else {
        all[`${game}`] = links;
    }
    set("balance", all);
}
// A team's members that are players, earliest on it first.
fn humans_by_time(g, team) {
    let out = [];
    for m in members(g, team) {
        if !own_bot(m) {
            out.push(m);
        }
    }
    out.sort(|a, b| get_player(a.id, "joined_at") - get_player(b.id, "joined_at"));
    out
}
// `getMemberValue`, and the members of least value (`getBalanceCandidate`;
// `by_time` counts how long they have been on the team).
fn member_value(m) { m.score + get_player(m.id, "kills") - get_player(m.id, "deaths") }
fn balance_candidates(g, team, by_time) {
    let list = humans_by_time(g, team);
    let best = [];
    let lowest = 0;
    for i in 0..list.len() {
        let v = member_value(list[i]) + (if by_time { i } else { 0 });
        if best.is_empty() || v < lowest {
            lowest = v;
            best = [list[i].id];
        } else if v == lowest {
            best.push(list[i].id);
        }
    }
    best
}
fn sorted_ratio(g, t, counts) {
    let w = team_setting(g.id, t.id, "team_sort_weight");
    counts[`${t.id}`].to_float() / w.to_float()
}
fn balances(g, t) {
    team_setting(g.id, t.id, "team_sort") && team_setting(g.id, t.id, "team_sort_weight") > 0
}
// `Slayer_TeamSO::balanceTeam(force)`: the team most over its share gives
// `t` a member, now when forced (or when `t` is empty of two teams and
// joins are regulated), else at that team's next death. The giving team,
// or ().
fn balance_team(g, t, force, depth) {
    let links = balance_links(g.id);
    for key in links.keys() {
        if links[key] == t.id {
            links.remove(key);
        }
    }
    set_balance_links(g.id, links);
    if !balances(g, t) || depth > 8 {
        return ();
    }
    let counts = counts(g);
    let n = counts[`${t.id}`];
    let max = team_setting(g.id, t.id, "team_max_players");
    if max >= 0 && n >= max {
        return ();
    }
    let ratio = sorted_ratio(g, t, counts);
    let highest = 0.0;
    let source = ();
    let sum = 0.0;
    let count = 0;
    let i = g.teams.len() - 1;
    while i >= 0 {
        let o = g.teams[i];
        i -= 1;
        if !balances(g, o) || team_setting(g.id, o.id, "team_max_players") == 0 {
            continue;
        }
        let r = sorted_ratio(g, o, counts);
        sum += r;
        count += 1;
        if o.id != t.id && r > highest {
            highest = r;
            source = o;
        }
    }
    if count > 1 && ratio < sum / count.to_float() {
        // No longer a giver: the team it gave to needs another.
        let key = `${t.id}`;
        if key in links {
            let taker = team_by_id(g, links[key]);
            if taker != () {
                balance_team(g, taker, false, depth + 1);
            }
        }
    }
    if source == () || highest - ratio <= 1.0 {
        return ();
    }
    if force || (n < 1 && g.teams.len() == 2 && setting(g.id, "swap_mode") != 1) {
        let picks = balance_candidates(g, source.id, true);
        if !picks.is_empty() {
            let pick = picks[(hash3(seed(), tick(), t.id, picks.len()) * picks.len()).to_int()];
            add_member(pick, t, "Team Balance", false, false);
        }
    } else {
        let links = balance_links(g.id);
        links[`${source.id}`] = t.id;
        set_balance_links(g.id, links);
    }
    source
}
// `Slayer_TeamSO::isTeamBalanced`.
fn team_balanced(g, t) {
    if !balances(g, t) {
        return true;
    }
    let counts = counts(g);
    let max = team_setting(g.id, t.id, "team_max_players");
    if max >= 0 && counts[`${t.id}`] >= max {
        return true;
    }
    let ratio = sorted_ratio(g, t, counts);
    let highest = 0.0;
    for o in g.teams {
        if o.id == t.id || !balances(g, o) || team_setting(g.id, o.id, "team_max_players") == 0 {
            continue;
        }
        let r = sorted_ratio(g, o, counts);
        if r > highest {
            highest = r;
        }
    }
    highest - ratio <= 1.0
}
// `Slayer_TeamHandlerSG::balanceTeams(force)`: forced, players move until
// every team is even; otherwise each short team waits for a death.
fn balance_teams(g, force) {
    let moves = 0;
    let i = g.teams.len() - 1;
    while i >= 0 {
        let t = g.teams[i];
        i -= 1;
        if !balances(g, t) {
            continue;
        }
        while moves < 64 && !team_balanced(minigame(g.id), t) {
            moves += 1;
            if balance_team(minigame(g.id), t, force, 0) == () || !force {
                break;
            }
        }
    }
}
// The `onDeath` half: a candidate of a giving team moves on dying.
fn balance_on_death(g, me) {
    if !setting(g.id, "balance_teams") || me.team == () || own_bot(me) {
        return;
    }
    let links = balance_links(g.id);
    let key = `${me.team}`;
    if !(key in links) {
        return;
    }
    let taker = team_by_id(g, links[key]);
    if taker != () && balance_candidates(g, me.team, false).contains(me.id) {
        add_member(me.id, taker, "Team Balance", false, true);
    }
}

// Team shuffles between rounds (`Slayer_TeamHandlerSG::shuffleTeams`):
// rounds counted by game.
fn shuffle_count(game) {
    let all = get("shuffles");
    let key = `${game}`;
    if key in all { all[key] } else { 0 }
}
fn set_shuffle_count(game, n) {
    let all = get("shuffles");
    all[`${game}`] = n;
    set("shuffles", all);
}
fn shuffles(g, t) {
    balances(g, t) && team_setting(g.id, t.id, "team_max_players") != 0
}
fn shuffle_teams(g, mode) {
    let able = 0;
    let open = 0;
    let counts = counts(g);
    for t in g.teams {
        if shuffles(g, t) {
            able += 1;
        }
        let max = team_setting(g.id, t.id, "team_max_players");
        if (max <= 0 || counts[`${t.id}`] < max) && balances(g, t) {
            open += 1;
        }
    }
    if able < 2 {
        return;
    }
    if mode == 1 && open == 0 {
        mode = 0;
    }
    let humans = [];
    for p in players() {
        if p.minigame == g.id && p.team != () {
            humans.push(p);
        }
    }
    if mode == 0 {
        // Off the shuffling teams, then sorted afresh.
        let counts = counts(g);
        let movers = [];
        for p in humans {
            let t = team_by_id(g, p.team);
            if t != () && shuffles(g, t) {
                counts[`${t.id}`] -= 1;
                movers.push(p);
            }
        }
        for p in movers {
            let team = counts.pick_team(g, p.id);
            if team != () {
                add_member(p.id, team, "Team Shuffle", true, true);
            } else {
                set_team(p.id, ());
            }
        }
    } else {
        // Every player to another team (`pickTeam(client, 1)`).
        let counts = counts(g);
        for p in humans {
            let team = counts.pick_other_team(g, p.id, p.team);
            if team != () {
                counts[`${p.team}`] -= 1;
                add_member(p.id, team, "Team Shuffle", true, true);
            }
        }
    }
    balance_teams(minigame(g.id), false);
}
// `pickTeam(client, skipCurrentTeam)`.
fn pick_other_team(g, p, current) {
    let best = [];
    let least = 999999.0;
    for t in g.teams {
        if t.id == current {
            continue;
        }
        let n = this[`${t.id}`];
        if !sorts_into(g, t, n) {
            continue;
        }
        let r = n.to_float() / team_setting(g.id, t.id, "team_sort_weight").to_float();
        if r < least {
            least = r;
            best = [t];
        } else if r == least {
            best.push(t);
        }
    }
    if best.is_empty() {
        return ();
    }
    let team = best[(hash3(seed(), tick(), p, best.len()) * best.len()).to_int()];
    this[`${team.id}`] += 1;
    team
}

// `serverCmdAddLives`: an editor gives (or takes) lives, -99 to 99.
fn cmd_addlives(p, line) {
    let me = player(p);
    if me == () || me.minigame == () {
        return;
    }
    let g = minigame(me.minigame);
    if !can_edit(p, g) {
        return;
    }
    let parts = words(line);
    let amount = if parts.is_empty() { () } else { number(parts[0]) };
    if amount == () {
        tell(p, `${c5()}Please use the following format: ${c3()}/addLives Amount Player Name`);
        return;
    }
    let target = find_player(rest(parts, 1));
    if target == () || target.minigame != g.id {
        tell(p, `${c5()}Invalid target.`);
        return;
    }
    if setting(g.id, "lives") <= 0 {
        return;
    }
    let amount = if amount < -99.0 { -99.0 } else if amount > 99.0 { 99.0 } else { amount };
    let shown = if amount == amount.floor() { `${amount.to_int()}` } else { `${amount}` };
    let lives = get_player(target.id, "lives") + amount.to_int();
    tell_game(g, `${c3()}${me.name} ${c5()}gave${c3()} ${shown} ${c5()}lives to${c3()} ${target.name}${c5()}.`);
    if lives < 0 {
        lives = 0;
    }
    set_player(target.id, "lives", lives);
    let out = get_player(target.id, "out");
    if lives > 0 && out {
        revive(target.id);
        if !target.alive {
            respawn(target.id);
        }
    }
    if lives <= 0 && !out {
        knock_out(target.id);
        if target.alive {
            remove_body(target.id);
        }
        bottom_print(target.id, `<just:center>${c5()}The number of lives was reduced. You are now out of lives.`, 5.0);
    }
}
// `Slayer_Support::isFloat`: a typed number, or ().
fn number(word) {
    let sign = 1.0;
    let w = word;
    if w.starts_with("-") {
        sign = -1.0;
        w = w.sub_string(1);
    }
    let halves = w.split(".");
    if w == "" || halves.len() > 2 || whole(halves[0]) < 0 && halves[0] != "" {
        return ();
    }
    let n = if halves[0] == "" { 0.0 } else { whole(halves[0]).to_float() };
    if halves.len() == 2 {
        if halves[1] == "" || whole(halves[1]) < 0 {
            return ();
        }
        n += whole(halves[1]).to_float() / (10.0 ** halves[1].len().to_float());
    }
    sign * n
}

// Bots (Slayer's Preferred Player Count, a team's Bots setting): the
// Blockhead Bot Add-On's bots fill a team up to its count and leave as
// players join it (`Slayer_TeamSO::botFillTeam`). Slayer made
// `BlockheadHoleBot`s ("this should be changeable") and needed Bot_Hole and
// Bot_Blockhead; the Blockhead Bot Add-On is ours of both.
fn bot_kind() { "bot.blockhead" }
fn bot_kind_view() {
    for k in bot_kinds() {
        if k.id == bot_kind() {
            return k;
        }
    }
    ()
}
// "Bot " and a random first name (`"Bot" SPC getRandomFirstName()`).
// `getRandomFirstName` draws from Slayer's own first-names.txt
// (Module_Names).
fn bot_name(kind, n) {
    let names = data_lines("first_names").filter(|line| line != "");
    if names.is_empty() {
        return "Bot";
    }
    let i = (hash3(seed(), tick(), n, names.len()) * names.len()).to_int();
    `Bot ${names[i]}`
}
// `Slayer_TeamSO::botFillTeam`: bots go while the team has more members
// than its count, and come while it has fewer. `room` is how many more
// bots the server takes; what is left of it comes back.
fn bot_fill(g, team, room) {
    let limit = team_setting(g.id, team.id, "team_bot_fill");
    let list = members(g, team.id);
    let n = list.len();
    for m in list {
        if n <= limit {
            break;
        }
        if own_bot(m) {
            remove_bot(m.id);
            n -= 1;
            room += 1;
        }
    }
    let kind = bot_kind_view();
    if kind == () {
        return room;
    }
    while n < limit && room > 0 {
        add_bot(g.id, #{ kind: kind.id, team: team.id, name: bot_name(kind, n * 64 + team.id) });
        n += 1;
        room -= 1;
    }
    room
}
// Every team filled, and bots without a team taken away
// (`Slayer_TeamSO::onRemove`'s `deleteAllBots`).
fn fill_bots(g) {
    if !uses_teams(g.id) {
        for b in game_bots(g) {
            remove_bot(b.id);
        }
        return;
    }
    let room = bot_limit() - bots().len();
    for b in game_bots(g) {
        if b.team == () || team_by_id(g, b.team) == () {
            remove_bot(b.id);
            room += 1;
        }
    }
    for t in g.teams {
        room = bot_fill(g, t, room);
    }
}
// `AiPlayer::useRandomTool`: of the slots up to the last tool, each a
// coin toss in turn, the last one sure.
fn hold_random_tool(p, tools) {
    let last = -1;
    for i in 0..tools.len() {
        if tools[i] != () && tools[i] != "" {
            last = i;
        }
    }
    for i in 0..(last + 1) {
        if tools[i] != () && tools[i] != "" && (i == last || hash3(seed(), tick(), p, i) < 0.5) {
            bot_tool(p, i);
            return;
        }
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
    for p in everyone() {
        if p.minigame == g.id && !get_player(p.id, "out") {
            left[`${p.id}`] = p;
        }
    }
    if teams {
        for t in g.teams {
            if spectating(g.id, t.id) {
                continue;
            }
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
            if spectating(g.id, t.id) {
                continue;
            }
            if team_score(g, t.id) >= points {
                end_round(g.id, #{ teams: [t.id] });
                return;
            }
        }
    }
    for p in everyone() {
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
            if spectating(g.id, t.id) {
                continue;
            }
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
    for p in everyone() {
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
// Ticks left in the round: its time, with what events added or took.
fn time_left(g, r) {
    let extra = if "extra" in r { r.extra } else { 0 };
    r.clock + setting(g.id, "time") * 60 * 120 + extra - tick()
}
fn check_time(g, r) {
    let minutes = setting(g.id, "time");
    if minutes <= 0 {
        return r;
    }
    let left = time_left(g, r);
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
    r.extra = 0;
    r.custom = ();
    r.lives = setting(g.id, "lives");
    // `Slayer_TeamSO::onMinigameReset`: teams' own points start again with
    // Clear Scores or a points limit, as members' scores do.
    if setting(g.id, "clear_scores") || setting(g.id, "points") > 0 {
        let points = get("team_points");
        for key in points.keys() {
            if key.starts_with(`${g.id}:`) {
                points.remove(key);
            }
        }
        set("team_points", points);
    }
    for p in everyone() {
        if p.minigame == g.id {
            set_player(p.id, "out", false);
            set_player(p.id, "lives", lives_for(g, p.team));
            if setting(g.id, "clear_stats") {
                set_player(p.id, "kills", 0);
                set_player(p.id, "deaths", 0);
            }
        }
    }
    r.flying = ();
    r.flyers = [];
    set_round(g.id, r);
    for m in g.members {
        spectate_end(m);
        // `Slayer_MiniGameSO::reset`: `Slayer_ForceGUI Slayer_CtrDisplay 0`.
        hide_report(m);
    }
    // `Slayer_Module_FlyThruCam`'s `onReset`: the fly-through first, and the
    // countdown after it unless it plays during the countdown.
    let fly = fly_this_round(g);
    let countdown = setting(g.id, "pre_round_seconds");
    if fly && !(setting(g.id, "fly_during_countdown") && countdown > 0) {
        start_fly_through(g, true);
    } else {
        begin_round(g);
        if fly {
            start_fly_through(g, false);
        }
    }
    // `serverCmdSlayer(rules)` for each member, at their team's lives,
    // unless Slayer's messages are off.
    // Members who read the same rules get them as one line each.
    if !setting(g.id, "disable_messages") {
        let groups = #{};
        for m in g.members {
            let me = player(m);
            if me == () || me.bot {
                continue;
            }
            let key = `${lives_for(g, me.team)}`;
            if key in groups {
                groups[key].players.push(m);
            } else {
                groups[key] = #{ team: me.team, players: [m] };
            }
        }
        for key in groups.keys() {
            let group = groups[key];
            for line in rules(g, group.team) {
                tell(group.players, line);
            }
        }
    }
}

// The pre-round countdown (`preRoundCountdownTick(0)`), or the round at
// once (`startRound`).
fn begin_round(g) {
    let r = round_of(g.id);
    let countdown = setting(g.id, "pre_round_seconds");
    if countdown > 0 {
        r.countdown = tick() + countdown * 120;
        r.counted = ();
        for m in g.members {
            freeze(m);
        }
    } else {
        r.countdown = ();
        r.clock = tick();
        r.told = ();
        // `startRound`'s `resetHoleLoop`: its bots play again.
        for b in game_bots(g) {
            thaw(b.id);
        }
    }
    set_round(g.id, r);
    if countdown <= 0 {
        // `startRound`'s `onMinigameRoundStart`.
        fire_game_input(g.id, "onMinigameRoundStart");
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
                thaw(m);
            }
        }
        // The fly-through's package of `preRoundCountdownTick`: whoever
        // still watches it gets their body back as the round starts.
        for m in r.flyers {
            follow_path(m, ());
        }
        r.flyers = [];
        play_all(g, "{{namespace}}:sound/slayer_begin_sound");
        center_print_minigame(g.id, `<font:Arial Bold:100>${magenta()}GO!`, 2.0);
        // `startRound`'s `onMinigameRoundStart`.
        fire_game_input(g.id, "onMinigameRoundStart");
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

// `changeDatablock(PlayerFrozenArmor)` through the countdown: no moving,
// jumping or using items, the camera behind the body.
fn frozen_body() { "{{namespace}}:archetype/playerfrozenarmor" }
fn freeze(p) {
    let me = player(p);
    // A bot's brain stops instead (`stopHoleLoop`).
    if me != () && own_bot(me) {
        rest_bot(p, true);
        return;
    }
    if me == () || !me.alive || me.archetype == frozen_body() {
        return;
    }
    set_archetype(p, frozen_body());
    // A body for now, not a choice kept across respawns.
    set_archetype(p, "");
}
// `startRound`: the team's player type, or the game's
// (`%t.playerDatablock`, else `%this.playerDatablock`).
fn thaw(p) {
    let me = player(p);
    // `resetHoleLoop`: a bot's brain runs again.
    if me != () && own_bot(me) {
        rest_bot(p, false);
        return;
    }
    if me == () || !me.alive || me.minigame == () {
        return;
    }
    let body = body_for(minigame(me.minigame), me.team);
    if body != "" {
        set_archetype(p, body);
        set_archetype(p, "");
    }
}

// A team's player type and start tools (`playerDatablock`,
// `startEquip0` to `4`): the game's own while the team syncs with the
// mini-game's loadout (Sync w/ Minigame Loadout, which copies them in
// Slayer's window) or names no type.
fn body_for(g, team) {
    if g == () {
        return "";
    }
    if team != () && !team_setting(g.id, team, "team_sync_loadout") {
        let body = team_setting(g.id, team, "team_player_type");
        if body != "" {
            return body;
        }
    }
    g.player_type
}
fn tools_for(g, team) {
    if team == () || team_setting(g.id, team, "team_sync_loadout") {
        return g.loadout;
    }
    let tools = [];
    for i in 0..5 {
        tools.push(team_setting(g.id, team, `team_equip_${i}`));
    }
    tools
}
// `Slayer_TeamSO::updateRespawnTime`: seconds, at least one, or the
// game's own at -1.
fn respawn_for(g, team) {
    let s = if team == () { -1 } else { team_setting(g.id, team, "team_respawn_time") };
    if s < 0 { () } else { max(s * 1000, 1000) }
}

// `GameConnection::createPlayer`: the team's player type, start tools,
// scale and respawn time on every new body.
fn kit(p, g, team) {
    let me = player(p);
    if me != () && own_bot(me) {
        // `Slayer_AiController::onDeath`: back after the game's bot
        // respawn time, whatever its team's (-1 at once).
        let s = setting(g.id, "respawn_time_bot");
        set_respawn_time(p, if s < 0 { 0 } else { s * 1000 });
    } else {
        set_respawn_time(p, respawn_for(g, team));
    }
    if team == () {
        return;
    }
    let kits = get("kits");
    kits[kit_key(g.id, team)] = tools_for(g, team);
    set("kits", kits);
    let scale = team_setting(g.id, team, "team_scale");
    if scale != 1 {
        set_scale(p, scale.to_float());
    }
    if !team_setting(g.id, team, "team_sync_loadout") {
        let body = team_setting(g.id, team, "team_player_type");
        if body != "" {
            set_archetype(p, body);
            set_archetype(p, "");
        }
        set_tools(p, tools_for(g, team));
    }
}

// What each team's start tools were, to tell which a change replaces
// (`updateEquip`'s `%old`).
fn kit_key(game, team) { `${game}:${team}` }
fn note_kits(g) {
    let kits = get("kits");
    for key in kits.keys() {
        if key.starts_with(`${g.id}:`) {
            kits.remove(key);
        }
    }
    for t in g.teams {
        kits[kit_key(g.id, t.id)] = tools_for(g, t.id);
    }
    set("kits", kits);
}
// `GameConnection::updateEquip`: a changed start tool replaces the old
// one in each member's slot, where they still carry it.
fn update_equips(g) {
    let kits = get("kits");
    for t in g.teams {
        let key = kit_key(g.id, t.id);
        let now = tools_for(g, t.id);
        if key in kits {
            let old = kits[key];
            for m in members(g, t.id) {
                if !m.alive {
                    continue;
                }
                let tools = m.tools;
                let changed = false;
                for i in 0..min(5, tools.len()) {
                    let was = if i < old.len() { old[i] } else { "" };
                    if now[i] != was && tools[i] == was {
                        tools[i] = now[i];
                        changed = true;
                    }
                }
                if changed {
                    set_tools(m.id, tools);
                }
            }
        }
    }
    note_kits(g);
}
// `Slayer_TeamSO::updateDatablock`, `updatePlayerScale` and
// `updateRespawnTime`: members' bodies now. A frozen body keeps the
// countdown's and gets the new one at the start.
fn update_bodies(g, keys) {
    let counting = round_of(g.id).countdown != ();
    for t in g.teams {
        for m in members(g, t.id) {
            if !m.alive {
                continue;
            }
            if is_key(keys, "team_respawn_time") && !own_bot(m) {
                set_respawn_time(m.id, respawn_for(g, t.id));
            }
            if is_key(keys, "team_scale") {
                set_scale(m.id, team_setting(g.id, t.id, "team_scale").to_float());
            }
            let body = body_for(g, t.id);
            if (is_key(keys, "team_player_type") || is_key(keys, "team_sync_loadout"))
                && body != "" && !counting && m.archetype != body {
                set_archetype(m.id, body);
                set_archetype(m.id, "");
            }
        }
    }
}

// `GameConnection::applyUniform`: the team's look by its Uniform, over
// the player's own avatar. None leaves it; Shirt Only paints the torso
// and pack the team's colour; Full is a cop hat, plain chest, pants,
// shoes and arms in the team's colour, with a skin colour on the head and
// hands; Custom is the team's Uniform settings, part by part.
fn team_color(team) {
    let pal = palette();
    if team.color < pal.len() { pal[team.color] } else { [1.0, 1.0, 1.0, 1.0] }
}
// A v20 colour, "r g b a" from 0 to 1, or the team's for "TEAMCOLOR".
fn color_of(text, team) {
    if text == "TEAMCOLOR" {
        return team;
    }
    let out = [];
    for w in text.split(" ") {
        if w == "" {
            continue;
        }
        let n = 0.0;
        try {
            n = parse_float(w);
        } catch {
            return ();
        }
        out.push(if n < 0.0 { 0.0 } else if n > 1.0 { 1.0 } else { n });
    }
    if out.len() == 3 || out.len() == 4 { out } else { () }
}
// `$Slayer::Server::Bots::SkinColor[getRandom(SkinColorCount)]`: 0 to 4,
// and 0 names none, so the default; once a body.
fn skin_color(p) {
    let skins = ["{{skin_default}}", "{{skin_1}}", "{{skin_2}}", "{{skin_3}}", "{{skin_4}}"];
    let i = get_player(p, "skin");
    if i < 0 {
        i = (hash3(seed(), tick(), p, skins.len()) * skins.len()).to_int();
        set_player(p, "skin", i);
    }
    color_of(skins[i], ())
}
// `fileBase`: a decal or face file's name without its folder or ending.
fn file_base(path) {
    let name = path;
    while name.contains("/") {
        name = name.sub_string(name.index_of("/") + 1);
    }
    let dot = name.index_of(".");
    if dot >= 0 { name.sub_string(0, dot) } else { name }
}
fn uni(g, team, key) { team_setting(g.id, team.id, `uni_${key}`) }
fn custom_uniform(p, g, team, c) {
    let choices = avatar_choices();
    let parts = #{};
    // Each Uniform part is its place in v20's list for the slot.
    for slot in [["chest", "chest"], ["hat", "hat"], ["hip", "hip"], ["larm", "l_arm"],
                 ["lhand", "l_hand"], ["lleg", "l_leg"], ["pack", "pack"], ["rarm", "r_arm"],
                 ["rhand", "r_hand"], ["rleg", "r_leg"], ["secondpack", "second_pack"]] {
        let i = uni(g, team, slot[1]);
        if slot[0] in choices && i < choices[slot[0]].len() {
            parts[slot[0]] = choices[slot[0]][i];
        }
    }
    // The accent is the hat's: a helmet's visor, or another hat's plumes.
    let hat = if "hat" in parts { parts.hat.to_lower() } else { "" };
    let accents = `accents.${hat}`;
    let i = uni(g, team, "accent");
    if accents in choices && i < choices[accents].len() {
        parts.accent = choices[accents][i];
    } else if i == 0 {
        parts.accent = "none";
    }
    let decal = file_base(uni(g, team, "decal_name"));
    if decal != "" {
        parts.decal = decal;
    }
    let face = file_base(uni(g, team, "face_name"));
    if face != "" && !setting(g.id, "allow_custom_faces") {
        parts.face = face;
    }
    let colors = #{};
    for slot in [["head", "head_color"], ["torso", "torso_color"], ["hat", "hat_color"],
                 ["accent", "accent_color"], ["pack", "pack_color"],
                 ["secondpack", "second_pack_color"], ["hip", "hip_color"],
                 ["larm", "l_arm_color"], ["rarm", "r_arm_color"], ["lhand", "l_hand_color"],
                 ["rhand", "r_hand_color"], ["lleg", "l_leg_color"], ["rleg", "r_leg_color"]] {
        let color = color_of(uni(g, team, slot[1]), c);
        if color != () {
            colors[slot[0]] = color;
        }
    }
    set_avatar_parts(p, parts);
    set_avatar_colors(p, colors);
}
fn undress(p) {
    if get_player(p, "dressed") {
        set_avatar_parts(p, ());
        set_avatar_colors(p, ());
        set_player(p, "dressed", false);
    }
}
fn dress(p) {
    let me = player(p);
    let g = if me == () || me.minigame == () { () } else { minigame(me.minigame) };
    let team = if g == () || me.team == () { () } else { team_by_id(g, me.team) };
    let mode = if team == () { 0 } else { team_setting(g.id, team.id, "team_uniform") };
    // `Slayer_AiController::applyBodyColors`: a bot's head and hands in a
    // random skin colour, whatever its uniform.
    let bot = me != () && own_bot(me);
    if mode == 0 {
        undress(p);
        if bot {
            let skin = skin_color(p);
            set_avatar_colors(p, #{ head: skin, lhand: skin, rhand: skin });
            set_player(p, "dressed", true);
        }
        return;
    }
    set_player(p, "dressed", true);
    let c = team_color(team);
    if mode == 1 {
        set_avatar_parts(p, ());
        if bot {
            let skin = skin_color(p);
            set_avatar_colors(p, #{ torso: c, pack: c, head: skin, lhand: skin, rhand: skin });
        } else {
            set_avatar_colors(p, #{ torso: c, pack: c });
        }
    } else if mode == 2 {
        let skin = skin_color(p);
        set_avatar_parts(p, #{
            hat: "copHat", accent: "none", pack: "none", secondpack: "none", chest: "chest",
            hip: "pants", larm: "lArm", rarm: "rArm", lhand: "lHand", rhand: "rHand",
            lleg: "lShoe", rleg: "rShoe"
        });
        set_avatar_colors(p, #{
            hat: c, torso: c, hip: c, larm: c, rarm: c, lleg: c, rleg: c,
            head: skin, lhand: skin, rhand: skin
        });
    } else {
        custom_uniform(p, g, team, c);
    }
}
fn dress_game(g) {
    for m in g.members {
        dress(m);
    }
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
    // The `Win` output's custom text wins in place of a team or player.
    let custom = if "custom" in r && r.custom != () { r.custom } else { () };
    if custom != () {
        r.custom = ();
        set_round(g.id, r);
    }
    let msg = if custom != () {
        `${yellow()}${custom} ${c5()}won this round. Resetting in ${seconds} seconds.`
    } else if names.is_empty() {
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
        let me = player(m);
        // `endRound`: its bots are out too, and stand still unless players
        // may move while resetting (`stopHoleLoop`).
        if me != () && own_bot(me) {
            if still {
                rest_bot(m, true);
            }
            continue;
        }
        if still {
            if camera != () {
                watch(m, camera);
            } else if me != () && me.alive {
                watch(m, m);
            }
        }
    }
    tell_game(g, msg);
    if setting(g.id, "eorr_enable") {
        send_report(g, teams, winners);
    }
    // A reset waits at least the engine's 5 s between resets.
    r.countdown = ();
    r.reset_at = tick() + seconds * 120;
    r.shown = ();
    set_round(g.id, r);
    // `endRound`'s `onMinigameRoundEnd`.
    fire_game_input(g.id, "onMinigameRoundEnd");
}

// Highest score first, ties in the order given (Slayer's bubble sorts).
fn by_score(list) {
    let out = [];
    for item in list {
        let at = out.len();
        while at > 0 && out[at - 1].score < item.score {
            at -= 1;
        }
        out.insert(at, item);
    }
    out
}
// `Slayer_MiniGameSO::sendScoreListAll`: the End of Round Report, each
// member's with VICTORY or DEFEAT over it. Teams first (with Display Team
// Scores), then players; a team player's Rounds Won is blank. Another
// Add-On may change its columns (`report_column`), as Slayer's modes did
// with `scoreListInit` and `scoreListAdd`.
fn send_report(g, teams, winners) {
    let wins = get("wins");
    let sections = [];
    let team_rows = [];
    if uses_teams(g.id) && !g.teams.is_empty() && setting(g.id, "eorr_display_team_scores") {
        let list = [];
        for t in g.teams {
            if spectating(g.id, t.id) {
                continue;
            }
            let kills = 0;
            let deaths = 0;
            for m in members(g, t.id) {
                kills += get_player(m.id, "kills");
                deaths += get_player(m.id, "deaths");
            }
            let key = `${g.id}:${t.id}`;
            list.push(#{ key: `team:${t.id}`, name: t.name, color: t.color, score: team_score(g, t.id),
                         kills: kills, deaths: deaths, wins: if key in wins { wins[key] } else { 0 } });
        }
        for t in by_score(list) {
            team_rows.push(#{ key: t.key, name: t.name, color: t.color,
                              cells: #{ score: t.score, kills: t.kills, deaths: t.deaths, wins: t.wins } });
        }
        sections.push(#{ title: "{{eorr_teams}}", rows: team_rows });
    }
    let list = [];
    for m in g.members {
        let me = player(m);
        if me != () {
            list.push(me);
        }
    }
    let rows = [];
    for me in by_score(list) {
        if spectating(g.id, me.team) {
            continue;
        }
        let team = if me.team == () { () } else { team_by_id(g, me.team) };
        rows.push(#{ key: `player:${me.id}`, name: me.name, color: if team == () { () } else { team.color },
                     cells: #{ score: me.score, kills: get_player(me.id, "kills"), deaths: get_player(me.id, "deaths"),
                               wins: if team == () { get_player(me.id, "wins") } else { () } } });
    }
    sections.push(#{ title: if team_rows.is_empty() { "" } else { "{{eorr_players}}" }, rows: rows });
    let columns = [
        #{ key: "score", title: "{{eorr_score}}" },
        #{ key: "kills", title: "{{eorr_kills}}" },
        #{ key: "deaths", title: "{{eorr_deaths}}" },
        #{ key: "wins", title: "{{eorr_wins}}" }
    ];
    let victory = setting(g.id, "eorr_display_victory");
    for m in g.members {
        if player(m) == () || player(m).bot {
            continue;
        }
        let banner = ();
        if victory {
            let me = player(m);
            let won = winners.contains(m) || (me.team != () && teams.contains(me.team));
            banner = if spectating(g.id, me.team) { () } else if won { "{{eorr_victory}}" } else { "{{eorr_defeat}}" };
        }
        show_report(m, #{ title: "{{eorr_title}}", banner: banner, columns: columns, sections: sections });
    }
}

// Capture points (`Slayer_CPTriggerData`, `FxDtsBrick::setCPControl`):
// standing on one fills its bar for your team each time its trigger ticks;
// a full bar captures it in your team's colour. Its trigger is the zone in
// behaviour.json; a point's state lives here by brick id.
fn cp_ticks(kind) {
    if kind == "{{namespace}}:brick/brickslyrcpdata" {
        {{cp_ticks}}
    } else if kind == "{{namespace}}:brick/brickslyrlrgcpdata" {
        {{lrg_cp_ticks}}
    } else {
        ()
    }
}
// `Slayer_CPTriggerData.tickPeriodMS`, in ticks.
fn cp_period(g) { (setting(g, "cp_tick_ms") * 120 + 999) / 1000 }
// A point as first seen: held by the colour it was built in (`origColor`).
fn cp_of(b) {
    let cps = get("cps");
    let key = `${b.id}`;
    if key in cps {
        return cps[key];
    }
    #{ game: b.game, orig: b.color, control: b.color, capture: #{}, decay: #{},
       on: #{}, period: 1 }
}
// `isLocked[color]`: the colour may not use the brick (fill a capture
// point's bar, take a flag), kept on the brick for other modes to read
// (`setTeamControlLocked`).
fn locked(id, color) {
    let l = brick_field(id, "locked");
    l != () && l.contains(color)
}
fn set_cp(id, cp) {
    let cps = get("cps");
    cps[`${id}`] = cp;
    set("cps", cps);
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
fn bar(n) {
    let out = "";
    for i in 0..n {
        out += "_";
    }
    out
}
// `getAverageColor` then `getClosestPaintColor`: the paint colour nearest
// the mix of the holder's and the attacker's, alpha within 0.3.
fn mixed(a, b) {
    let pal = palette();
    if a >= pal.len() || b >= pal.len() {
        return a;
    }
    let mix = [];
    for i in 0..4 {
        mix.push((pal[a][i] + pal[b][i]) / 2.0);
    }
    let best = 0;
    let least = 100000.0;
    for i in 0..pal.len() {
        let c = pal[i];
        let d = ((mix[0] - c[0]) ** 2 + (mix[1] - c[1]) ** 2 + (mix[2] - c[2]) ** 2).sqrt();
        if d < least && (mix[3] - c[3]).abs() < 0.3 {
            least = d;
            best = i;
        }
    }
    best
}
// `setCPControl`: the point turns to `color`; a capture (not a reset)
// scores for the capturer and fires onCPCapture and onCPCapture(TeamN)
// for each team of that colour, a reset fires onCPReset.
fn take_control(g, id, cp, color, p, reset) {
    if p != () && !reset {
        add_score(p, setting(g.id, "points_cp"));
    }
    set_brick_color(id, color);
    cp.control = color;
    let kept = #{};
    if `${color}` in cp.capture {
        kept[`${color}`] = cp.capture[`${color}`];
    }
    cp.capture = kept;
    cp.decay = #{};
    if reset {
        fire_brick_input(id, "onCPReset");
    } else {
        fire_brick_input(id, "onCPCapture", p);
        let i = 0;
        for t in g.teams {
            i += 1;
            if t.color == color && i <= 6 {
                fire_brick_input(id, `onCPCapture(Team${i})`, p);
            }
        }
    }
    cp
}
// `Slayer_CPTriggerData::onEnterTrigger`, `onTickTrigger`, `onLeaveTrigger`.
fn on_zone(p, brick_id, event) {
    let b = brick(brick_id);
    if b == () || cp_ticks(b.kind) == () {
        return;
    }
    let max = cp_ticks(b.kind);
    let cp = cp_of(b);
    if event == "leave" {
        cp.on.remove(`${p}`);
        set_cp(b.id, cp);
        return;
    }
    let me = player(p);
    if me == () || me.minigame == () || me.team == () || b.game != me.minigame {
        return;
    }
    let g = minigame(me.minigame);
    let team = team_by_id(g, me.team);
    if team == () {
        return;
    }
    let att = team.color;
    cp.on[`${p}`] = att;
    cp.game = g.id;
    cp.period = cp_period(g.id);
    if event != "tick" || !running(g.id) {
        set_cp(b.id, cp);
        return;
    }
    let def = cp.control;
    let key = `${att}`;
    cp.decay.remove(key);
    let have = if key in cp.capture { cp.capture[key] } else { 0 };
    if att == def {
        bottom_print(p, `<just:center><color:${hex(att)}>${bar(max)}`, 1);
    } else if have < max && locked(b.id, att) {
        bottom_print(p, `<just:center>${c5()}{{cp_locked_text}}`, 1);
    } else if have < max {
        // The first easing back comes a second after the last tick.
        cp.decay[key] = tick() + cp.period + 120;
        have += 1;
        cp.capture[key] = have;
        bottom_print(p, `<just:center><color:${hex(att)}>${bar(have)}<color:${hex(def)}>${bar(max - have)}`, 1);
        if setting(g.id, "cp_transition_colors") {
            set_brick_color(b.id, mixed(def, att));
        }
    } else {
        cp = take_control(g, b.id, cp, att, p, false);
        bottom_print(p, `<just:center><color:${hex(att)}>${bar(have)}`, 1);
    }
    set_cp(b.id, cp);
}
// `Slayer_CPTriggerData::decreaseCapture`: once nobody of a colour stands
// on a point, its bar eases back a step each trigger tick, and the point
// shows its holder's colour again when the bar is empty.
fn ease_cps() {
    let cps = get("cps");
    let changed = false;
    for key in cps.keys() {
        let cp = cps[key];
        for color in cp.decay.keys() {
            if tick() < cp.decay[color] {
                continue;
            }
            changed = true;
            let held = false;
            for c in cp.on.values() {
                if `${c}` == color {
                    held = true;
                }
            }
            if held {
                cp.decay.remove(color);
                continue;
            }
            let n = if color in cp.capture { cp.capture[color] - 1 } else { 0 };
            if n <= 0 {
                cp.capture.remove(color);
                cp.decay.remove(color);
                if brick(parse_int(key)) != () {
                    set_brick_color(parse_int(key), cp.control);
                }
            } else {
                cp.capture[color] = n;
                cp.decay[color] = tick() + cp.period;
            }
        }
        cps[key] = cp;
    }
    if changed {
        set("cps", cps);
    }
}
// `Slayer_MiniGameSO::resetCapturePoints`: a reset or the game's end gives
// each of its points back to the colour it was built in.
fn reset_cps(game) {
    let cps = get("cps");
    let g = minigame(game);
    for key in cps.keys() {
        let cp = cps[key];
        let id = parse_int(key);
        if cp.game != game || brick(id) == () {
            continue;
        }
        cp.on = #{};
        if g == () {
            set_brick_color(id, cp.orig);
            cps.remove(key);
        } else {
            cps[key] = take_control(g, id, cp, cp.orig, (), true);
        }
    }
    set("cps", cps);
}

fn on_tick() {
    auto_start();
    ease_cps();
    expire_swaps();
    step_spectators();
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
    if me == () || me.minigame == () {
        return;
    }
    // The kills and deaths the event outputs and round report count; a
    // punished teamkill counts neither.
    let k = if killer == () { () } else { player(killer) };
    let g = minigame(me.minigame);
    if g != () && is_teamkill(g, me, k) {
        punish_teamkill(g, me, k);
    } else {
        set_player(victim, "deaths", get_player(victim, "deaths") + 1);
    }
    if killer != () && killer != victim && !(g != () && is_teamkill(g, me, k)) {
        set_player(killer, "kills", get_player(killer, "kills") + 1);
        // Killing a bot is worth Kill Bot, not Kill Player, whoever does
        // it (`incScore(%mini.points_killBot)`, and the correction for
        // double bot scoring); the engine gave the killer Kill Player.
        let k = player(killer);
        if own_bot(me) && k != () && k.minigame == me.minigame {
            let g = minigame(me.minigame);
            let extra = setting(g.id, "points_kill_bot") - g.points_kill_player;
            if extra != 0 {
                add_score(killer, extra);
            }
        }
    }
    // `onMinigameDeath` on every brick of the game: the client who died,
    // and the killer as `Player(Killer)` and `Client(Killer)`.
    if killer != () && player(killer) != () {
        fire_game_input(me.minigame, "onMinigameDeath", victim, killer);
    } else {
        fire_game_input(me.minigame, "onMinigameDeath", victim);
    }
    if g != () {
        balance_on_death(g, me);
    }
    if !running(me.minigame) {
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
        // `spectateStartSched`: spectating five seconds on, unless the
        // round ended. Bots have nobody to watch for.
        if me.bot {
        } else if running(me.minigame) {
            set_spec(victim, #{ init_at: tick() + ms_ticks({{spectate_delay}}), died: tick() });
        } else {
            set_spec(victim, #{ died: tick() });
        }
    }
}

// `Slayer_MiniGameSO::canDamage` for a hole bot's hit (`%classA $=
// "AiPlayer"`, the Bot setting under Damage): with it off, bots a spawn
// brick made hurt nobody in the game. These rules' own bots hit as
// players do.
fn on_damage(victim, attacker, amount, info) {
    if attacker == () {
        return ();
    }
    // `Slayer_TeamHandlerSG::canDamage`: a spectator team neither hurts nor
    // is hurt by another member.
    let me = player(victim);
    let them = player(attacker);
    if me != () && them != () && attacker != victim && me.minigame != () && me.minigame == them.minigame
        && (spectating(me.minigame, me.team) || spectating(me.minigame, them.team)) {
        return 0.0;
    }
    let a = player(attacker);
    if a == () || !a.bot || a.spawner != () {
        return ();
    }
    let me = player(victim);
    if me == () || me.minigame == () || me.minigame != a.minigame || setting(me.minigame, "bot_damage") {
        return ();
    }
    0.0
}

// `GameConnection::createPlayer`: the lives left, and still during the
// countdown.
fn on_spawn(p) {
    let me = player(p);
    // A late joiner's body was taken back before this ran: they watch.
    if me == () || me.minigame == () || !me.alive {
        return;
    }
    // `createPlayer`: out of spectate mode, the team's kit and look; a new
    // body starts its kill spree, quick kills and teamkill penalty afresh.
    spectate_end(p);
    set_player(p, "spree", 0);
    set_player(p, "quick", 0);
    set_player(p, "last_kill", -1);
    set_player(p, "ff_extra", 0);
    let g = minigame(me.minigame);
    kit(p, g, me.team);
    set_player(p, "skin", -1);
    dress(p);
    let r = round_of(me.minigame);
    if r.countdown != () {
        freeze(p);
    }
    if own_bot(me) {
        // `Slayer_AiController::onSpawn`: a random tool in hand, earlier
        // slots likelier (`useRandomTool`). Between rounds it stands still
        // unless players may move.
        let tools = if me.team != () && !team_setting(g.id, me.team, "team_sync_loadout") {
            tools_for(g, me.team)
        } else {
            me.tools
        };
        hold_random_tool(p, tools);
        if r.reset_at != () && !setting(g.id, "allow_move_while_resetting") {
            freeze(p);
        }
        return;
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
                remove_body(p.id);
            }
            center_print(p.id, `${c5()}The number of lives was reduced. You are now out of lives.`, 5.0);
        } else if lives > 0 && out {
            revive(p.id);
        }
    }
    check_lives(minigame(g.id));
}

// `Slayer_TeamSO::updateLives`: the team's members take its new lives at
// once (the change, from a number they had); none or the game's at 0 or
// -1. Out at none, back in when they have some.
fn note_team_lives(g) {
    let seen = get("team_lives");
    for t in g.teams {
        seen[`${g.id}:${t.id}`] = team_setting(g.id, t.id, "team_lives");
    }
    set("team_lives", seen);
}
fn team_lives_changed(g, team) {
    let seen = get("team_lives");
    let key = `${g.id}:${team}`;
    let now = team_setting(g.id, team, "team_lives");
    let old = if key in seen { seen[key] } else { -1 };
    if old == now {
        return;
    }
    seen[key] = now;
    set("team_lives", seen);
    let check = false;
    for m in members(g, team) {
        if now == 0 {
            set_player(m.id, "lives", 0);
            continue;
        }
        if now < 0 {
            set_player(m.id, "lives", setting(g.id, "lives"));
            continue;
        }
        let lives = if old < 0 { now } else { get_player(m.id, "lives") + now - old };
        set_player(m.id, "lives", lives);
        let out = get_player(m.id, "out");
        if lives <= 0 && !out {
            knock_out(m.id);
            if m.alive {
                remove_body(m.id);
            }
            center_print(m.id, `${c5()}The number of lives was reduced. You are now out of lives.`, 5.0);
            check = true;
        } else if lives > 0 && out {
            revive(m.id);
            if !m.alive {
                respawn(m.id);
            }
        }
    }
    if check {
        check_lives(minigame(g.id));
    }
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
    // Without teams its bots go (`deleteAllBots`); with them, the teams
    // fill.
    fill_bots(g);
}

// `Slayer_TeamSO::updateBotFillLimit`: the teams filled to their new
// counts, unless there are no bots to fill them with; then the count goes
// back to -1 and whoever runs the game is told.
fn bot_fill_changed(g) {
    if bot_kind_view() == () {
        let refused = false;
        for t in g.teams {
            if team_setting(g.id, t.id, "team_bot_fill") > 0 {
                set_team_setting(g.id, t.id, "team_bot_fill", -1);
                refused = true;
            }
        }
        if refused && g.owner != () {
            message_box(g.owner, "Slayer | Error", "You must enable the Blockhead Bot Add-On to use the Preferred Player Count setting.");
        }
        return;
    }
    fill_bots(g);
}

// `Slayer_TeamHandlerSG::onNewTeam`, for each team the game did not have:
// everyone without a team is sorted, then players move onto it until it
// is balanced.
fn balance_new(g) {
    let known = get("known_teams");
    let key = `${g.id}`;
    let had = if key in known { known[key] } else { () };
    let ids = [];
    for t in g.teams {
        ids.push(t.id);
    }
    known[key] = ids;
    set("known_teams", known);
    if had == () {
        return 0;
    }
    let added = 0;
    for t in g.teams {
        if !had.contains(t.id) {
            added += 1;
        }
    }
    if !uses_teams(g.id) || !setting(g.id, "balance_new_teams") {
        return added;
    }
    for t in g.teams {
        if had.contains(t.id) {
            continue;
        }
        let counts = counts(g);
        for p in players() {
            if p.minigame == g.id && p.team == () {
                let team = counts.pick_team(g, p.id);
                if team != () {
                    add_member(p.id, team, "", false, false);
                }
            }
        }
        let moves = 0;
        while moves < 64 && !team_balanced(minigame(g.id), t) {
            moves += 1;
            if balance_team(minigame(g.id), t, true, 0) == () {
                break;
            }
        }
    }
    added
}

// The game rules Slayer's settings stand for, on the engine's side:
// the mini-game's colour (`updateColor`), whether it is the default game
// (`updateDefaultMinigame`), how far names show (`updateNameDistance`),
// and whether scores carry over a reset (`setScore` while resetting,
// unless Clear Scores or a points limit). Slayer keeps a leaver's event
// objects and vehicles (`removeMember`), and a game using every player's
// bricks claims those of builders in no game (`getMinigameFromBLID`).
fn apply_game_rules(g) {
    set_minigame_color(g.id, setting(g.id, "color"));
    set_default_minigame(g.id, setting(g.id, "default_minigame"));
    set_name_distance(g.id, setting(g.id, "name_distance"));
    set_keep_scores(g.id, !setting(g.id, "clear_scores") && setting(g.id, "points") <= 0);
    set_cleanup_on_leave(g.id, false);
    set_claims_bricks(g.id, true);
}

// `serverCmdSlayer_getPrefs_Start` and `_Tick`: who updated the game, and
// each setting that changed unless it is quiet or the editor asked not to
// tell the players (`notifyPlayersOnChange`, `notifyOnChange`).
fn announce_changes(g, event) {
    if event.by == () || event.changes.is_empty() {
        return;
    }
    let who = player(event.by);
    let name = if who == () { "" } else { who.name };
    tell_game(g, `${c3()}${name} ${c5()}updated the ${c3()}${g.title} ${c5()}minigame.`);
    if event.quiet {
        return;
    }
    for c in event.changes {
        let info = setting_info(c.key);
        if info == () || info.quiet || info.scope == "server" {
            continue;
        }
        let text = if c.team == () { setting_text(g.id, c.key) } else { team_setting_text(g.id, c.team, c.key) };
        let plus = if c.team == () { " + " } else { " + + " };
        tell_game(g, `${c3()}${plus}${c5()}[${c3()}${info.category}${c5()}|${c3()}${info.title}${c5()}] is now ${c3()}${text}`);
    }
}
// `serverCmdSlayer_getPrefs_End`: the teams the editor added.
fn announce_new_teams(g, added, event) {
    if added <= 0 || event.by == () || event.quiet {
        return;
    }
    tell_game(g, `${c3()} + ${c5()}Added${c3()} ${added} ${c5()}new ${plural(added, "team", "teams")}.`);
    tell_game(g, `${c3()} + + ${c5()}Type ${c3()}/listTeams ${c5()}to view a list of teams and ${c3()}/joinTeam [Team Name] ${c5()}to join a team.`);
    if setting(g.id, "balance_new_teams") {
        tell_game(g, `${c3()} + + ${c5()}Teams have been ${c3()}balanced${c5()}.`);
    }
}
// Save Settings on Update (`$Pref::Slayer::Server::SavePrefsOnUpdate`):
// the host's game, kept as the one Auto Start brings back.
fn save_last(g, event) {
    if event.by == () || !server_setting("save_on_update") {
        return;
    }
    let owner = if g.owner == () { () } else { player(g.owner) };
    if (owner == () && g.owner == ()) || (owner != () && owner.host) {
        let snap = minigame_snapshot(g.id);
        if snap != () {
            set_host_data("last", snap);
        }
    }
}
// Auto Start With Server (`onMissionLoaded`): the last saved game, once,
// as the server's own.
fn auto_start() {
    if get("auto_started") {
        return;
    }
    set("auto_started", true);
    if !server_setting("auto_start") || !minigames().is_empty() {
        return;
    }
    let last = host_data("last");
    if last == () {
        return;
    }
    set("auto_restore", true);
    create_minigame((), #{});
}

fn mg_restart(g, event) {
    if event.kind == "created" {
        apply_game_rules(g);
        if get("auto_restore") {
            set("auto_restore", false);
            let last = host_data("last");
            if last != () {
                restore_minigame(g.id, last);
            }
        }
    }
    reset_cps(g.id);
    note_kits(g);
    note_team_lives(g);
    // The teams it starts with are not new teams.
    let known = get("known_teams");
    if !(`${g.id}` in known) {
        known[`${g.id}`] = g.teams.map(|t| t.id);
        set("known_teams", known);
    }
    // `Slayer_TeamHandlerSG::onMinigameReset`: teams shuffled every so
    // many rounds, without respawning anyone twice.
    let shuffling = uses_teams(g.id) && setting(g.id, "shuffle_teams");
    if shuffling && event.kind == "reset" && shuffle_count(g.id) >= setting(g.id, "rounds_between_shuffle") {
        shuffle_teams(g, setting(g.id, "shuffle_mode"));
        set_shuffle_count(g.id, 0);
    }
    start_round(minigame(g.id));
    // `Slayer_TeamSO::onMinigameReset`: each team filled with bots, unless
    // teams shuffle.
    if !shuffling {
        fill_bots(g);
    }
}

fn mg_loaded(g, event) {
    // A build's saved mini-game (`importMinigamePreferences`, its
    // `.pathcam` read with it): the fly-through path came back with
    // the build, untested and its rounds counted afresh, and the game
    // resets, as `serverCmdSlayer_loadConfig` does.
    let cam = flycam_of(g.id);
    if cam != () {
        cam.test = ();
        cam.passed = 0;
        set_flycam(g.id, cam);
    }
    reset_minigame(g.id);
}

fn mg_round_end(g, event) {
    round_ended(g, event.teams, event.players);
    // `Slayer_TeamHandlerSG::onMinigameRoundEnd`: rounds toward a shuffle.
    set_shuffle_count(g.id, if setting(g.id, "shuffle_teams") { shuffle_count(g.id) + 1 } else { 0 });
}

fn mg_kicked(g, event) {
    // `serverCmdRemoveFromMinigame`: the player kicked, and the game,
    // told who did it.
    let by = if event.by == () { () } else { player(event.by) };
    let them = player(event.player);
    if by != () && them != () {
        tell(event.player, `${by.name} kicked you from the minigame`);
        tell_minigame(g.id, `${by.name} kicked ${them.name} from the minigame`);
    }
}

fn mg_rejected(g, event) {
    // `serverCmdRejectMinigameInvite`: the creator hears who said no.
    let them = player(event.player);
    if g.owner != () && them != () {
        let ignoring = if event.ignored { " (ignoring)" } else { "" };
        tell(g.owner, `${them.name} rejected your mini-game invitation${ignoring}`);
    }
}

fn mg_settings(g, event) {
    announce_changes(g, event);
    save_last(g, event);
    if is_key(event.keys, "color") {
        set_minigame_color(g.id, setting(g.id, "color"));
    }
    if is_key(event.keys, "default_minigame") {
        set_default_minigame(g.id, setting(g.id, "default_minigame"));
    }
    if is_key(event.keys, "name_distance") {
        set_name_distance(g.id, setting(g.id, "name_distance"));
    }
    if is_key(event.keys, "clear_scores") || is_key(event.keys, "points") {
        set_keep_scores(g.id, !setting(g.id, "clear_scores") && setting(g.id, "points") <= 0);
    }
    if is_key(event.keys, "friendly_fire") || is_key(event.keys, "ally_same_colors") {
        sync_team_rules(g);
    }
    if is_key(event.keys, "mode") {
        mode_changed(g);
    }
    if is_key(event.keys, "team_bot_fill") {
        bot_fill_changed(g);
    }
    if is_key(event.keys, "lives") {
        lives_changed(g);
    }
    if is_key(event.keys, "cp_tick_ms") {
        // `Slayer_CPTriggerData.tickPeriodMS = %1`: every point's.
        set_zone_period(0, setting(g.id, "cp_tick_ms"));
    }
    let looks = is_key(event.keys, "team_uniform") || is_key(event.keys, "allow_custom_faces");
    for key in event.keys {
        looks = looks || key.starts_with(`{{rules}}:uni_`);
    }
    if looks {
        dress_game(g);
    }
    update_bodies(g, event.keys);
    let equips = is_key(event.keys, "team_sync_loadout");
    for i in 0..5 {
        equips = equips || is_key(event.keys, `team_equip_${i}`);
    }
    if equips {
        update_equips(g);
    }
    if is_key(event.keys, "time") {
        // `Slayer_MiniGameSO::updateTime`: a clock starts from none, and a
        // running one keeps the time it had left.
        let r = round_of(g.id);
        let now = setting(g.id, "time");
        let old = if "minutes" in r { r.minutes } else { now };
        if now > 0 && old <= 0 {
            r.clock = tick();
            r.extra = 0;
            r.told = ();
        } else if now > 0 {
            r.extra += (old - now) * 60 * 120;
        }
        r.minutes = now;
        set_round(g.id, r);
    }
    for c in event.changes {
        if c.key == `{{rules}}:team_lives` && c.team != () {
            team_lives_changed(g, c.team);
        }
    }
}

fn mg_joined(g, event) {
    // `Slayer_MiniGameSO::addMember`: the game's lives, then a team.
    set_player(event.player, "lives", setting(g.id, "lives"));
    set_player(event.player, "out", false);
    set_player(event.player, "wins", 0);
    // A bot comes with its team, and no greeting or `onMinigameJoin`.
    let me = player(event.player);
    if me != () && own_bot(me) {
        set_player(event.player, "kills", 0);
        set_player(event.player, "deaths", 0);
        return;
    }
    // Reset When Empty: the first player back starts a paused game afresh.
    if setting(g.id, "reset_when_empty") && get("paused").contains(g.id) {
        set("paused", get("paused").filter(|id| id != g.id));
        reset_minigame(g.id);
    }
    if !setting(g.id, "disable_messages") {
        for line in rules(g, ()) {
            tell(event.player, line);
        }
        if uses_teams(g.id) && !setting(g.id, "lock_teams") {
            tell(event.player, `${c5()} + Type ${c3()}/joinTeam [Team Name] ${c5()}to join a team. Type ${c3()}/listTeams ${c5()}to view a list of teams.`);
        }
    }
    if uses_teams(g.id) && !g.teams.is_empty() && setting(g.id, "auto_sort") {
        auto_sort(g, [event.player]);
    }
    // Late Join Time: more than one other player alive and the round older
    // than its minutes, and they watch until the next round.
    let late = setting(g.id, "late_join_time");
    let alive = 0;
    for m in g.members {
        if m != event.player && !get_player(m, "out") {
            alive += 1;
        }
    }
    let r = round_of(g.id);
    if alive > 1 && late >= 0 && r.countdown == () && (tick() - r.clock) > late * 60 * 120 {
        knock_out(event.player);
        spectate_init(event.player);
        tell(event.player, `${c5()}You will spawn when the next round starts.`);
        center_print(event.player, `${c5()}You will spawn when the next round starts.`, 6.0);
        if me != () && me.alive {
            remove_body(event.player);
        }
    }
    fire_game_input(g.id, "onMinigameJoin", event.player);
}

fn mg_left(g, event) {
    // `Slayer_MiniGameSO::removeMember`: `onMinigameLeave`, then maybe
    // the last one standing.
    let me = player(event.player);
    if me != () && own_bot(me) {
        check_lives(g);
        return;
    }
    if me != () {
        fire_game_input(g.id, "onMinigameLeave", event.player);
        hide_report(event.player);
    } else {
        fire_game_input(g.id, "onMinigameLeave");
    }
    set_player(event.player, "out", false);
    spectate_end(event.player);
    undress(event.player);
    // Reset When Empty: the last player gone pauses the game, its bots
    // gone from the field, until someone joins.
    let humans = 0;
    for m in g.members {
        let p = player(m);
        if m != event.player && p != () && !own_bot(p) {
            humans += 1;
        }
    }
    if setting(g.id, "reset_when_empty") && humans < 1 {
        for b in game_bots(g) {
            if b.alive {
                remove_body(b.id);
            }
        }
        let paused = get("paused");
        if !paused.contains(g.id) {
            paused.push(g.id);
            set("paused", paused);
        }
        end_round(g.id, #{ teams: [], players: [] });
        return;
    }
    check_lives(g);
    // `Slayer_TeamSO::removeMember`: their team takes a bot back.
    fill_bots(g);
}

// `Slayer_TeamSO::updateName`: at most 13 words, and a name another team
// has gets a random number after it.
fn unique_names(g) {
    let list = [];
    let seen = [];
    let changed = false;
    for t in g.teams {
        // `getWords(%name, 0, 12)`: thirteen, in fact.
        let ws = words(t.name);
        let name = if ws.len() > 13 { rest(ws.extract(0, 13), 0) } else { trim_copy(t.name) };
        if seen.contains(name.to_lower()) {
            name = `${name} ${(hash3(seed(), tick(), t.id, 7) * 1000000.0).to_int()}`;
        }
        seen.push(name.to_lower());
        if name != t.name {
            changed = true;
        }
        let copy = t;
        copy.name = name;
        list.push(copy);
    }
    if changed {
        set_teams(g.id, list, #{ friendly_fire: g.friendly_fire, ally_same_color: g.ally_same_color });
    }
    changed
}
fn trim_copy(text) {
    let t = text;
    t.trim();
    t
}

fn mg_teams(g, event) {
    if unique_names(g) {
        return;
    }
    // `Slayer_TeamSO::updateColor`: members in their team's new colour.
    // (The Mini-Game window's Apply sends its teams before their
    // settings, so the kits noted are still the old ones.)
    dress_game(g);
    note_team_lives(g);
    announce_new_teams(g, balance_new(g), event);
    save_last(g, event);
    // A team gone takes its bots with it; a new one gets its own.
    fill_bots(g);
}

fn mg_left_team(g, event) {
    undress(event.player);
    let me = player(event.player);
    let was = get_player(event.player, "team_was");
    set_player(event.player, "team_was", -1);
    let old = if was < 0 { () } else { team_by_id(g, was) };
    // `Slayer_TeamSO::removeMember`: their swap request goes, and the
    // game's lives come back unless they had more than the team's.
    if old != () {
        let swap = swap_from(g.id, old.id);
        if swap != () && swap.client == event.player {
            set_swap(g.id, old.id, ());
        }
        if get_player(event.player, "lives") >= team_setting(g.id, old.id, "team_lives") {
            set_player(event.player, "lives", setting(g.id, "lives"));
        }
    }
    if me != () && own_bot(me) {
        remove_bot(event.player);
    } else {
        if old != () && setting(g.id, "balance_teams") {
            balance_team(g, old, false, 0);
        }
        fill_bots(g);
    }
}

fn mg_joined_team(g, event) {
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
        // The `joinTeam` output's reason, `(reason)`.
        let reason = get_player(event.player, "reason");
        if reason != "" {
            reason = `${c5()}(${c3()}${reason}${c5()})`;
            set_player(event.player, "reason", "");
        }
        // `addMember` greets only connections, and a connection's
        // joining sends a bot of the team away (`botFillTeam`).
        let bot = own_bot(me);
        let quiet = get_player(event.player, "quiet_join");
        set_player(event.player, "quiet_join", false);
        set_player(event.player, "joined_at", tick());
        let was = get_player(event.player, "team_was");
        set_player(event.player, "team_was", team.id);
        if !bot {
            tell(event.player, `${c5()}You have joined ${colored(team)} ${reason}`);
            if setting(g.id, "notify_member_changes") && !quiet {
                tell_minigame(g.id, `${c3()}${me.name} ${c5()}has joined ${colored(team)} ${reason}`, event.player);
            }
        }
        dress(event.player);
        if !bot {
            // `onRemoveMember` and `onAddMember`: both teams rebalance,
            // unless an editor placed them (`/teams addMember`).
            let placed = get_player(event.player, "no_balance");
            set_player(event.player, "no_balance", false);
            if setting(g.id, "balance_teams") && !placed {
                let old = if was < 0 { () } else { team_by_id(g, was) };
                if old != () {
                    balance_team(g, old, false, 0);
                }
                balance_team(minigame(g.id), team, false, 0);
            }
            fill_bots(g);
        }
    }
}

fn on_minigame(event) {
    let g = minigame(event.game);
    if g == () {
        if event.kind == "ended" {
            reset_cps(event.game);
            for p in players() {
                if p.minigame == () {
                    undress(p.id);
                }
            }
            let kits = get("kits");
            for key in kits.keys() {
                if key.starts_with(`${event.game}:`) {
                    kits.remove(key);
                }
            }
            set("kits", kits);
            let games = get("games");
            games.remove(`${event.game}`);
            set("games", games);
            // `onRemove`: the fly-through camera goes with the game.
            let cams = get("flycams");
            cams.remove(`${event.game}`);
            set("flycams", cams);
        }
        return;
    }
    switch event.kind {
        "created" | "reset" => mg_restart(g, event),
        "loaded" => mg_loaded(g, event),
        "round_end" => mg_round_end(g, event),
        "kicked" => mg_kicked(g, event),
        "rejected" => mg_rejected(g, event),
        "settings" => mg_settings(g, event),
        "joined" => mg_joined(g, event),
        "left" => mg_left(g, event),
        "teams" => mg_teams(g, event),
        "team" => if event.team == () { mg_left_team(g, event) } else { mg_joined_team(g, event) },
    }
}

// `FxDtsBrick::onPlayerTouch` and `onActivate`: a player on a team sets
// off `onPlayerTouch(TeamN)` or `onActivate(TeamN)` too, N their team's
// place in the game's list, for the first six teams.
fn on_brick_input(input, brick, p) {
    let me = player(p);
    if me == () || me.minigame == () || me.team == () {
        return ();
    }
    let g = minigame(me.minigame);
    if g == () {
        return ();
    }
    let i = 0;
    for t in g.teams {
        i += 1;
        if t.id == me.team {
            return if i <= 6 { `${input}(Team${i})` } else { () };
        }
    }
    ()
}

// `$Slayer::Server::Events::RestrictedEvent__`: who may add each output,
// by `canEdit`'s level (-1 anyone who can edit the game, 0 to 2 an admin,
// 3 the game's owner).
fn restricted() {
    #{
        "minigame:bottomprintall": {{restrict_mg_bottom_print_all}},
        "minigame:centerprintall": {{restrict_mg_center_print_all}},
        "minigame:chatmsgall": {{restrict_mg_chat_msg_all}},
        "minigame:inctimeremaining": {{restrict_mg_inc_time_remaining}},
        "minigame:reset": {{restrict_mg_reset}},
        "minigame:respawnall": {{restrict_mg_respawn_all}},
        "minigame:settimeremaining": {{restrict_mg_set_time_remaining}},
        "minigame:win": {{restrict_mg_win}},
        "minigame:startflythrough": {{fly_restricted}},
        "slayer_teamso:bottomprintall": {{restrict_team_bottom_print_all}},
        "slayer_teamso:centerprintall": {{restrict_team_center_print_all}},
        "slayer_teamso:chatmsgall": {{restrict_team_chat_msg_all}},
        "slayer_teamso:respawnall": {{restrict_team_respawn_all}},
        "slayer_teamso:incscore": {{restrict_team_inc_score}},
    }
}
// `serverCmdAddEvent`: with Restrict Output Events on, a player in the
// game adds a restricted output only if `canEdit` at its level allows.
fn on_event_row(p, brick, row) {
    let me = player(p);
    if me == () || me.minigame == () || !setting(me.minigame, "restrict_output_events") {
        return true;
    }
    let g = minigame(me.minigame);
    let key = `${row.class}:${row.output}`.to_lower();
    let levels = restricted();
    if g == () || !(key in levels) {
        return true;
    }
    let level = levels[key];
    let allowed = if level >= 0 && level <= 2 { me.admin } else { can_edit(p, g) };
    if allowed {
        true
    } else {
        `You do not have permission to use the [${row.class}, ${row.output}] event.`
    }
}

// `Slayer_MiniGameSO::pickSpawnPoint`: a team spawn of the game's in the
// team's colour, else a plain spawn brick of the game's; open ones first.
fn on_pick_spawn(p) {
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    // `Slayer_MiniGameSO::pickSpawnPoint`: the map's spawns unless the game
    // uses spawn bricks; a team's own spawns, else plain spawn bricks (of
    // the Spawn Brick Color, when set), else the map's.
    if !g.settings.use_spawn_bricks {
        return "map";
    }
    let team = if me.team == () { () } else { team_by_id(g, me.team) };
    let spawns = [];
    if team != () {
        for b in bricks(team_spawn()) {
            if b.game == g.id && control_of(b) == team.color {
                spawns.push(b);
            }
        }
    }
    if spawns.is_empty() {
        let color = setting(g.id, "spawn_brick_color");
        for b in bricks(plain_spawn()) {
            if b.game == g.id && (color < 0 || b.color == color) {
                spawns.push(b);
            }
        }
    }
    if spawns.is_empty() {
        return "map";
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

// The colour that holds a brick: a capture point's holder, else the
// colour an event gave it (`controlColor`), else its paint
// (`getTeamControl`).
fn control_of(b) {
    if cp_ticks(b.kind) != () {
        return cp_of(b).control;
    }
    let controls = get("controls");
    let key = `${b.id}`;
    if key in controls { controls[key] } else { b.color }
}

// Slayer's wrench outputs (`Slayer::createBrickEvents`). `target` is the
// brick, player or mini-game the row aims at; `info.client` whoever set
// the row off.
fn on_brick_output(output, target, params, info) {
    switch output {
        "setTeamControl" => set_team_control(target, params[0], info.client),
        "setTeamControlLocked" => lock_control(target, params[0], params[1], params[2], info.client),
        "addLives" => set_player(target, "lives", get_player(target, "lives") + params[0]),
        "addDeaths" => set_player(target, "deaths", get_player(target, "deaths") + params[0]),
        "addKills" => set_player(target, "kills", get_player(target, "kills") + params[0]),
        "setLives" => set_player(target, "lives", params[0]),
        "setDeaths" => set_player(target, "deaths", params[0]),
        "setKills" => set_player(target, "kills", params[0]),
        "joinTeam" => join_by_event(target, params[0], params[1], params[2]),
        "incTimeRemaining" => change_time(target, params[0], params[1], true),
        "setTimeRemaining" => change_time(target, params[0], params[1], false),
        "Win" => win_by_event(target, params[0], params[1], info.client),
        "checkTeam" => return check_team(info.brick, params[0], params[1], params[2], info.client),
        "checkTeamCount" => return check_team_count(info.brick, params[0], params[1], params[2], params[3]),
        "StartFlyThrough" => fly_by_event(target, info.owner),
        "BottomPrintAll" | "CenterPrintAll" | "ChatMsgAll" | "RespawnAll" | "IncScore" =>
            team_event(output, target, params, info),
    }
    ()
}
// The teams a row aimed at `Team(Client)` or `Team(Brick)` stands for:
// whoever set it off's team (`%client.slyrTeam`), or every team of the
// colour holding the brick (`getTeamControlList`).
fn event_teams(target, info) {
    if info.target == "Team(Client)" {
        let me = player(target);
        if me == () || me.minigame == () || me.team == () {
            return [];
        }
        let g = minigame(me.minigame);
        let t = if g == () { () } else { team_by_id(g, me.team) };
        return if t == () { [] } else { [#{ g: g, t: t }] };
    }
    let b = brick(target);
    if b == () || b.game == () {
        return [];
    }
    let g = minigame(b.game);
    if g == () {
        return [];
    }
    let color = control_of(b);
    let out = [];
    for t in g.teams {
        if t.color == color {
            out.push(#{ g: g, t: t });
        }
    }
    out
}
// Slayer_TeamSO's outputs, for each team the row stands for, with `%1` in
// a message as the name of whoever set it off.
fn team_event(output, target, params, info) {
    let text = if params.is_empty() || type_of(params[0]) != "string" { "" } else { params[0] };
    if info.client != () {
        let me = player(info.client);
        if me != () {
            text.replace("%1", me.name);
        }
    }
    for one in event_teams(target, info) {
        let g = one.g;
        let t = one.t;
        switch output {
            "BottomPrintAll" => {
                for m in members(g, t.id) { bottom_print(m.id, text, params[1], params[2]); }
            }
            "CenterPrintAll" => {
                for m in members(g, t.id) { center_print(m.id, text, params[1]); }
            }
            "ChatMsgAll" => {
                for m in members(g, t.id) { tell(m.id, text); }
            }
            "RespawnAll" => {
                for m in members(g, t.id) { respawn(m.id); }
            }
            "IncScore" => add_team_points(g, t.id, params[0]),
        }
    }
}
// `Slayer_TeamSO::incScore`: the team's own points
// (`setArtificialScore`), then whether they reach the points to win.
fn add_team_points(g, team, n) {
    let all = get("team_points");
    let key = `${g.id}:${team}`;
    all[key] = team_points(g.id, team) + n;
    set("team_points", all);
    if running(g.id) {
        check_points(minigame(g.id));
    }
}
// `minigameCanUse(client, brick)`: the brick is of the client's game.
fn can_use(p, b) {
    let me = player(p);
    me != () && me.minigame != () && b.game == me.minigame
}
// `FxDtsBrick::setTeamControl`: a team spawn spawns the colour's teams; a
// capture point is captured for it (scoring for whoever set it off).
fn set_team_control(id, color, p) {
    let b = brick(id);
    if b == () || (p != () && !can_use(p, b)) {
        return;
    }
    if b.kind == team_spawn() {
        let controls = get("controls");
        for key in controls.keys() {
            if brick(parse_int(key)) == () {
                controls.remove(key);
            }
        }
        controls[`${id}`] = color;
        set("controls", controls);
    } else if cp_ticks(b.kind) != () {
        let g = minigame(b.game);
        if g != () {
            set_cp(id, take_control(g, id, cp_of(b), color, p, false));
        }
    }
}
// `FxDtsBrick::setTeamControlLocked`: lock or unlock any brick of the
// game for the team that set it off (mode 0), a colour (1) or everyone
// (2). A locked colour's capture point bar empties; Capture the Flag
// reads the locks on its flags and return points.
fn lock_control(id, mode, color, flag, p) {
    let b = brick(id);
    if b == () || (p != () && !can_use(p, b)) {
        return;
    }
    let colors = [];
    if mode == 0 {
        let me = if p == () { () } else { player(p) };
        if me != () && me.minigame != () && me.team != () {
            let team = team_by_id(minigame(me.minigame), me.team);
            if team != () {
                colors.push(team.color);
            }
        }
    } else if mode == 1 {
        colors.push(color);
    } else {
        for c in 0..64 {
            colors.push(c);
        }
    }
    let l = brick_field(id, "locked");
    if l == () {
        l = [];
    }
    let cp = if cp_ticks(b.kind) == () { () } else { cp_of(b) };
    let control = control_of(b);
    for c in colors {
        if l.contains(c) != flag {
            if flag {
                l.push(c);
            } else {
                let rest = [];
                for x in l {
                    if x != c {
                        rest.push(x);
                    }
                }
                l = rest;
            }
            if cp != () && c != control {
                cp.capture.remove(`${c}`);
                cp.decay.remove(`${c}`);
            }
        }
    }
    set_brick_field(id, "locked", if l.is_empty() { () } else { l });
    if cp != () {
        set_cp(id, cp);
    }
}
// `GameConnection::joinTeam`: onto the named team of their game, with an
// optional reason in the join message; `stay` keeps them where they are.
fn join_by_event(p, name, reason, stay) {
    let me = player(p);
    if me == () || me.minigame == () {
        return;
    }
    let g = minigame(me.minigame);
    let team = team_by_name(g, name);
    if team == () || me.team == team.id {
        return;
    }
    set_player(p, "reason", reason);
    if stay {
        set_team(p, team.id);
    } else {
        join(p, team);
    }
}
// `incTimeRemaining` and `setTimeRemaining`: a game with a time limit
// gets `minutes` more (the time left rounded to the half minute first) or
// exactly that many left.
fn change_time(game, minutes, display, add) {
    let g = minigame(game);
    if g == () {
        return;
    }
    if display {
        let unit = plural(minutes, "minute", "minutes");
        let text = if add { "{{time_extended}}" } else { "{{time_set_to}}" };
        tell_game(g, `${c5()}${text}${c3()} ${minutes} ${c5()}${unit}.`);
    }
    if setting(game, "time") <= 0 || !running(game) {
        return;
    }
    let r = round_of(game);
    let left = time_left(g, r);
    let wanted = if add {
        let half = 30 * 120;
        let rest = left % half;
        let rounded = if rest >= half / 2 { left + half - rest } else { left - rest };
        rounded + minutes * 60 * 120
    } else {
        minutes * 60 * 120
    };
    r.extra += wanted - left;
    r.told = ();
    set_round(game, r);
}
// `MiniGameSO::Win`: end the round for whoever set the row off (0), their
// team (1), a named player (2) or team (3), the text (4), or nobody (5).
fn win_by_event(game, mode, text, p) {
    let g = minigame(game);
    if g == () || !running(game) {
        return;
    }
    let me = if p == () { () } else { player(p) };
    if me != () && me.minigame != game {
        return;
    }
    if mode == 0 {
        if me != () {
            end_round(game, #{ players: [p] });
        }
    } else if mode == 1 {
        if me != () && me.team != () {
            end_round(game, #{ teams: [me.team] });
        }
    } else if mode == 2 {
        let wanted = text.to_lower();
        for o in players() {
            if o.minigame == game && o.name.to_lower().contains(wanted) {
                end_round(game, #{ players: [o.id] });
                return;
            }
        }
    } else if mode == 3 {
        let team = team_by_name(g, text);
        if team != () {
            end_round(game, #{ teams: [team.id] });
        }
    } else if mode == 4 {
        let r = round_of(game);
        r.custom = text;
        set_round(game, r);
        end_round(game, #{});
    } else {
        end_round(game, #{});
    }
}
// `checkTeam`'s and `checkTeamCount`'s row range: "first last" runs only
// those rows; anything else runs every row.
fn check_rows(range) {
    let parts = words(range);
    if parts.len() != 2 {
        return ();
    }
    [whole(parts[0]), whole(parts[1])]
}
fn team_check(pass, range) {
    let input = if pass { "onTeamCheckTrue" } else { "onTeamCheckFalse" };
    let rows = check_rows(range);
    if rows == () || rows[0] < 0 || rows[1] < 0 {
        return input;
    }
    #{ input: input, rows: rows }
}
// `fxDTSBrick::checkTeam`: whether whoever set the row off is on (0), not
// on (1) or allied with (2) the named team.
fn check_team(id, kind, name, range, p) {
    if p == () {
        return ();
    }
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    let check = team_by_name(g, name);
    let theirs = if me.team == () { 0 } else { me.team };
    let wanted = if check == () { 0 } else { check.id };
    let pass = if kind == 0 {
        theirs == wanted
    } else if kind == 1 {
        theirs != wanted
    } else if me.team == () || check == () {
        false
    } else {
        let mine = team_by_id(g, me.team);
        mine.id == check.id || (setting(g.id, "ally_same_colors") && mine.color == check.color)
    };
    team_check(pass, range)
}
// `fxDTSBrick::checkTeamCount`: compares how many are on the named team of
// the brick's game with a number (`>=`, `<=`, `==`, `!=`).
fn check_team_count(id, name, op, number, range) {
    let b = brick(id);
    if b == () || b.game == () {
        return ();
    }
    let g = minigame(b.game);
    if g == () {
        return ();
    }
    let team = team_by_name(g, name);
    let n = if team == () { 0 } else { members(g, team.id).len() };
    // Text that is not a number counts as 0, as in TorqueScript.
    let text = number;
    text.trim();
    let check = whole(text);
    let check = if check < 0 { 0 } else { check };
    let pass = if op == 0 {
        n >= check
    } else if op == 1 {
        n <= check
    } else if op == 2 {
        n == check
    } else {
        n != check
    };
    team_check(pass, range)
}
// `StartFlyThrough` (restricted to the mini-game's creator): everyone in
// the game watches its fly-through, and play carries on.
fn fly_by_event(game, owner) {
    let g = minigame(game);
    let who = player(owner);
    if g == () || (g.owner != owner && (who == () || !who.admin)) {
        return;
    }
    let cam = flycam_of(game);
    if cam == () || cam.knots.is_empty() || g.members.is_empty() {
        return;
    }
    start_fly_through(g, false);
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
        team_join(p, g, mine, name);
    } else if cmd == "acceptswap" {
        if mine == () {
            return;
        }
        let offer = swap_to(g.id, mine.id);
        if offer == () {
            tell(p, `${c5()}No one is trying to swap teams right now.`);
            return;
        }
        swap_members(g, p, offer.client);
    } else if cmd == "cancelswap" {
        let swap = if mine == () { () } else { swap_from(g.id, mine.id) };
        if swap == () || swap.client != p {
            return;
        }
        tell(p, `${c5()}No one swapped teams.`);
        set_swap(g.id, mine.id, ());
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
    } else if cmd == "list" || cmd == "count" || cmd == "score" || cmd == "living" {
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
            } else if cmd == "living" {
                tell(p, `${c3()}(${living(list)}) ${t.name}`);
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
        // Players by name, then how many bots (shown from two up, as
        // Slayer's `numMembers["AiController"] > 1`).
        let bots = 0;
        for m in list {
            if own_bot(m) {
                bots += 1;
            } else {
                tell(p, ` ${c5()}+ ${c3()}${m.name}`);
            }
        }
        if bots > 1 {
            tell(p, ` ${c5()}+ ${c3()}${bots} ${c5()}${plural(bots, "bot", "bots")}`);
        }
    } else if !can_edit(p, g) {
        // An editor's commands do nothing for anyone else.
        if !["balance", "addmember", "removemember"].contains(cmd) {
            tell(p, `${c5()}Please enter a valid command.`);
        }
    } else if cmd == "balance" {
        balance_teams(g, true);
        tell_game(g, `${c3()}${me.name} ${c5()}balanced ${c3()}all ${c5()}teams.`);
    } else if cmd == "addmember" || cmd == "removemember" {
        member_command(p, g, cmd, parts);
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
        let most = server_setting("max_teams");
        if g.teams.len() >= most {
            tell(p, `${c5()}This server allows at most ${most} ${plural(most, "team", "teams")}.`);
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
fn cmd_teamliving(p) { cmd_teams(p, "living") }
fn cmd_swapteam(p, name) { cmd_teams(p, `join ${name}`) }
fn cmd_swapteams(p, name) { cmd_teams(p, `join ${name}`) }
fn cmd_acceptswap(p) { cmd_teams(p, "acceptswap") }

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

// The fly-through camera (`Slayer_Module_FlyThruCam`): the mini-game's
// owner or an admin lays a path of knots with /createFlyCam, /setKnot and
// /setJump, tries it with /testFlyCam and removes it with /deleteFlyCam;
// each reset flies every member along it before the round. A game's path
// lives here by game id: its knots, `count` (`numNodes`, which counts the
// point /createFlyCam was typed at though that point is not flown),
// `passed` (`flyCam_roundsPassed`) and the player testing it.
fn fly_max_knots() { {{fly_max_nodes}} }
fn fly_default_speed() { {{fly_default_speed}} }
fn flycam_of(game) {
    let cams = get("flycams");
    let key = `${game}`;
    if key in cams { cams[key] } else { () }
}
fn set_flycam(game, cam) {
    let cams = get("flycams");
    if cam == () {
        cams.remove(`${game}`);
    } else {
        cams[`${game}`] = cam;
    }
    set("flycams", cams);
}
// The knots to fly, as `follow_path` takes them (kept flat here, as state
// nests only so deep).
fn fly_knots(cam) {
    let out = [];
    for k in cam.knots {
        out.push(#{ at: [k.x, k.y, k.z], yaw: k.yaw, pitch: k.pitch, speed: k.speed,
                    type: k.type, path: k.path, jump: k.jump });
    }
    out
}

// A typed number like `7` or `2.5`, or -1 (`atof` of a speed).
fn decimal(word) {
    let halves = word.split(".");
    if halves.len() > 2 {
        return -1.0;
    }
    let whole_part = whole(halves[0]);
    if whole_part < 0 {
        return -1.0;
    }
    let n = whole_part.to_float();
    if halves.len() == 2 && halves[1] != "" {
        let frac = whole(halves[1]);
        if frac < 0 {
            return -1.0;
        }
        n += frac.to_float() / (10.0 ** halves[1].len().to_float());
    }
    n
}

// The caller's mini-game when they may edit it (`canEdit`), else ().
fn editing(p) {
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    if g == () || !can_edit(p, g) { () } else { g }
}

fn cmd_createflycam(p) {
    let g = editing(p);
    if g == () {
        return;
    }
    // `createPathCamera`: a new path, its first point the caller's view.
    let cam = flycam_of(g.id);
    let passed = if cam == () { 0 } else { cam.passed };
    set_flycam(g.id, #{ knots: [], count: 1, passed: passed, test: () });
    bottom_print(p, `<just:center>${c2()}Fly-through camera created. Type ${c3()}/setKnot ? ${c2()}for help adding knots.`, 3.0);
}

fn cmd_deleteflycam(p) {
    let g = editing(p);
    if g == () || flycam_of(g.id) == () {
        return;
    }
    let cam = flycam_of(g.id);
    if cam.test != () {
        follow_path(cam.test, ());
    }
    let r = round_of(g.id);
    for m in r.flyers {
        follow_path(m, ());
    }
    set_flycam(g.id, ());
    bottom_print(p, `<just:center>${c2()}Fly-through camera deleted.`, 3.0);
}

// `serverCmdSetKnot` and `serverCmdSetJump`: a knot where the caller's
// camera is, with a speed (units a second), a type (Normal, Kink or
// Position) and a path to it (Spline or Linear).
fn add_knot(p, line, jump) {
    let g = editing(p);
    if g == () {
        return;
    }
    let cam = flycam_of(g.id);
    if cam == () {
        tell(p, `${c2()}Please create the fly-through camera first by typing ${c3()}/createFlyCam${c2()}.`);
        return;
    }
    let parts = words(line);
    let what = if jump { "/setJump" } else { "/setKnot" };
    if !parts.is_empty() && parts[0] == "?" {
        tell(p, `${c2()}USAGE: ${c3()}${what} [speed | int 1-100] [type | string Normal, Kink, Position] [path | Spline, Linear]`);
        return;
    }
    if cam.count >= fly_max_knots() {
        tell(p, `${c2()}You have reached the limit of ${fly_max_knots()} nodes.`);
        return;
    }
    let speed = fly_default_speed().to_float();
    if parts.len() > 0 {
        let n = decimal(parts[0]);
        if n > 0.0 {
            speed = n;
        }
    }
    let kind = if jump { "kink" } else { "normal" };
    if parts.len() > 1 {
        let t = parts[1].to_lower();
        if t == "kink" {
            kind = "kink";
        } else if t == "position" {
            kind = "position";
        } else if t == "normal" {
            kind = "normal";
        }
    }
    let path = "spline";
    if parts.len() > 2 && parts[2].to_lower() == "linear" {
        path = "linear";
    }
    let view = player(p).camera;
    cam.knots.push(#{ x: view.at[0], y: view.at[1], z: view.at[2], yaw: view.yaw, pitch: view.pitch,
                      speed: speed.max(0.1).min(1000.0), type: kind, path: path, jump: jump });
    cam.count += 1;
    set_flycam(g.id, cam);
    let added = if jump { "Jump" } else { "Knot" };
    bottom_print(p, `<just:center>${c2()}${added} added to camera path. (${cam.count}/${fly_max_knots()})`, 3.0);
}
fn cmd_setknot(p, line) { add_knot(p, line, false) }
fn cmd_setjump(p, line) { add_knot(p, line, true) }

fn cmd_testflycam(p) {
    let g = editing(p);
    if g == () {
        return;
    }
    let cam = flycam_of(g.id);
    if cam == () {
        tell(p, `${c2()}Please create the fly-through camera first. Type ${c3()}/flyCamHelp ${c2()}for more info.`);
        return;
    }
    if cam.knots.is_empty() {
        return;
    }
    cam.test = p;
    set_flycam(g.id, cam);
    follow_path(p, fly_knots(cam));
}

// `onReset`'s count: a fly-through this round when the game plays one at
// the start of rounds and enough rounds have passed since the last.
fn fly_this_round(g) {
    let cam = flycam_of(g.id);
    if !setting(g.id, "fly_play_on_reset") || cam == () || cam.knots.is_empty() || g.members.is_empty() {
        if cam != () {
            cam.passed = 0;
            set_flycam(g.id, cam);
        }
        return false;
    }
    cam.passed += 1;
    // "Rounds Between Fly-Thrus" is a check box in the original, so 0 or 1.
    let between = if setting(g.id, "fly_rounds_between") { 1 } else { 0 };
    let fly = cam.passed > between;
    if fly {
        cam.passed = 0;
    }
    set_flycam(g.id, cam);
    fly
}

// `startFlyThrough`: every member watches the path; with `waits` the round
// starts when it ends.
fn start_fly_through(g, waits) {
    let cam = flycam_of(g.id);
    cam.test = ();
    set_flycam(g.id, cam);
    let r = round_of(g.id);
    r.flyers = [];
    for m in g.members {
        follow_path(m, fly_knots(cam));
        r.flyers.push(m);
    }
    r.flying = if waits { #{} } else { () };
    set_round(g.id, r);
}

// `Slayer_PathCamData::onNode` at the end of the path: the tester gets
// their camera back; otherwise the round starts (when it waited) and every
// member still watching gets their body back. A spectator's auto camera
// moves on to the next player.
fn on_path_node(p, knot) {
    let spec = get_spec(p);
    if spec.mode == "auto" {
        if knot >= 1 {
            spectate_auto(p);
        }
        return;
    }
    let me = player(p);
    if me == () || me.minigame == () {
        return;
    }
    let g = minigame(me.minigame);
    let cam = flycam_of(g.id);
    if cam == () || knot + 1 < cam.knots.len() {
        return;
    }
    if cam.test == p {
        cam.test = ();
        set_flycam(g.id, cam);
        follow_path(p, ());
        return;
    }
    let r = round_of(g.id);
    if !r.flyers.contains(p) {
        return;
    }
    let starts = r.flying != ();
    r.flying = ();
    let flyers = r.flyers;
    r.flyers = [];
    set_round(g.id, r);
    if starts {
        begin_round(g);
    }
    for m in flyers {
        follow_path(m, ());
    }
}

// Spectating (`GameConnection::spectateInit` and the rest): a player out of
// lives watches the game. Fire and jet step through the living players
// (and the capture points), jump or the light key change the mode: orbit
// a target, fly freely, or the auto camera gliding past each player.
fn get_spec(p) {
    let s = get_player(p, "spec");
    if type_of(s) != "map" {
        s = #{};
    }
    if !("mode" in s) {
        s.mode = ();
    }
    s
}
fn set_spec(p, s) { set_player(p, "spec", s); }

fn resetting(game) { round_of(game).reset_at != () }
fn ms_ticks(ms) { (ms * 120 + 999) / 1000 }

// `spectateTargets`: the game's capture points, then its members.
fn spectate_targets(g) {
    let out = [];
    for kind in ["{{namespace}}:brick/brickslyrcpdata", "{{namespace}}:brick/brickslyrlrgcpdata"] {
        for b in bricks(kind) {
            if b.game == g.id {
                out.push(#{ cp: b });
            }
        }
    }
    for m in g.members {
        out.push(#{ player: m });
    }
    out
}

fn spectate_print(p, g, text) {
    if !resetting(g.id) {
        bottom_print(p, text, 0.0);
    }
}
fn keys_line(full) {
    let line = `<just:right>${c5()}[${c3()}Change Mode${c5()}|${c3()}Use Light${c5()}] `;
    if full {
        line += `\n${c5()}[${c3()}Next${c5()}|${c3()}Fire${c5()}] [${c3()}Previous${c5()}|${c3()}Alt-Fire${c5()}] `;
    }
    line
}

// Whether `t` can be watched by `p` (on team `team`): a living player
// (of their own team with team-only dead cam), or a capture point of the
// game (held by their team, with team-only dead cam).
fn watchable(g, team, t, players_only) {
    let team_only = setting(g.id, "team_only_dead_cam");
    if "player" in t {
        let o = player(t.player);
        if o == () || !o.alive {
            return false;
        }
        return !team_only || team == () || team == o.team;
    }
    if players_only || !setting(g.id, "spectate_cps") {
        return false;
    }
    let control = cp_of(t.cp).control;
    let mine = if team == () { () } else { team_by_id(g, team) };
    !team_only || mine == () || mine.color == control
}

// `spectateNextTarget` (step 1) and `spectatePrevTarget` (step -1).
fn spectate_step(p, step) {
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    let spec = get_spec(p);
    let targets = spectate_targets(g);
    let n = targets.len();
    spec.mode = "orbit";
    spec.on = true;
    if n > 0 {
        let start = if "idx" in spec { spec.idx + step } else if step > 0 { 0 } else { n - 1 };
        for k in 0..n {
            let i = ((start + step * k) % n + n) % n;
            let t = targets[i];
            if !watchable(g, me.team, t, false) {
                continue;
            }
            spec.idx = i;
            set_spec(p, spec);
            if "player" in t {
                let o = player(t.player);
                let team = if o.team == () { () } else { team_by_id(g, o.team) };
                let color = if team == () { c3() } else { `<color:${hex(team.color)}>` };
                spectate_print(p, g, `<just:right>${c3()}Spectating: ${color}${o.name} \n${keys_line(true)}`);
                watch(p, t.player);
            } else {
                let b = t.cp;
                let control = cp_of(b).control;
                let name = b.name;
                name.replace("_", " ");
                name.trim();
                if name == "" {
                    name = "Capture Point";
                }
                spectate_print(p, g, `<just:right>${c3()}Spectating: <color:${hex(control)}>${name} \n${keys_line(true)}`);
                orbit_point(p, [b.x, b.y + {{cp_lift}}, b.z], {{cp_orbit}});
            }
            return t;
        }
    }
    set_spec(p, spec);
    spectate_print(p, g, `<just:right>${c3()}No Target \n${keys_line(true)}`);
    ()
}

// `spectateFree`.
fn spectate_free(p) {
    let me = player(p);
    let g = minigame(me.minigame);
    let spec = get_spec(p);
    spec.mode = "free";
    spec.on = true;
    set_spec(p, spec);
    free_camera(p);
    spectate_print(p, g, `<just:right>${c3()}Free Camera \n${keys_line(false)}`);
}

// `spectateAutoCam`: a straight glide over the next living player's
// shoulder, from five units behind (or in front, looking back) to half a
// unit, at a wide angle; the next player when it ends. A glide that a
// player or brick would block is skipped; with nobody to watch it tries
// again each second.
fn spectate_auto(p) {
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    let spec = get_spec(p);
    spec.mode = "auto";
    spec.on = true;
    spec.retry_at = ();
    let targets = spectate_targets(g);
    let n = targets.len();
    if n > 0 {
        let start = if "idx" in spec { spec.idx + 1 } else { 0 };
        for k in 0..n {
            let i = (start + k) % n;
            let t = targets[i];
            if !watchable(g, me.team, t, true) {
                continue;
            }
            let o = player(t.player);
            let fx = o.lx;
            let fz = o.lz;
            let len = (fx * fx + fz * fz).sqrt();
            if len < 0.001 {
                fx = 0.0;
                fz = -1.0;
            } else {
                fx /= len;
                fz /= len;
            }
            let yaw = atan(fx, -fz);
            let high = hash3(seed(), tick(), p, 1) < 0.5;
            let up_start = if high { {{auto_high}} } else { {{auto_low}} };
            let up_end = if high { {{auto_low}} } else { {{auto_high}} };
            let back = if hash3(seed(), tick(), p, 2) < 0.5 { -1.0 } else { 1.0 };
            if back > 0.0 {
                yaw += PI();
            }
            let far = {{auto_far}}.to_float() * back;
            let near = {{auto_near}}.to_float() * back;
            let from = [o.x + fx * far, o.y + up_start, o.z + fz * far];
            let to = [o.x + fx * near, o.y + up_end, o.z + fz * near];
            let dir = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
            let dist = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            let hit = raycast(from, dir, dist, t.player);
            if hit != () && (hit.kind == "player" || hit.kind == "brick") {
                continue;
            }
            spec.idx = i;
            set_spec(p, spec);
            let team = if o.team == () { () } else { team_by_id(g, o.team) };
            let color = if team == () { c3() } else { `<color:${hex(team.color)}>` };
            spectate_print(p, g, `<just:right>${c3()}Spectating: ${color}${o.name} \n${keys_line(false)}`);
            follow_path(p, [
                #{ at: from, yaw: yaw, pitch: 0.0, speed: {{auto_glide}}, type: "normal", path: "linear" },
                #{ at: to, yaw: yaw, pitch: 0.0, speed: {{auto_glide}}, type: "normal", path: "linear" }
            ]);
            set_fov(p, {{auto_fov}}.to_float());
            return t;
        }
    }
    spec.retry_at = tick() + ms_ticks({{auto_retry}});
    set_spec(p, spec);
    spectate_print(p, g, `<just:right>${c3()}Waiting for target... \n${keys_line(false)}`);
    ()
}

// `spectateInit`: orbit the next target, or fly freely with none.
fn spectate_init(p) {
    if spectate_step(p, 1) == () {
        spectate_free(p);
    }
}

// `spectateEnd`: back to the body (or corpse), the auto camera's angle
// undone and the keys line gone.
fn spectate_end(p) {
    let spec = get_spec(p);
    if spec.mode == () && !("init_at" in spec) {
        return;
    }
    let was = spec.mode;
    set_spec(p, #{});
    if was == () {
        return;
    }
    if was == "auto" {
        set_fov(p, ());
    }
    watch(p, ());
    bottom_print(p, "", 0.0);
}

// `spectateChangeMode`.
fn spectate_change_mode(p) {
    let me = player(p);
    let g = minigame(me.minigame);
    let spec = get_spec(p);
    if spec.mode == "free" {
        if setting(g.id, "spectate_auto_cam") {
            spectate_auto(p);
        } else {
            spectate_step(p, 1);
        }
    } else if spec.mode == "auto" {
        set_fov(p, ());
        spec.retry_at = ();
        set_spec(p, spec);
        spectate_step(p, 1);
    } else if !setting(g.id, "team_only_dead_cam") || me.team == () {
        spectate_free(p);
    } else if setting(g.id, "spectate_auto_cam") {
        spectate_auto(p);
    } else {
        center_print(p, `<div:1>\n<div:1>${c0()}No other camera modes available.\n<div:1>`, 5.0);
    }
}

// `Observer::onTrigger` and `serverCmdLight` for a spectator: fire and jet
// step through the targets while orbiting, jump or the light key change
// the mode. Keys in the first second after death do nothing. The auto
// camera, a path camera, hears only the light key.
fn on_observer(p, button) {
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let spec = get_spec(p);
    if "died" in spec && tick() - spec.died < ms_ticks({{death_grace}}) {
        return true;
    }
    let spectator = spec.mode != ();
    if !resetting(me.minigame) && me.alive && !spectator {
        return ();
    }
    if button == "light" {
        if spectator {
            spectate_change_mode(p);
        }
        return true;
    }
    if spec.mode == "auto" {
        return true;
    }
    if button == "fire" && spec.mode != "free" {
        spectate_step(p, 1);
    } else if button == "jet" && spec.mode != "free" {
        spectate_step(p, -1);
    } else if button == "jump" {
        spectate_change_mode(p);
    }
    true
}

// Spectating five seconds after the last life, and the auto camera's retry.
fn step_spectators() {
    for q in players() {
        if q.minigame == () {
            continue;
        }
        let spec = get_spec(q.id);
        if "init_at" in spec && spec.init_at != () && tick() >= spec.init_at {
            spec.init_at = ();
            set_spec(q.id, spec);
            if get_player(q.id, "out") && !q.alive && !resetting(q.minigame) {
                spectate_init(q.id);
            }
        } else if spec.mode == "auto" && "retry_at" in spec && spec.retry_at != () && tick() >= spec.retry_at {
            spectate_auto(q.id);
        }
    }
}

// ---------------------------------------------------------------------
// Chat (`serverCmdMessageSent`, `serverCmdTeamMessageSent`): Team Display
// Mode shows a player's team by colour or tag, a team's Name Color
// replaces its colour, and Allow Dead Talking keeps the dead to the dead.
// The engine keeps the flood and repeat checks.

// `Slayer_TeamSO::getColorHex`, or the team's Name Color when it is six
// hex digits.
fn chat_color(g, team) {
    let own = team_setting(g.id, team.id, "team_chat_color");
    if own.len() == 6 {
        let ok = true;
        for c in own.to_chars() {
            ok = ok && "0123456789abcdefABCDEF".contains(c);
        }
        if ok {
            return `<color:${own}>`;
        }
    }
    `<color:${hex(team.color)}>`
}
// The line's layout by Team Display Mode (%1 prefix, %2 name, %3 suffix,
// %4 text, %5 team colour, %6 team name, %7 the tail before the text).
fn chat_line(mode, pre, name, suf, msg, color, team_name, tail) {
    switch mode {
        1 => `${color}${pre}${c3()}${name}${color}${suf}${tail}: ${msg}`,
        2 => `${c7()}${pre}${color}${name}${c7()}${suf}${tail}: ${msg}`,
        3 => `${c7()}[${color}${team_name}${c7()}] ${pre}${c3()}${name}${c7()}${suf}${tail}: ${msg}`,
        _ => `${c7()}${pre}${c3()}${name}${c7()}${suf}${tail}: ${msg}`
    }
}
// Web addresses become links, as Slayer's chat made them.
fn chat_links(msg) {
    let out = [];
    for w in msg.split(" ") {
        let at = w.index_of("://");
        let lower = w.to_lower();
        if at > 0 && (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("ftp://")) {
            let url = w.sub_string(at + 3);
            if !url.contains(":") && url != "" {
                out.push(`<sPush><a:${url}>${url}</a><sPop>`);
                continue;
            }
        }
        out.push(w);
    }
    out.reduce(|acc, w| if acc == () { w } else { `${acc} ${w}` })
}
// Members of `g` out of lives (`GameConnection::dead`).
fn dead_members(g, teams) {
    let out = [];
    for q in players() {
        if q.minigame == g.id && !q.bot && get_player(q.id, "out")
            && (teams == () || (q.team != () && teams.contains(q.team))) {
            out.push(q.id);
        }
    }
    out
}
fn team_members(g, teams) {
    let out = [];
    for q in players() {
        if q.minigame == g.id && !q.bot && q.team != () && teams.contains(q.team) {
            out.push(q.id);
        }
    }
    out
}
// The teams a team chat line reaches: the team, or every team of its
// colour with Ally Same Colors (`messageAllByColor`).
fn chat_teams(g, team) {
    if !setting(g.id, "ally_same_colors") {
        return [team.id];
    }
    let out = [];
    for t in g.teams {
        if t.color == team.color {
            out.push(t.id);
        }
    }
    out
}
fn on_chat(p, info) {
    let me = player(p);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    if g == () {
        return ();
    }
    let team = if me.team == () || !uses_teams(g.id) { () } else { team_by_id(g, me.team) };
    let dead = get_player(p, "out") && !resetting(g.id);
    let mode = if team == () { 0 } else { setting(g.id, "team_display_mode") };
    let color = if team == () { "" } else { chat_color(g, team) };
    let team_name = if team == () { "" } else { team.name };
    let msg = chat_links(info.plain);
    let dead_mode = setting(g.id, "dead_chat_mode");
    if info.team {
        if team == () {
            return ();
        }
        if !team_setting(g.id, team.id, "team_chat") || !setting(g.id, "enable_team_chat") {
            return `${c5()}Team chat disabled.`;
        }
        let teams = chat_teams(g, team);
        if dead && (dead_mode == 0 || dead_mode == 2) {
            let tail = ` ${c7()}[DEAD]<color:00ffff>`;
            return #{ line: chat_line(mode, info.clan_prefix, info.name, info.clan_suffix, msg, color, team_name, tail),
                      to: dead_members(g, teams) };
        }
        return #{ line: chat_line(mode, info.clan_prefix, info.name, info.clan_suffix, msg, color, team_name, "<color:00ffff>"),
                  to: team_members(g, teams) };
    }
    if dead && (dead_mode == 0 || dead_mode == 3) {
        let tail = ` ${c7()}[DEAD]<color:c0c0c0>`;
        return #{ line: chat_line(mode, info.clan_prefix, info.name, info.clan_suffix, msg, color, team_name, tail),
                  to: dead_members(g, ()) };
    }
    if mode == 0 && msg == info.plain {
        return ();
    }
    #{ line: chat_line(mode, info.clan_prefix, info.name, info.clan_suffix, msg, color, team_name, "<color:ffffff>") }
}

// ---------------------------------------------------------------------
// Kill lines (`isSpecialKill_Slayer_Teams`, `isSpecialKill_Slayer_BK`):
// Death Message Mode hides them or colours names by team, a punished
// teamkill says so, a team's Hide Kills drops the killer, and Bonus Kills
// adds kill sprees and quick kills.

// Bonus Kills' lines from the player's copy (`$Slayer::Server::BK::*`):
// by spree length and by quick kills in a row, the [0] line for any
// other count.
fn bk_spree_lines() {
    #{ "0": {{bk_spree_0|text}}, "100": {{bk_spree_100|text}}, "200": {{bk_spree_200|text}} }
}
fn bk_quick_lines() {
    #{ "0": {{bk_quick_0|text}}, "2": {{bk_quick_2|text}}, "3": {{bk_quick_3|text}},
       "4": {{bk_quick_4|text}}, "5": {{bk_quick_5|text}}, "6": {{bk_quick_6|text}},
       "7": {{bk_quick_7|text}}, "8": {{bk_quick_8|text}}, "9": {{bk_quick_9|text}},
       "10": {{bk_quick_10|text}} }
}
fn bk_line(lines, n, killer, victim) {
    let key = if `${n}` in lines { `${n}` } else { "0" };
    // Rhai's `replace` changes the string in place.
    let line = lines[key];
    line.replace("%1", `${n}`);
    line.replace("%2", killer);
    line.replace("%3", victim);
    line
}
// `isSpecialKill_Slayer_BK`: the killer's spree and quick kills grow (a
// new body starts both afresh), each worth its bonus points.
fn bonus_kills(g, me, k, info) {
    if !setting(g.id, "bk_enable") || k == () || k.id == me.id {
        return "";
    }
    if me.team != () && k.team != () && me.team == k.team {
        return "";
    }
    let killer = if info.killer_name == () { k.name } else { info.killer_name };
    let victim = if info.victim_name == () { me.name } else { info.victim_name };
    let msg = "";
    let spree = get_player(k.id, "spree") + 1;
    set_player(k.id, "spree", spree);
    if spree >= setting(g.id, "bk_spree_start") {
        msg = bk_line(bk_spree_lines(), spree, killer, victim);
        add_score(k.id, setting(g.id, "bk_points_spree"));
    }
    let now = tick();
    let last = get_player(k.id, "last_kill");
    let quick = get_player(k.id, "quick");
    quick = if last < 0 || now - last <= ms_ticks({{bk_quick_ms}}) { quick + 1 } else { 0 };
    set_player(k.id, "quick", quick);
    set_player(k.id, "last_kill", now);
    if quick > 1 {
        let qk = bk_line(bk_quick_lines(), quick, killer, victim);
        msg = if msg == "" { qk } else { `${msg} ${qk}` };
        add_score(k.id, setting(g.id, "bk_points_multi"));
    }
    msg
}
fn on_death_message(victim, killer, info) {
    let me = player(victim);
    if me == () || me.minigame == () {
        return ();
    }
    let g = minigame(me.minigame);
    if g == () {
        return ();
    }
    let mode = setting(g.id, "death_msg_mode");
    if mode == 0 {
        return false;
    }
    let k = if killer == () { () } else { player(killer) };
    let out = #{};
    if uses_teams(g.id) {
        let colored = |team| if mode == 1 { `<color:${hex(team.color)}>` } else { "<color:ff0040>" };
        let mine = if me.team == () { () } else { team_by_id(g, me.team) };
        if mine != () {
            out.victim = `${colored.call(mine)}${info.victim_name}`;
        }
        if k != () && k.id != me.id {
            let theirs = if k.team == () { () } else { team_by_id(g, k.team) };
            if theirs != () {
                out.killer = `${colored.call(theirs)}${info.killer_name}`;
                if mine != () && theirs.id == mine.id && setting(g.id, "punish_ff") {
                    out.suffix = `${c3()}(Teamkill)`;
                } else if team_setting(g.id, theirs.id, "team_hide_kills") {
                    out.hide_killer = true;
                }
            } else {
                out.killer = `<color:ff0040>${info.killer_name}`;
            }
        }
    }
    let bonus = bonus_kills(g, me, k, info);
    if bonus != "" {
        out.suffix = if "suffix" in out { `${out.suffix} ${bonus}` } else { bonus };
    }
    if out.is_empty() { () } else { out }
}

// The punishment for a teamkill with Punish Friendly Fire on
// (`GameConnection::onDeath`): Friendly Fire points instead of Kill
// Player, the victim's Die points back, no kill or death counted, and the
// killer's next respawn later by the penalty (times their teamkills with
// Progressive FF Penalty).
fn punish_teamkill(g, me, k) {
    add_score(k.id, setting(g.id, "points_friendly_fire") - g.points_kill_player);
    add_score(me.id, -g.settings.points_die);
    let count = get_player(k.id, "ff_count") + 1;
    set_player(k.id, "ff_count", count);
    let penalty = setting(g.id, "ff_penalty") * 1000;
    let add = if setting(g.id, "ff_progressive") { count * penalty } else { penalty };
    let base = respawn_for(g, k.team);
    let now = (if base == () { g.settings.respawn_ms } else { base }) + get_player(k.id, "ff_extra") + add;
    set_player(k.id, "ff_extra", get_player(k.id, "ff_extra") + add);
    set_respawn_time(k.id, now);
    if !k.bot && setting(g.id, "ff_warning") {
        let seconds = (now + 999) / 1000;
        center_print(k.id, `<just:center>\n${c2()}You just killed a team-mate!\n${c2()}Be more careful.\n${c2()}Your respawn time is now${c0()} ${seconds} ${c2()}seconds.\n`, 7.0);
    }
}
fn is_teamkill(g, me, k) {
    k != () && k.id != me.id && me.team != () && k.team == me.team && uses_teams(g.id)
        && setting(g.id, "punish_ff")
}

// ---------------------------------------------------------------------
// Who may do what to a game (`canEdit`, `canReset`, `canLeave`,
// `canCreateMinigame` and the `serverCmd*Minigame` overrides).
fn no_permission() { `${c5()}You don't have permission to do that.` }
fn invite_error(text) { #{ title: "Mini-Game Invite Error", text: text } }
fn on_minigame_request(p, action, info) {
    let me = player(p);
    if me == () {
        return ();
    }
    let mine = if me.minigame == () { () } else { minigame(me.minigame) };
    let g = if info.game == () { () } else { minigame(info.game) };
    switch action {
        "create" => {
            if me.host {
                return true;
            }
            if !can_leave(p, mine) {
                return no_permission();
            }
            if admin_level(me) > server_setting("create_rights") {
                return no_permission();
            }
            ()
        }
        "leave" => if can_leave(p, mine) { () } else { no_permission() },
        "join" => {
            if !can_leave(p, mine) {
                return no_permission();
            }
            if g == () {
                return ();
            }
            if mine != () && mine.id == g.id {
                return "You're already in that mini-game.";
            }
            if g.settings.invite_only && me.invite != g.id && !can_edit(p, g) {
                return "That mini-game is invite-only.";
            }
            // Slayer's join skips v20's wait between joins.
            true
        }
        "reset" => {
            if g == () {
                return ();
            }
            if can_reset(p, g) {
                if resetting(g.id) { false } else { true }
            } else {
                no_permission()
            }
        }
        "edit" | "end" | "kick" | "team" | "respawn_all" => {
            if g == () {
                return ();
            }
            if !can_edit(p, g) {
                return no_permission();
            }
            // `$Pref::Server::Slayer::Teams::maxTeams`: `addTeam` makes no
            // more.
            let most = server_setting("max_teams");
            if action == "edit" && info.teams != () && info.teams > most && info.teams > g.teams.len() {
                return `This server allows at most ${most} ${plural(most, "team", "teams")}.`;
            }
            true
        }
        "invite" => {
            if g == () {
                return ();
            }
            if !can_edit(p, g) {
                return invite_error("You do not have permission to send invites.");
            }
            let them = if info.target == () { () } else { player(info.target) };
            if them == () {
                return false;
            }
            if them.invite != () {
                return invite_error(if them.invite == g.id {
                    "This person hasn't responded to your first invite yet."
                } else {
                    "This person is considering another invite right now."
                });
            }
            if them.minigame == g.id {
                return invite_error("This person is already in the mini-game.");
            }
            true
        }
        // Slayer only turned the invitation down (`serverCmdIgnoreMinigameInvite`).
        "ignore" => false,
        _ => ()
    }
}

// The light key and /suicide (`serverCmdLight`, `serverCmdSuicide`), and an
// admin's free camera while out of lives (`serverCmdDropPlayerAtCamera`).
fn allow_light(p) {
    let me = player(p);
    if me == () || me.minigame == () || setting(me.minigame, "enable_lights") {
        return true;
    }
    `${c5()}Lights are disabled in this minigame.`
}
fn allow_suicide(p) {
    let me = player(p);
    if me == () || me.minigame == () || setting(me.minigame, "enable_suicide") {
        return true;
    }
    `${c5()}Suicide is disabled in this minigame.`
}
fn allow_admin_camera(p) {
    let me = player(p);
    if me == () || me.minigame == () || !get_player(p, "out") || can_edit(p, minigame(me.minigame)) {
        return true;
    }
    `${c5()}You're out of lives! Don't cheat!`
}
