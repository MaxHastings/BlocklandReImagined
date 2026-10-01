// Grapple Rope: throw a hook into a brick or the map and swing on the
// rope. You need momentum. Think Tarzan. Swing.
//
// - Hold left click: the hook flies out where you aim and bites into the
//   first brick or piece of map it meets, up to `reach()` away. Keep
//   holding to hang on; let go to drop off the rope, keeping every bit of
//   speed the swing gave you.
// - While hanging, the movement keys pump the swing, and the mouse wheel
//   climbs up the rope (rolled forward) or pays it out (rolled back).
//   Right click is left alone: it jets, as with any tool.
//
// Everything here is policy. The engine's rope (`tether`) does the
// swinging: it is part of the player's own movement, so it feels the same
// on the player's screen as on the host, and it breaks by itself if the
// brick it bit into goes, or the player dies, sits or is flung far away.
// The hook bites only bricks and the map, never players or vehicles, the
// same in minigames and out.
//
// Outside minigames every player gets the rope whenever their items are
// set (spawning, respawning, leaving a minigame). Inside one, the
// minigame's loadout decides.
//
// Each player's `rope` is what their game draws (the grapple-rope-fx
// Add-On reads it): [phase, x, y, z, length]. Phases: 0 none, 1 the hook
// flying out to x, 2 hooked at x with the rope reeling toward `length`,
// 3 missed, the hook flying out to x and falling back. `flight` is the
// host's own note of when a flying hook lands and on which brick
// ([tick, brick], -1 for the map).

// Farthest the hook flies, units.
fn reach() { 64.0 }
// How fast it flies, units a second (the effects draw it at this speed).
fn speed() { 160.0 }
// Shortest the rope climbs to, and how much one notch of the wheel climbs.
fn shortest() { 2.0 }
fn notch() { 2.5 }

fn clampf(x, lo, hi) {
    if x < lo { lo } else if x > hi { hi } else { x }
}

fn rope_of(p) {
    let r = get_player(p, "rope");
    if type_of(r) != "array" || r.len() != 5 {
        r = [0, 0.0, 0.0, 0.0, 0.0];
    }
    r
}

fn clear(p) {
    set_player(p, "rope", [0, 0.0, 0.0, 0.0, 0.0]);
    set_player(p, "flight", [0, -1]);
}

// When a hook thrown `distance` away lands.
fn landing(distance) {
    tick() + (distance / speed() * 120.0).ceiling().to_int()
}

fn on_loadout(p) {
    let me = player(p);
    if me != () && me.minigame == () {
        give_item(p, get("tool"), false);
    }
}

fn cmd_grapplerope(p) {
    let me = player(p);
    if me == () {
        return;
    }
    if me.minigame != () {
        tell(p, "In a minigame, its loadout decides your items.");
        return;
    }
    give_item(p, get("tool"), true);
    tell(p, "Grapple Rope: hold left click to hook on and swing, let go to fly off. WASD pumps the swing, the mouse wheel climbs.");
}

// Left click pressed: throw the hook where they aim.
fn cmd_hook(p) {
    let me = player(p);
    if me == () || !me.alive || me.mounted {
        return;
    }
    if tethered(p) != () {
        untether(p);
    }
    let eye = [me.ex, me.ey, me.ez];
    let look = [me.lx, me.ly, me.lz];
    let hit = raycast(eye, look, reach(), p);
    if hit != () && (hit.kind == "brick" || hit.kind == "map") {
        let brick = if hit.kind == "brick" { hit.id } else { -1 };
        set_player(p, "rope", [1, hit.x, hit.y, hit.z, hit.distance]);
        set_player(p, "flight", [landing(hit.distance), brick]);
    } else {
        // Nothing to bite (open sky, a player, a vehicle): it flies out
        // as far as it goes, or to what it met, and falls back.
        let far = if hit == () { reach() } else { hit.distance };
        set_player(p, "rope", [3, me.ex + me.lx * far, me.ey + me.ly * far, me.ez + me.lz * far, far]);
        set_player(p, "flight", [landing(far) + 30, -1]);
    }
}

// Left click let go: off the rope, flying on as they were moving.
fn cmd_unhook(p) {
    if tethered(p) != () {
        untether(p);
    }
    let r = rope_of(p);
    if r[0] == 1 || r[0] == 2 {
        clear(p);
    }
}

// The mouse wheel while hooked: climb up (rolled forward) or pay out.
fn cmd_climb(p, notches) {
    let t = tethered(p);
    if t == () {
        return;
    }
    let n = clampf(notches.to_float(), -5.0, 5.0);
    let length = clampf(t.target - n * notch(), shortest(), reach() + 8.0);
    tether_length(p, length);
    let r = rope_of(p);
    r[4] = length;
    set_player(p, "rope", r);
}

// Forty times a second: hooks land, and ropes that broke are put away.
fn on_tick() {
    let tool = get("tool");
    let now = tick();
    for pl in players() {
        let p = pl.id;
        let r = rope_of(p);
        if r[0] == 0 {
            continue;
        }
        let armed = pl.item == tool && pl.alive && !pl.mounted;
        if !armed {
            if tethered(p) != () {
                untether(p);
            }
            clear(p);
            continue;
        }
        let flight = get_player(p, "flight");
        if r[0] == 1 && now >= flight[0] {
            // The hook bites, unless its brick went while it flew. The
            // rope is as long as it spans now (the engine refuses one
            // past its longest).
            if flight[1] >= 0 {
                tether(p, [r[1], r[2], r[3]], (), #{ brick: flight[1] });
            } else {
                tether(p, [r[1], r[2], r[3]], ());
            }
            r[0] = 2;
            let dx = r[1] - pl.x;
            let dy = r[2] - pl.y - 2.25 * pl.scale;
            let dz = r[3] - pl.z;
            r[4] = (dx * dx + dy * dy + dz * dz).sqrt();
            set_player(p, "rope", r);
        } else if r[0] == 2 && tethered(p) == () {
            // The rope broke (its brick went, or it could not reach).
            clear(p);
        } else if r[0] == 3 && now >= flight[0] {
            clear(p);
        }
    }
}
