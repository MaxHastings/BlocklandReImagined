// Stress Lab Creeper: walks to the nearest living player within sight,
// hisses for a moment when close, then explodes, hurting players and
// blowing a hole in the ground. It gives up the fuse if the player escapes.

fn kind() { "stresslab-creeper:entity/creeper" }

// Tuning, in world units and think steps (a think runs every 6 ticks, 20 Hz).
fn sight() { 28.0 }
fn fuse_range() { 3.2 }
fn escape_range() { 6.0 }
fn fuse_steps() { 15 }

fn cmd_spawn(player) {
    let p = player(player);
    spawn_entity(kind(), p.x + 8.0, p.y + 2.0, p.z);
    tell(player, "A creeper appears nearby.");
}

fn cmd_spawn_here(player) {
    let hit = aim();
    if hit == () {
        tell(player, "Aim at the ground to place a creeper.");
        return;
    }
    spawn_entity(kind(), hit.x, hit.y + 1.5, hit.z);
}

// Natural spawning: every 10 seconds, one creeper near a random player,
// while fewer than four are out.
fn on_tick() {
    if get("natural_spawning") != true { return; }
    let alive = 0;
    for e in entities() {
        if e.kind == kind() { alive += 1; }
    }
    let players = players();
    if alive >= 4 || players.len() == 0 { return; }
    let t = tick();
    let p = players[t / 1200 % players.len()];
    if !p.alive { return; }
    let angle = hash3(seed(), t, 0, p.id) * 6.2831;
    spawn_entity(kind(), p.x + angle.cos() * 18.0, p.y + 6.0, p.z + angle.sin() * 18.0);
}

fn nearest(me) {
    let best = ();
    let best_d = sight();
    for p in players() {
        if !p.alive { continue; }
        let d = ((p.x - me.x) ** 2 + (p.y - me.y) ** 2 + (p.z - me.z) ** 2).sqrt();
        if d < best_d {
            best = p;
            best_d = d;
        }
    }
    if best == () { return (); }
    best.distance = best_d;
    best
}

fn think(me) {
    let target = nearest(me);
    let fuse = entity_get(me.id, "fuse");
    if fuse == () { fuse = 0; }
    if target == () {
        entity_set(me.id, "fuse", 0);
        steer(me.id, 0.0, 0.0, false);
        label(me.id, "idle");
        return;
    }
    let lit = fuse > 0;
    if target.distance < fuse_range() || (lit && target.distance < escape_range()) {
        fuse += 1;
        steer(me.id, 0.0, 0.0, false);
        if fuse % 2 == 0 { label(me.id, "fuse_a"); } else { label(me.id, "fuse_b"); }
        if fuse >= fuse_steps() {
            explode(me.x, me.y + 1.0, me.z, 5.5, 70.0, 3.2);
            set("explosions", get("explosions") + 1);
            remove_entity(me.id);
            return;
        }
    } else {
        fuse = 0;
        label(me.id, "chase");
        // Jump when a step blocks the way.
        let stuck = me.speed < 0.6;
        steer(me.id, target.x - me.x, target.z - me.z, stuck);
    }
    entity_set(me.id, "fuse", fuse);
}
