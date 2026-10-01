// Grappling Hook: fire a hook and get pulled straight to it.
//
// - Click: the hook flies out where you aim and bites into the first
//   brick, piece of map, player or vehicle it meets, up to `reach()`
//   away, and the winch pulls you straight to it. You hang there until
//   you click again to let go.
// - While hanging you may switch items: the rope holds. Hold jump to reel
//   in, crouch to let rope out (the engine's winch keys, so it feels the
//   same on your screen as on the host), or roll the mouse wheel.
// - Hooked to a player or vehicle, you are carried along as it moves.
//   An admin turns that off (bricks and the map only) with /hookobjects.
//
// Everything here is policy. The engine's rope (`tether`) does the
// pulling and hanging, and breaks by itself if what the hook bit goes, or
// the player dies, sits or is flung far away. The same in minigames and
// out.
//
// Outside minigames every player gets the hook whenever their items are
// set (spawning, respawning, leaving a minigame). Inside one, the
// minigame's loadout decides.
//
// Each player's `hook` is what their game draws (the grappling-hook-fx
// Add-On reads it): [phase, x, y, z, distance, kind, id]. Phases: 0 none,
// 1 flying out to x, 2 hooked at x, 3 missed, flying out to x and falling
// back. `kind` is what it bit: 0 a brick or the map, 1 player `id`, 2
// vehicle `id`; the effects follow a player or vehicle as it moves.
// `flight` is the host's own note of a flying hook: [landing tick, brick
// (-1 for none), object ref ("" for none), offset from the object xyz].

// Farthest the hook flies, units.
fn reach() { 80.0 }
// How fast it flies, units a second (the effects draw it at this speed).
fn speed() { 200.0 }
// How close the winch pulls you (rope length, hands to hook), how fast,
// and how much one notch of the wheel reels.
fn shortest() { 2.5 }
fn winch() { 48.0 }
fn notch() { 3.0 }

fn clampf(x, lo, hi) {
    if x < lo { lo } else if x > hi { hi } else { x }
}

fn hook_of(p) {
    let h = get_player(p, "hook");
    if type_of(h) != "array" || h.len() != 7 {
        h = [0, 0.0, 0.0, 0.0, 0.0, 0, 0];
    }
    h
}

fn clear(p) {
    set_player(p, "hook", [0, 0.0, 0.0, 0.0, 0.0, 0, 0]);
    set_player(p, "flight", [0, -1, "", 0.0, 0.0, 0.0]);
}

// When a hook fired `distance` away lands.
fn landing(distance) {
    tick() + (distance / speed() * 120.0).ceiling().to_int()
}

// Where an object is now: a player's feet, a vehicle's middle; () when
// it is gone. (A bot is not one of the `players()`, but an object.)
fn where_is(kind, id, reference) {
    if kind == 1 {
        let pl = player(id);
        if pl != () {
            if !pl.alive { return (); }
            return [pl.x, pl.y, pl.z];
        }
    }
    let o = object(reference);
    if o == () { return (); }
    [o.x, o.y, o.z]
}

fn on_loadout(p) {
    let me = player(p);
    if me != () && me.minigame == () {
        give_item(p, get("tool"), false);
    }
}

fn cmd_grapplinghook(p) {
    let me = player(p);
    if me == () {
        return;
    }
    if me.minigame != () {
        tell(p, "In a minigame, its loadout decides your items.");
        return;
    }
    give_item(p, get("tool"), true);
    tell(p, "Grappling Hook: click to fire and get pulled in, click again to let go. Jump reels in, crouch lets out.");
}

// Admins (the command is declared for them): may the hook bite players
// and vehicles?
fn cmd_hookobjects(p) {
    let on = !get("objects");
    set("objects", on);
    if on {
        broadcast("Grappling Hooks now bite players and vehicles too.");
    } else {
        broadcast("Grappling Hooks now bite only bricks and the map.");
    }
}

