// The Hookshot by Kaje, Nitramtj, Aloshi, Loz and Polymer, ported: the
// behaviour its scripts gave it, running on the player's own imported
// copy. The importer brings the gun, its spearhead and their sounds; this
// is `hookshotProjectile::onCollision`, `pushPlayerToObj`,
// `pushPlayerToObj2` and `serverCmdDegrapple`.
//
// The spearhead flies where you shoot it. Where it strikes, it pulls you
// there: every `every()` milliseconds your speed is set straight at the
// spot, `fast()` while you are farther than `far()`, `slow()` within
// `near()`, until you are within `stop()` of it. That is the end of the
// pull; you keep whatever speed it last gave you. A spearhead that
// strikes a player or a vehicle pulls you after them as they move.
// `/degrapple` stops it. Seated in a vehicle, a spearhead that strikes a
// player or vehicle pulls the vehicle you ride; one that strikes anything
// else does nothing, as in the original.
//
// Every number comes from the copy's own script, filled in at import.

fn num(x) { if type_of(x) == "f64" { x } else { x.to_float() } }
fn stop() { num({{stop}}) }
fn near() { num({{near}}) }
fn slow() { num({{slow}}) }
fn far() { num({{far}}) }
fn fast() { num({{fast}}) }
// The schedule, from milliseconds to ticks (120 a second).
fn every() { (num({{every}}) * 0.12).round().to_int().max(1) }
// Highest change of speed one push may make (the engine's limit).
fn most() { 200.0 }

fn hook() { "{{namespace}}:projectile/hookshotprojectile" }

// What moves (`mover`, a ref) and where to: a ref to follow, or a spot.
fn start(p, mover, target, x, y, z) {
    set_player(p, "pull", [mover, target, x, y, z, tick()]);
}

fn stop_pull(p) {
    set_player(p, "pull", []);
}

fn on_projectile_hit(hit) {
    if hit.projectile != hook() || hit.by == () {
        return;
    }
    let p = hit.by;
    let me = player(p);
    if me == () || !me.alive {
        return;
    }
    let moving = hit.kind == "player" || hit.kind == "vehicle";
    let target = if moving { hit.ref } else { "" };
    if me.mounted {
        // `%player.getObjectMount()`: the vehicle they ride, pulled only
        // toward a player or vehicle (the original's other branch names a
        // variable that does not exist, so it pulls nothing).
        let ride = me.riding;
        if ride != () && moving {
            start(p, "vehicle:" + ride, target, hit.x, hit.y, hit.z);
        }
        return;
    }
    start(p, "player:" + p, target, hit.x, hit.y, hit.z);
}

fn cmd_degrapple(p) {
    let pull = get_player(p, "pull");
    let me = player(p);
    // `cancel(%client.player.pptoTick)`: a vehicle's pull is the vehicle's.
    if pull.len() == 6 && me != () && pull[0] == "player:" + p {
        stop_pull(p);
    }
}

// One push: speed set straight at the target, or the end of the pull.
fn pulse(p, pull) {
    let mover = object(pull[0]);
    let living = player(p);
    if mover == () || living == () || !living.alive {
        return false;
    }
    let to = [pull[2], pull[3], pull[4]];
    if pull[1] != "" {
        let t = object(pull[1]);
        if t == () {
            return false;
        }
        to = [t.x, t.y, t.z];
    }
    let dx = to[0] - mover.x;
    let dy = to[1] - mover.y;
    let dz = to[2] - mover.z;
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len < stop() {
        return false;
    }
    let speed = if len < near() { slow() } else if len > far() { fast() } else { 1.0 };
    let vx = dx / len * speed - mover.vx;
    let vy = dy / len * speed - mover.vy;
    let vz = dz / len * speed - mover.vz;
    let change = (vx * vx + vy * vy + vz * vz).sqrt();
    if change > most() {
        let k = most() / change;
        vx *= k;
        vy *= k;
        vz *= k;
    }
    push(pull[0], vx, vy, vz);
    true
}

fn on_tick() {
    let now = tick();
    for pl in players() {
        let p = pl.id;
        let pull = get_player(p, "pull");
        if pull.len() != 6 || now < pull[5] {
            continue;
        }
        if pulse(p, pull) {
            pull[5] = now + every();
            set_player(p, "pull", pull);
        } else {
            stop_pull(p);
        }
    }
}
