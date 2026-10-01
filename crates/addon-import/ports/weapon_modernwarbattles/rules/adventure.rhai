// Host rules for Bushido's Adventurer's Weapons (both releases: Weapon_
// ModernWarbattles and Glass 1019, Weapon_AdventurePack), written for this
// game from what their hl2 ammo system, hit scripts and taser did. Filled
// in at import from the player's own copy: the tables below are its
// datablocks' numbers, the values the port's patterns read from its
// scripts.

// Projectile => #{ headshotmultiplier }, from each projectile (and each
// hitscan gun's ray, which has its image's fields).
fn headshots() { {{headshots}} }
// Ammo box item => #{ ammotype } (none, or "ALL", for the box of every type).
fn boxes() { {{boxes}} }
// Gun item => the engine's ammo name.
fn guns() { {{magazine_items}} }
// The original's ammo type => #{ ammo, reserve, max_reserve }; a box adds
// the type's `reserve`, the amount a player starts with.
fn ammo_types() { {{magazine_types}} }
// Whether a box feeds each carried gun of a type (ModernWarbattles) or
// each type once (1019).
fn each_gun() { {{each_gun}} }
// Hitscan ray => #{ crit, kill, shove, up }: a crit test (×3 on a hit
// above the target's middle less 3.3 times its scale), a kill (twice the
// target's health, as the melee swings), and the shove each gives.
fn rays() { {{rays}} }
// Taser projectile => #{ spread, offset, up, ms }: the random sideways
// push (getRandom(spread) - offset on each axis), the push up, and how
// long the tumble lasts.
fn tasers() { {{tasers}} }

// Where the shooter faces, flat: Torque's getForwardVector.
fn forward(p) {
    let me = player(p);
    let len = (me.lx * me.lx + me.lz * me.lz).sqrt();
    if len < 0.0001 { [0.0, 0.0] } else { [me.lx / len, me.lz / len] }
}

fn on_damage(victim, attacker, amount, info) {
    if info.kind != "weapon" || info.projectile == () {
        return ();
    }
    let rays = rays();
    if rays.contains(info.projectile) {
        let ray = rays[info.projectile];
        let me = player(victim);
        if attacker != () && player(attacker) != () {
            let f = forward(attacker);
            push(`player:${victim}`, f[0] * ray.shove, ray.up, f[1] * ray.shove, attacker);
        }
        if ray.kill != () {
            return me.max_health * me.scale * ray.kill;
        }
        if ray.crit && info.y > me.cy - 3.3 * me.scale {
            return amount * 3.0;
        }
        return ();
    }
    // A projectile's damage times its headshotMultiplier on a head hit, and
    // on any hit to a crouched player, as the original's hitbox test
    // counted a crouched target's head.
    let table = headshots();
    if !table.contains(info.projectile) {
        return ();
    }
    let multiplier = table[info.projectile].headshotmultiplier;
    let head = info.region == "head" || player(victim).crouched;
    if head && multiplier != 1 {
        return amount * multiplier;
    }
    ()
}

// The taser knocks a player who is not seated or riding off their feet:
// a random push sideways and one up, then a tumble of its length.
fn on_projectile_hit(hit) {
    let tasers = tasers();
    if hit.kind != "player" || !tasers.contains(hit.projectile) {
        return;
    }
    let me = player(hit.id);
    if me == () || me.mounted {
        return;
    }
    let t = tasers[hit.projectile];
    let n = t.spread + 1;
    let x = (hash3(seed(), tick(), hit.id, 0) * n).floor() - t.offset;
    let z = (hash3(seed(), tick(), hit.id, 1) * n).floor() - t.offset;
    tumble(hit.id, me.vx + x, me.vy + t.up, me.vz + z, hit.by, t.ms / 1000.0);
}

// An ammo box tops up the reserve of the carried guns' ammo: every type
// for "ALL", its own type twice over for a typed box. The original uses
// the box up whenever it is a box a living player touched, given anything
// or not. A spare gun from a spawn brick, touched by a player carrying
// that gun, gives a box's worth of its ammo and respawns.
fn on_pickup(p, item, info) {
    let types = ammo_types();
    let guns = guns();
    let all_boxes = boxes();
    let carried = [];
    for tool in player(p).tools {
        if guns.contains(tool) && (each_gun() || !carried.contains(guns[tool])) {
            carried.push(guns[tool]);
        }
    }
    if !all_boxes.contains(item) {
        if info.spawner == () || !guns.contains(item) || !player(p).tools.contains(item) {
            return ();
        }
        // Only while that ammo is short of full; a full player picks it up
        // as usual, into another slot if they have room.
        let ammo = guns[item];
        let reserves = player(p).reserves;
        for name in types.keys() {
            let t = types[name];
            let have = if reserves.contains(ammo) { reserves[ammo] } else { 0 };
            if t.ammo == ammo && have != () && have < t.max_reserve {
                give_ammo(p, ammo, t.reserve);
                return "take";
            }
        }
        return ();
    }
    let kind = all_boxes[item].ammotype;
    let all = kind == () || kind.to_lower() == "all";
    for name in types.keys() {
        let t = types[name];
        for ammo in carried {
            if ammo != t.ammo {
                continue;
            }
            if all {
                give_ammo(p, t.ammo, t.reserve);
            } else if name.to_lower() == kind.to_lower() {
                give_ammo(p, t.ammo, t.reserve * 2);
            }
        }
    }
    "take"
}
