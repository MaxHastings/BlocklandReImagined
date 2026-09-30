// Gravity Gun: grab, drag, swing, fling and blast players, corpses,
// vehicles and Add-On creatures, the way Garry's Mod's physics gun feels.
//
// - Hold left click: the beam grabs what you point at, by the very spot
//   it hit, and keeps that spot where you point, as far off as it was
//   when you grabbed it. It swings round with you and keeps its angle to
//   you. Let go and it carries on as it was moving, so flick your view
//   and let go to throw it.
// - Right click: blast what you hold away along your view, or punt what
//   you point at.
//
// Everything here is policy. The engine's `physics` operations do the
// moving (the hold itself runs in the engine every tick), and refuse
// anything the minigame or trust rules forbid; thrown things credit the
// thrower, so a tank flung in a minigame kills. A living player grabbed
// goes limp (their tumble) until they land after being let go.
//
// Outside minigames every player gets the gun whenever their items are
// set (spawning, respawning, leaving a minigame). Inside one, the
// minigame's loadout decides.
//
// Each player's `beam` is what their game draws (the gravity-gun-fx
// Add-On reads it): [held kind, held id, beam on, blasts fired, last
// blast's kind, last blast's id, beam length]. Kinds: 0 nothing, 1
// vehicle, 2 player (living or not), 3 entity.

// How hard the hold may pull: everything up to a player's weight answers
// at once, a jeep (300) slightly slower, a steel ball (900) swings in,
// and past about 4,500 it can only be dragged.
fn force() { 90000.0 }
// Nearest and farthest a grabbed thing floats from your eye.
fn nearest() { 2.5 }
fn farthest() { 60.0 }

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
    if type_of(b) != "array" || b.len() != 7 {
        b = [0, 0, 0, 0, 0, 0, 0.0];
    }
    b
}

// Light things fly, heavy ones lob: speed falls with the square root of
// the mass, so every throw carries about the same punch.
fn blast_speed(o) {
    clampf(75.0 * sqrt(90.0 / maxf(o.mass, 1.0)), 18.0, 60.0)
}
fn punt_speed(o) {
    clampf(50.0 * sqrt(90.0 / maxf(o.mass, 1.0)), 6.0, 45.0)
}

fn on_loadout(p) {
    let me = player(p);
    if me != () && me.minigame == () {
        give_item(p, get("tool"), false);
    }
}

fn cmd_gravitygun(p) {
    let me = player(p);
    if me == () {
        return;
    }
    if me.minigame != () {
        tell(p, "In a minigame, its loadout decides your items.");
        return;
    }
    give_item(p, get("tool"), true);
    tell(p, "Gravity Gun: hold left click to grab and drag, let go to drop or fling. Right click blasts.");
}

// Left click pressed: grab what the beam meets.
fn cmd_grab(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let b = beam_of(p);
    if held(p) != () {
        let_go(p);
    }
    b[0] = 0;
    b[1] = 0;
    b[2] = 1;
    let a = aim();
    b[6] = if a == () { farthest() } else { clampf(a.distance, 0.5, farthest()) };
    if a != () && a.object != () && a.movable {
        let o = object(a.object);
        if o != () {
            let reach = clampf(a.object_distance, nearest(), farthest());
            let at = [
                me.ex + me.lx * a.object_distance,
                me.ey + me.ly * a.object_distance,
                me.ez + me.lz * a.object_distance,
            ];
            hold(p, a.object, reach, #{ at: at, force: force(), turn: true });
            b[0] = kind_code(o.kind);
            b[1] = o.id;
            b[6] = reach;
        }
    }
    set_player(p, "beam", b);
}

// Left click let go: drop it, moving as it was.
fn cmd_release(p) {
    if held(p) != () {
        let_go(p);
    }
    let b = beam_of(p);
    b[0] = 0;
    b[1] = 0;
    b[2] = 0;
    set_player(p, "beam", b);
}

// Right click: blast away what you hold, else punt what you point at.
fn cmd_blast(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let b = beam_of(p);
    let target = held(p);
    let holding = target != ();
    if holding {
        let_go(p);
        b[0] = 0;
        b[1] = 0;
    } else {
        let a = aim();
        if a != () && a.object != () && a.movable {
            target = a.object;
        }
    }
    b[3] += 1;
    b[4] = 0;
    b[5] = 0;
    if target != () {
        let o = object(target);
        if o != () {
            let v = if holding { blast_speed(o) } else { punt_speed(o) };
            let vx = me.lx * v;
            let vy = me.ly * v + v * 0.12;
            let vz = me.lz * v;
            let living = o.kind == "player" && player(o.id) != () && player(o.id).alive;
            if living {
                tumble(target, vx, vy, vz, p);
            } else if holding {
                // A blast replaces how it was swinging in the beam.
                push(target, vx - o.vx, vy - o.vy, vz - o.vz, p);
            } else {
                push(target, vx, vy, vz, p);
            }
            b[4] = kind_code(o.kind);
            b[5] = o.id;
        }
    }
    set_player(p, "beam", b);
}

// Twenty times a second: a beam whose hold ended (it snagged, the rules
// changed, the gun was put away) lets go.
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
            changed = true;
        }
        if changed {
            set_player(p, "beam", b);
        }
    }
}
