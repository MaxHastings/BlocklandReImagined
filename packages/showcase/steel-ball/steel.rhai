// Steel Ball: a heavy, perfectly round, polished steel ball. Left click
// with the ball in hand rolls one out in front of you; right click hurls
// it where you look. Each player keeps three at most; a fourth replaces
// their oldest. /clearballs puts yours away.
//
// The ball is a vehicle from steel-ball-kit: the engine rolls it as a real
// sphere and pushes players and vehicles aside with its weight. Only in
// minigames does it do harm (`harms_only_in_minigames`): a hard hit punches
// through bricks under the same rules a rocket's hit follows, kills the
// players it runs down and wrecks vehicles. This script only decides when
// balls appear.
//
// Outside minigames every player gets the ball item whenever their items
// are set; inside one, the minigame's loadout decides.

fn mine(p) {
    let ball = get("ball");
    let out = [];
    for o in objects() {
        if o.definition == ball && o.spawner == "steel-ball" && o.owner == p {
            out.push(o);
        }
    }
    // Vehicle ids only grow, so the lowest is the oldest.
    out.sort(|a, b| a.id - b.id);
    out
}

fn release(p, speed, lift, follow_look) {
    let me = player(p);
    if me == () || !me.alive {
        return;
    }
    let balls = mine(p);
    let limit = get("per_player");
    while balls.len() >= limit {
        remove_vehicle(balls[0].ref);
        balls.remove(0);
    }
    // Out in front at chest height, clear of the thrower's body.
    let fx = me.lx;
    let fz = me.lz;
    let flat = sqrt(fx * fx + fz * fz);
    if flat < 0.05 {
        fx = 0.0;
        fz = -1.0;
        flat = 1.0;
    }
    fx /= flat;
    fz /= flat;
    let ahead = 2.4;
    let a = aim();
    if a != () && a.distance < ahead + 1.0 {
        // A wall right in front: drop it at your feet instead.
        ahead = 0.0;
    }
    let x = me.x + fx * ahead;
    let rise = if ahead > 0.0 { 1.4 } else { 3.2 };
    let y = me.y + rise;
    let z = me.z + fz * ahead;
    let vx = fx * speed;
    let vy = lift;
    let vz = fz * speed;
    if follow_look {
        vx = me.lx * speed;
        vy = me.ly * speed + lift;
        vz = me.lz * speed;
    }
    spawn_vehicle(get("ball"), x, y, z, 0.0, [vx, vy, vz], p);
}

fn on_loadout(p) {
    let me = player(p);
    if me != () && me.minigame == () {
        give_item(p, get("tool"), false);
    }
}

fn cmd_steelball(p) {
    let me = player(p);
    if me == () {
        return;
    }
    if me.minigame != () {
        tell(p, "In a minigame, its loadout decides your items.");
        return;
    }
    give_item(p, get("tool"), true);
    tell(p, "Steel Ball: left click rolls one out, right click hurls it. /clearballs puts yours away.");
}

fn cmd_roll(p) {
    release(p, 11.0, 0.0, false);
}

fn cmd_hurl(p) {
    release(p, 24.0, 3.0, true);
}

fn cmd_clearballs(p) {
    for b in mine(p) {
        remove_vehicle(b.ref);
    }
}

// Every two seconds: balls that fell out of the world, or whose owner
// left, are put away.
fn on_tick() {
    let here = [];
    for pl in players() {
        here.push(pl.id);
    }
    let ball = get("ball");
    for o in objects() {
        if o.definition == ball && o.spawner == "steel-ball" {
            if o.y < -200.0 || o.owner == () || !here.contains(o.owner) {
                remove_vehicle(o.ref);
            }
        }
    }
}
