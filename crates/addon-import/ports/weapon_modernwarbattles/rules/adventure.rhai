// Host rules for Bushido's Adventurer's Weapons, written for this game from
// what its hl2 ammo system and projectile damage did. Filled in at import
// from the player's own copy: the tables below are its datablocks' numbers.

// Damage type => #{ headshotmultiplier }, from each projectile.
fn headshots() { {{headshots}} }
// Ammo box item => #{ ammotype } (none, or "ALL", for the box of every type).
fn boxes() { {{boxes}} }
// Gun item => the engine's ammo name.
fn guns() { {{magazine_items}} }
// The original's ammo type => #{ ammo, reserve, max_reserve }; a box adds
// the type's `reserve`, the amount a player starts with.
fn ammo_types() { {{magazine_types}} }

// A projectile's damage times its headshotMultiplier on a head hit, and
// on any hit to a crouched player, as the original's hitbox test counted
// a crouched target's head.
fn on_damage(victim, attacker, amount, info) {
    if info.kind != "weapon" {
        return ();
    }
    let table = headshots();
    if !table.contains(info.type) {
        return ();
    }
    let multiplier = table[info.type].headshotmultiplier;
    let head = info.region == "head" || player(victim).crouched;
    if head && multiplier != 1 {
        return amount * multiplier;
    }
    ()
}

// An ammo box tops up the reserve of every carried gun's ammo ("ALL") or
// of the carried guns of its type, and is used up when it gave anything.
// The original adds a typed box's amount twice before the cap; so does
// this. A player carrying nothing it feeds leaves it.
fn on_pickup(p, item, info) {
    let all_boxes = boxes();
    if !all_boxes.contains(item) {
        return ();
    }
    let kind = all_boxes[item].ammotype;
    let all = kind == () || kind.to_lower() == "all";
    let types = ammo_types();
    let guns = guns();
    let carried = [];
    for tool in player(p).tools {
        if guns.contains(tool) && !carried.contains(guns[tool]) {
            carried.push(guns[tool]);
        }
    }
    let gave = false;
    for name in types.keys() {
        let t = types[name];
        if !carried.contains(t.ammo) {
            continue;
        }
        if all {
            give_ammo(p, t.ammo, t.reserve);
            gave = true;
        } else if name.to_lower() == kind.to_lower() {
            give_ammo(p, t.ammo, t.reserve * 2);
            gave = true;
        }
    }
    if gave { "take" } else { false }
}
