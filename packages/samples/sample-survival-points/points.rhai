// Survival Points, a sample game rule. It runs only on the server: players
// can ask for the leaderboard, but only this script changes the points.
//
// A script may only define functions. The engine calls:
//   on_join(player)  because behaviour.json sets "on_join": true
//   on_tick()        every "tick_interval" ticks (120 ticks = 1 second)
//   cmd_<name>(...)  when a player sends a command listed in behaviour.json

fn on_join(player) {
    tell(player, "Survival Points: stay alive to earn a point every five seconds.");
}

fn on_tick() {
    let each = get("every");
    for p in players() {
        if !p.alive {
            // Dying costs your streak, but your best score stays.
            set_player(p.id, "points", 0);
            continue;
        }
        let now = add_player(p.id, "points", each);
        if now > get_player(p.id, "best") {
            set_player(p.id, "best", now);
        }
        set("awarded", get("awarded") + each);
    }
}

// Anyone may ask; the cooldown in behaviour.json stops chat spam.
fn cmd_top(player) {
    let leader = ();
    let score = -1;
    for p in players() {
        let best = get_player(p.id, "best");
        if best > score {
            leader = p.name;
            score = best;
        }
    }
    if leader == () {
        tell(player, "Nobody has scored yet.");
    } else {
        broadcast(`Survival Points leader: ${leader} with ${score}.`);
    }
}

// Only administrators may send it ("admin": true in behaviour.json).
fn cmd_reset(player) {
    for p in players() {
        set_player(p.id, "points", 0);
        set_player(p.id, "best", 0);
    }
    set("awarded", 0);
    broadcast("Survival Points were reset.");
}
