// Host rules for the Tier+Tactical packs, written for this game from what
// Support_TT_Ammo.cs, Item_Ammo.cs, Ammo_Unified.cs and
// TT_processHeadshotDamage did, with their default settings. Filled in at
// import from the player's own copy: the tables are its datablocks' numbers
// and its TT_registerAmmoType calls. Every pack's rules hand out the ammo
// types it registers; Tier 1's, which own Item_Ammo.cs and Ammo_Unified.cs,
// also run ammo items and the dead player's bag for every type.

// Projectile id => #{ multiplier, type } from its `damage` method.
fn headshots() { {{headshots}} }
// Ammo item id => its TT_ammoPickup lines ("9MM 70", "-1" for a full load)
// and TT_pickupSound.
fn ammo_items() { {{ammo_items}} }
// Tier+Tactical's ammo type => #{ ammo, reserve, max_reserve }.
fn ammo_types() { {{magazine_types}} }
// The types this pack registers (TT_registerAmmoType), by name.
fn registered() { {{registered}} }
// The bag a dead player's ammo spills into (Ammo_Unified.cs).
fn bag() { "{{namespace}}:weapon/ammodroppeditem" }

// Types Frog's pack registers with canDrop false (its heavy cache); the
// bag is Tier 1's, which never sees that pack's own table.
fn kept_on_death() { ["payload", "strikerocket", "50cal"] }

// An ammo type's line by its Tier+Tactical name, whatever its case.
fn ammo_type(name) {
    let types = ammo_types();
    for key in types.keys() {
        if key.to_lower() == name.to_lower() {
            return types[key];
        }
    }
    ()
}

// What a player carries of a type, `()` when no pack registered it.
fn carried(me, t) {
    let r = me.reserves;
    if r.contains(t.ammo) { r[t.ammo] } else { () }
}

// Armor::onAdd: a new life holds the starting amount of every registered
// type.
fn on_spawn(p) {
    for name in registered().keys() {
        let t = ammo_type(name);
        if t != () {
            set_reserve(p, t.ammo, t.reserve);
        }
    }
}

// TT_processHeadshotDamage: a hit above the victim's box centre less 3.3
// (times their scale) is to the head. A v20 player's world box is four
// times its collision box, its centre 5.3 above the feet standing and 2
// crouched, so the head is the top 0.65 of a standing body and all of a
// crouched one. The shooter hears the hit.
fn on_damage(victim, attacker, amount, info) {
    if info.kind != "weapon" || !info.contains("projectile") {
        return ();
    }
    let table = headshots();
    if !table.contains(info.projectile) {
        return ();
    }
    if attacker != () && amount > 0.0 {
        play_sound(attacker, "bulletHitSound");
    }
    let shot = table[info.projectile];
    let multiplier = shot.multiplier;
    if type_of(multiplier) == "string" {
        return ();
    }
    let me = player(victim);
    if me == () || !info.contains("y") {
        return ();
    }
    let scale = if me.scale > 0.0 { me.scale } else { 1.0 };
    let centre = if me.crouched { 2.0 } else { 5.3 };
    if info.y - me.y > (centre - 3.3) * scale {
        return #{ amount: amount * multiplier, type: shot.type };
    }
    ()
}

// Item_Ammo.cs: an ammo item adds each of its lines up to the type's most,
// and is used up when it added any. Ammo_Unified.cs: the bag a dead
// player left holds what they carried.
fn on_pickup(p, item, info) {
    let me = player(p);
    if me == () {
        return ();
    }
    let lines = [];
    // Item_Ammo.cs's own sound unless the item names one of its pack's.
    let sound = "weapon_package_tier1:sound/ammogetsound";
    if item == bag() {
        if type_of(info.data) != "map" {
            return "take";
        }
        for name in info.data.keys() {
            lines.push([name, info.data[name]]);
        }
    } else {
        let items = ammo_items();
        if !items.contains(item) {
            return ();
        }
        let row = items[item];
        for i in 0..12 {
            let key = "tt_ammopickup[" + i + "]";
            if row.contains(key) {
                let words = row[key].to_string().split(" ");
                if words.len() == 2 {
                    lines.push([words[0], parse_int(words[1])]);
                }
            }
        }
        if row.contains("tt_pickupsound") {
            sound = "{{namespace}}:sound/" + row["tt_pickupsound"].to_string().to_lower();
        }
    }
    let took = false;
    for line in lines {
        let t = ammo_type(line[0]);
        if t == () {
            continue;
        }
        let now = carried(me, t);
        if now == () {
            continue;
        }
        let amount = if line[1] == -1 { t.max_reserve } else { line[1] };
        let room = t.max_reserve - now;
        if amount > 0 && room > 0 {
            set_reserve(p, t.ammo, now + if amount < room { amount } else { room });
            took = true;
        }
    }
    if took {
        sound_at(sound, me.x, me.y, me.z);
        return "take";
    }
    false
}

// Ammo_Unified.cs: a dead player's ammo spills out in a bag thrown up and
// to a random side: every registered type they hold any of, but the heavy
// cache Frog's pack keeps.
fn on_death(victim, killer) {
    let me = player(victim);
    if me == () {
        return;
    }
    let types = ammo_types();
    let held = #{};
    let any = false;
    for name in types.keys() {
        if kept_on_death().contains(name.to_lower()) {
            continue;
        }
        let n = carried(me, types[name]);
        if n != () && n > 0 {
            held[name] = n;
            any = true;
        }
    }
    if !any {
        return;
    }
    drop_item(bag(), me.x, me.y + 1.0, me.z,
        me.vx + side(victim, 0), me.vy + 4.0, me.vz + side(victim, 1), held);
}

// getRandom(-8, 8), the same on every machine.
fn side(victim, axis) {
    floor(hash3(victim, tick(), axis, 0) * 17.0) - 8.0
}