// Click: let go if hooked or flying, else fire.
fn cmd_fire(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let h = hook_of(p);
    if h[0] == 1 || h[0] == 2 {
        if tethered(p) != () {
            untether(p);
        }
        clear(p);
        return;
    }
    if !me.alive || me.mounted {
        return;
    }
    let eye = [me.ex, me.ey, me.ez];
    let look = [me.lx, me.ly, me.lz];
    let hit = raycast(eye, look, reach(), p);
    let objects = get("objects");
    let bites = hit != () && (hit.kind == "brick" || hit.kind == "map"
        || (objects && (hit.kind == "player" || hit.kind == "vehicle")));
    if bites {
        let kind = if hit.kind == "player" { 1 } else if hit.kind == "vehicle" { 2 } else { 0 };
        let brick = if hit.kind == "brick" { hit.id } else { -1 };
        let reference = if kind == 0 { "" } else { hit.ref };
        let id = if kind == 0 { 0 } else { hit.id };
        let off = [0.0, 0.0, 0.0];
        if kind != 0 {
            let at = where_is(kind, id, reference);
            if at != () {
                off = [hit.x - at[0], hit.y - at[1], hit.z - at[2]];
            }
        }
        set_player(p, "hook", [1, hit.x, hit.y, hit.z, hit.distance, kind, id]);
        set_player(p, "flight", [landing(hit.distance), brick, reference, off[0], off[1], off[2]]);
    } else {
        // Nothing it may bite: it flies out as far as it goes, or to what
        // it met, and falls back.
        let far = if hit == () { reach() } else { hit.distance };
        set_player(p, "hook", [3, me.ex + me.lx * far, me.ey + me.ly * far, me.ez + me.lz * far, far, 0, 0]);
        set_player(p, "flight", [landing(far) + 30, -1, "", 0.0, 0.0, 0.0]);
    }
}

// The mouse wheel while hooked: reel in (rolled forward) or let out.
fn cmd_reel(p, notches) {
    let t = tethered(p);
    if t == () {
        return;
    }
    let n = clampf(notches.to_float(), -5.0, 5.0);
    tether_length(p, clampf(t.length - n * notch(), shortest(), reach()));
}

// Forty times a second: hooks land, and ropes that broke are put away.
// Unlike the Grapple Rope, switching items keeps you hanging.
fn on_tick() {
    let now = tick();
    for pl in players() {
        let p = pl.id;
        let h = hook_of(p);
        if h[0] == 0 {
            continue;
        }
        if !pl.alive || pl.mounted {
            if tethered(p) != () {
                untether(p);
            }
            clear(p);
            continue;
        }
        let flight = get_player(p, "flight");
        if h[0] == 1 && now >= flight[0] {
            // It bites where it met the brick or map, or the same spot on
            // the player or vehicle wherever it has moved to since.
            let anchor = [h[1], h[2], h[3]];
            let options = #{ reel: winch(), swing: 0.0, keys: [shortest(), reach()], straight: true };
            if flight[1] >= 0 {
                options.brick = flight[1];
            }
            if h[5] != 0 {
                let at = where_is(h[5], h[6], flight[2]);
                if at == () {
                    clear(p);
                    continue;
                }
                anchor = [at[0] + flight[3], at[1] + flight[4], at[2] + flight[5]];
                options.object = flight[2];
            }
            let dx = anchor[0] - pl.x;
            let dy = anchor[1] - pl.y - 2.25 * pl.scale;
            let dz = anchor[2] - pl.z;
            let spans = (dx * dx + dy * dy + dz * dz).sqrt();
            if spans > reach() + 4.0 {
                // Carried out of reach while it flew.
                clear(p);
                continue;
            }
            tether(p, anchor, (), options);
            // Then the winch pulls them straight in.
            tether_length(p, shortest());
            h[0] = 2;
            h[1] = anchor[0];
            h[2] = anchor[1];
            h[3] = anchor[2];
            set_player(p, "hook", h);
        } else if h[0] == 2 && tethered(p) == () {
            // The rope broke (what it bit went, or it could not reach).
            clear(p);
        } else if h[0] == 3 && now >= flight[0] {
            clear(p);
        }
    }
}
