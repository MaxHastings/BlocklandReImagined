// Gravity Gun: grab, drag, hold, throw and punt players, vehicles and
// Add-On creatures. Right click (jet) grabs what you aim at or drops what
// you hold; left click punts it away, and holding left click charges a
// throw of what you hold.
//
// Everything here is policy. The engine's `physics` operations do the
// moving, and refuse anything the minigame or trust rules forbid; thrown
// things credit the thrower, so a tank thrown in a minigame kills.
//
// Each player's `beam` is what their game draws (the gravity-gun-fx
// Add-On reads it): [held kind, held id, charging, shots fired, last shot's
// kind, last shot's id]. Kinds: 0 nothing, 1 vehicle, 2 player, 3 entity.

fn clampf(x, lo, hi) {
    if x < lo { lo } else if x > hi { hi } else { x }
}

fn maxf(a, b) {
    if a > b { a } else { b }
}

fn kind_code(kind) {
    if kind == "vehicle" { 1 } else if kind == "player" { 2 } else if kind == "entity" { 3 } else { 0 }
}

fn beam_of(p) {
    let b = get_player(p, "beam");
    if type_of(b) != "array" || b.len() != 6 {
        b = [0, 0, 0, 0, 0, 0];
    }
    b
}

// How hard anything is thrown scales with how heavy it is: a player
// flies, a tank lobs, a wrecking ball barely leaves your hands.
fn heft(o) {
    clampf(sqrt(300.0 / maxf(o.mass, 30.0)), 0.45, 1.4)
}

fn on_join(p) {
    give_item(p, get("tool"), false);
}

fn cmd_gravitygun(p) {
    give_item(p, get("tool"), true);
    tell(p, "Gravity Gun: right click grabs or drops. Left click punts; hold it to charge a throw.");
}

fn cmd_grab(p) {
    let b = beam_of(p);
    if held(p) != () {
        let_go(p);
        b[0] = 0;
        b[1] = 0;
        b[2] = 0;
        set_player(p, "beam", b);
        set_player(p, "charge_at", -1);
        return;
    }
    let a = aim();
    if a == () || a.object == () || !a.movable {
        return;
    }
    let o = object(a.object);
    if o == () {
        return;
    }
    // Drag it in to just past its own size, then hold it there.
    hold(p, a.object, clampf(o.radius + 2.5, 3.5, 10.0));
    b[0] = kind_code(o.kind);
    b[1] = o.id;
    b[2] = 0;
    set_player(p, "beam", b);
}

fn cmd_charge(p) {
    set_player(p, "charge_at", tick());
    let b = beam_of(p);
    b[2] = 1;
    set_player(p, "beam", b);
}

fn cmd_fire(p) {
    let me = player(p);
    if me == () {
        return;
    }
    // A full charge takes three quarters of a second.
    let started = get_player(p, "charge_at");
    let charge = 0.0;
    if started >= 0 {
        charge = clampf((tick() - started).to_float() / 90.0, 0.0, 1.0);
    }
    set_player(p, "charge_at", -1);
    let b = beam_of(p);
    b[2] = 0;
    let target = held(p);
    let throwing = target != ();
    if throwing {
        let_go(p);
        b[0] = 0;
        b[1] = 0;
    } else {
        let a = aim();
        if a != () && a.object != () && a.movable {
            target = a.object;
        }
    }
    if target != () {
        let o = object(target);
        if o != () {
            let speed = if throwing { 24.0 + 46.0 * charge } else { 22.0 };
            let v = speed * heft(o);
            let lift = if throwing { 2.0 } else { 5.0 };
            let vx = me.lx * v;
            let vy = me.ly * v + lift;
            let vz = me.lz * v;
            if o.kind == "player" {
                tumble(target, vx, vy, vz, p);
            } else if throwing {
                // A throw replaces how it was swinging in the beam.
                push(target, vx - o.vx, vy - o.vy, vz - o.vz, p);
            } else {
                push(target, vx, vy, vz, p);
            }
            b[3] += 1;
            b[4] = kind_code(o.kind);
            b[5] = o.id;
        }
    }
    set_player(p, "beam", b);
}

// Twenty times a second: a beam whose hold ended (the object broke, the
// rules changed, the gun was put away) goes dark.
fn on_tick() {
    let tool = get("tool");
    for pl in players() {
        let p = pl.id;
        let b = beam_of(p);
        let holding = held(p);
        let armed = pl.item == tool && pl.alive;
        if holding != () && !armed {
            let_go(p);
            holding = ();
        }
        let changed = false;
        if b[0] != 0 && holding == () {
            b[0] = 0;
            b[1] = 0;
            changed = true;
        }
        if b[2] != 0 && !armed {
            b[2] = 0;
            set_player(p, "charge_at", -1);
            changed = true;
        }
        if changed {
            set_player(p, "beam", b);
        }
    }
}
