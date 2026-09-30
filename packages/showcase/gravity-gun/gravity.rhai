// Gravity Gun: grab, drag, swing, reel and fling players, corpses,
// vehicles and Add-On creatures, the way Garry's Mod's physics gun feels.
//
// - Hold left click: the beam grabs what you point at, by the very spot
//   it hit, and keeps that spot where you point, as far off as it was
//   when you grabbed it. It swings round with you and keeps its angle to
//   you. Let go and it carries on as it was moving, so flick your view
//   and let go to throw it.
// - The mouse wheel, while grabbing, reels it out or in. Right click is
//   left alone: it jets, as with any tool.
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
// Add-On reads it): [held kind, held id, beam on, throws, last throw's
// kind, last throw's id, beam length]. Kinds: 0 nothing, 1 vehicle, 2
// player (living or not), 3 entity. A throw is letting go of something
// moving fast: the effects mark it with a burst.

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
    tell(p, "Gravity Gun: hold left click to grab and drag, scroll to reel it in or out, let go to drop or fling.");
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

// Left click let go: drop it, moving as it was. Let go of something
// flying (a flick of the view), it counts as a throw.
fn cmd_release(p) {
    let b = beam_of(p);
    let target = held(p);
    if target != () {
        let o = object(target);
        let_go(p);
        if o != () && o.speed > 12.0 {
            b[3] += 1;
            b[4] = b[0];
            b[5] = b[1];
        }
    }
    b[0] = 0;
    b[1] = 0;
    b[2] = 0;
    set_player(p, "beam", b);
}

// The mouse wheel while grabbing: reel it out (rolled forward) or in.
// Each notch moves it about a sixth of the way.
fn cmd_reel(p, notches) {
    if held(p) == () {
        return;
    }
    let b = beam_of(p);
    let n = clampf(notches.to_float(), -5.0, 5.0);
    let reach = clampf(b[6] * 1.18 ** n, nearest(), farthest());
    hold_distance(p, reach);
    b[6] = reach;
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
