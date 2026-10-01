// The Grapple Rope by Uristqwerty, SolarFlare and Demian (original code by
// Qwertyuiopas), ported: the behaviour its scripts gave it, running on the
// player's own imported copy. The importer brings the launcher and its
// hook; this is `GrappleRopeProjectile::onCollision`,
// `GrappleRopeImage::onHold`, `GrappleRopeImage::onRelease` and
// `GrappleRope`.
//
// Hold left click: the hook flies where you aim. Where it strikes
// (anything, as `$Pref::Server::GrappleRopeAnywhere` is on), if nothing
// solid stands between it and a point `lift()` above your feet, you hang
// from that spot on a rope as long as the distance then. You swing on it
// for as long as you hold the click; let go and you fly off with all
// your speed. The rope stays on the spot it struck, even if what it
// struck goes.
//
// `GrappleRope` held the player to the rope by resetting their speed every
// 10 ms; the engine's rope (`tether`) does that as part of the player's
// own movement, so it feels the same on their screen as on the host.

fn num(x) { if type_of(x) == "f64" { x } else { x.to_float() } }
// The line of sight is checked from this high above the feet.
fn lift() { num({{lift}}) }
// Longest rope the engine holds (past the original hook's reach).
fn longest() { 1000.0 }

fn hook() { "{{namespace}}:projectile/grappleropeprojectile" }
fn image() { "{{namespace}}:image/grappleropeimage" }

// Still holding the click: the image's Fire or Hold state.
fn holding(me) {
    me != () && me.alive && !me.mounted && me.image == image()
        && (me.image_state == "Hold" || me.image_state == "Fire")
}

// `ContainerRayCast(..., $TypeMasks::StaticObjectType)`: is any brick or
// piece of map, other than what the hook struck, in the way? Players and
// vehicles are not.
fn clear_line(p, from, to, kind, id) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];
    let left = (dx * dx + dy * dy + dz * dz).sqrt();
    if left < 0.01 {
        return true;
    }
    let dir = [dx / left, dy / left, dz / left];
    let at = from;
    for i in 0..8 {
        let h = raycast(at, dir, left, p);
        if h == () || h.distance >= left - 0.05 {
            return true;
        }
        if h.kind == "brick" || h.kind == "map" {
            return h.kind == kind && (kind == "map" || h.id == id);
        }
        let step = h.distance + 0.05;
        at = [at[0] + dir[0] * step, at[1] + dir[1] * step, at[2] + dir[2] * step];
        left -= step;
    }
    true
}

fn let_go(p) {
    if get_player(p, "rope").len() == 3 {
        if tethered(p) != () {
            untether(p);
        }
        set_player(p, "rope", []);
    }
}

fn on_projectile_hit(hit) {
    if hit.projectile != hook() || hit.by == () {
        return;
    }
    let p = hit.by;
    let me = player(p);
    if !holding(me) {
        return;
    }
    let at = [hit.x, hit.y, hit.z];
    if !clear_line(p, [me.x, me.y + lift(), me.z], at, hit.kind, hit.id) {
        return;
    }
    let dx = hit.x - me.x;
    let dy = hit.y - me.y;
    let dz = hit.z - me.z;
    if (dx * dx + dy * dy + dz * dz).sqrt() > longest() {
        return;
    }
    if tethered(p) != () {
        untether(p);
    }
    // As long as it spans now; the movement keys do not pump it.
    tether(p, at, (), #{ swing: 0.0 });
    set_player(p, "rope", at);
}

// `onRelease`: letting go of the click (and taking the launcher out).
fn cmd_release(p) {
    let_go(p);
}

// Put away, seated or dead, or the rope broke: off it.
fn on_tick() {
    for pl in players() {
        let p = pl.id;
        if get_player(p, "rope").len() == 3 && (!holding(pl) || tethered(p) == ()) {
            let_go(p);
        }
    }
}
