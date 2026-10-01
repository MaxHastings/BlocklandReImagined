// HookShot: point and click, and fly to the spot.
//
// - Click: a spring-loaded spearhead shoots out on its chain where you
//   aim. If it bites a brick, piece of map, player or vehicle within
//   `reach()`, the chain hauls you straight to it at full speed, and lets
//   go when you get there: you land on the spot. A miss shoots out and
//   rattles back.
// - Click again mid-flight to let go early, flying on at a third of the
//   speed. Putting it away lets go too.
//
// Everything here is policy. The engine's rope (`tether`, pulling
// `straight`) does the hauling, braking you to a stop at the end, and it
// breaks by itself if what the spearhead bit goes, or the player dies or
// sits. The same in minigames and out.
//
// Outside minigames every player gets the HookShot whenever their items
// are set (spawning, respawning, leaving a minigame). Inside one, the
// minigame's loadout decides.
//
// Each player's `hook` is what their game draws (the hookshot-fx Add-On
// reads it): [phase, x, y, z, distance, kind, id], as the Grappling
// Hook's: phases 0 none, 1 flying out to x, 2 hauling in to x, 3 missed;
// kind 0 a brick or the map, 1 player `id`, 2 vehicle `id`. `flight` is
// the host's own note: [landing tick, brick (-1 for none), object ref (""
// for none), offset from the object xyz]; while hauling, [give-up tick].

// Farthest it shoots, units; how fast the spearhead flies, units a second
// (the effects draw it at this speed).
fn reach() { 64.0 }
fn speed() { 120.0 }
// How close the chain hauls you (rope length, hands to spearhead), and
// how fast.
fn shortest() { 2.0 }
fn haul() { 80.0 }
// Hauled this long without arriving (something in the way), it lets go.
fn give_up_ticks() { 360 }

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

fn let_go(p) {
    if tethered(p) != () {
        untether(p);
    }
    clear(p);
}

// When a spearhead shot `distance` away lands.
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

fn cmd_hookshot(p) {
    let me = player(p);
    if me == () {
        return;
    }
    if me.minigame != () {
        tell(p, "In a minigame, its loadout decides your items.");
        return;
    }
    give_item(p, get("tool"), true);
    tell(p, "HookShot: point and click to fly to the spot. Click again to let go.");
}

// Let go mid-haul: they fly on, but the chain's grip slows them to a
// fraction of the haul's speed first, so letting go in front of a wall is
// a tumble, not a splat.
fn let_go_early(p) {
    if tethered(p) != () {
        untether(p, #{ keep: keep() });
    }
    clear(p);
}
fn keep() { 0.35 }

// Click: let go mid-flight, else shoot.
fn cmd_fire(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let h = hook_of(p);
    if h[0] == 1 || h[0] == 2 {
        let_go_early(p);
        return;
    }
    if !me.alive || me.mounted {
        return;
    }
    let eye = [me.ex, me.ey, me.ez];
    let look = [me.lx, me.ly, me.lz];
    let hit = raycast(eye, look, reach(), p);
    let bites = hit != () && (hit.kind == "brick" || hit.kind == "map"
        || hit.kind == "player" || hit.kind == "vehicle");
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
        // Nothing it may bite: out as far as it goes and back.
        let far = if hit == () { reach() } else { hit.distance };
        set_player(p, "hook", [3, me.ex + me.lx * far, me.ey + me.ly * far, me.ez + me.lz * far, far, 0, 0]);
        set_player(p, "flight", [landing(far) * 2 - tick(), -1, "", 0.0, 0.0, 0.0]);
    }
}

// Forty times a second: spearheads land, haulers arrive, and chains that
// broke or were put away let go.
fn on_tick() {
    let tool = get("tool");
    let now = tick();
    for pl in players() {
        let p = pl.id;
        let h = hook_of(p);
        if h[0] == 0 {
            continue;
        }
        if !pl.alive || pl.mounted {
            let_go(p);
            continue;
        }
        if pl.item != tool {
            let_go_early(p);
            continue;
        }
        let flight = get_player(p, "flight");
        if h[0] == 1 && now >= flight[0] {
            // It bites where it met the brick or map, or the same spot on
            // the player or vehicle wherever it has moved to since.
            let anchor = [h[1], h[2], h[3]];
            let options = #{ reel: haul(), swing: 0.0, straight: true };
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
            if (dx * dx + dy * dy + dz * dz).sqrt() > reach() + 4.0 {
                clear(p);
                continue;
            }
            tether(p, anchor, (), options);
            tether_length(p, shortest());
            h[0] = 2;
            h[1] = anchor[0];
            h[2] = anchor[1];
            h[3] = anchor[2];
            set_player(p, "hook", h);
            set_player(p, "flight", [now + give_up_ticks(), -1, "", 0.0, 0.0, 0.0]);
        } else if h[0] == 2 {
            let t = tethered(p);
            if t == () {
                // The chain broke (what it bit went, or it could not reach).
                clear(p);
                continue;
            }
            let dx = t.x - pl.x;
            let dy = t.y - pl.y - 2.25 * pl.scale;
            let dz = t.z - pl.z;
            let near = (dx * dx + dy * dy + dz * dz).sqrt() < shortest() + 0.6;
            if (near && t.length <= shortest() + 0.05) || now >= flight[0] {
                // There (or stuck): it lets go and they land.
                let_go(p);
            }
        } else if h[0] == 3 && now >= flight[0] {
            clear(p);
        }
    }
}
