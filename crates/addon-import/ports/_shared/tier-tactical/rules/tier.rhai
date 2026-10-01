// Host rules for the Tier+Tactical packs, written for this game from what
// Support_TT_Ammo.cs, Item_Ammo.cs, Ammo_Unified.cs and
// TT_processHeadshotDamage did. Filled in at import from the player's own
// copy: the tables are its datablocks' numbers and its TT_registerAmmoType
// calls. Its $Pref::Server::TT:: preferences are server settings the host
// changes (`pref`), whichever pack registers each; one no running pack
// registers keeps the port's default. Every pack's rules hand out the ammo
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
// Image id => #{ archetype } its scripts lay over the holder
// (Support_AltDatablock's pushDatablock: a machine gun's slow walk).
fn laid() { {{laid}} }
// Image id => #{ image } its jet press mounts in its place
// (Support_ImageAltTrigger's onAltTrigger: a second fire mode).
fn alt_images() { {{alt_images}} }
// Image id => #{ headshot } for the guns whose TT_isRaycastCritical is
// TT_isRaycastHeadshot (Tier 2's Military Sniper).
fn crit_images() { {{crit_images}} }
// Image id => #{ tt_raycastcritdirectdamagetype }: a crit's kill message.
fn crit_types() { {{crit_types}} }
// Image id => #{ head, below, ahead, up, type, after_turn } for Kai's
// ricocheting rifles (ShortRifleKai's onRaycastDamage and
// isRaycastCritical): a crit's multiplier to the head and from below, the
// shove along the shooter's facing and up on every hit, the crit's kill
// message, and that only a ray that has turned crits.
fn ricochet_shots() { {{ricochet_shots}} }
// The crit Add-On (Emote_Critical), or () for none: Support_TT_Raycasting
// crits only while it is on, as it checked isObject(CritProjectile).
fn critical() { {{critical}} }
// The bag a dead player's ammo spills into (Ammo_Unified.cs).
fn bag() { "{{namespace}}:weapon/ammodroppeditem" }

// Types Frog's pack registers with canDrop false (its heavy cache); the
// bag is Tier 1's, which never sees that pack's own table.
fn kept_on_death() { ["payload", "strikerocket", "50cal"] }
// The types the packs put in the "nades" set (Explosive 1's and Frog's
// grenades), whose drops and infinite supply have their own preferences;
// every other type is in "weps". Frog's Impact Grenades are in both.
fn grenade_types() { ["fragnades", "sticknades", "molnades", "impactgrenade", "smokegrenade"] }
fn weapon_type(name) { !grenade_types().contains(name.to_lower()) || name.to_lower() == "impactgrenade" }

// A Tier+Tactical preference ($Pref::Server::TT::<name>) as the host set
// it, or `fallback` when no running pack registers it.
fn tt(name, fallback) {
    let v = pref("$Pref::Server::TT::" + name);
    if v == () { fallback } else { v }
}

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
// type (Start<type>). Bots with BotInfAmmo never run out of a weapon's,
// and PlayerInfNades and BotInfNades give endless grenades.
fn on_spawn(p) {
    let me = player(p);
    let bot = me != () && me.bot;
    for name in registered().keys() {
        let t = ammo_type(name);
        if t == () {
            continue;
        }
        let grenade = !weapon_type(name);
        let endless = if grenade {
            tt(if bot { "BotInfNades" } else { "PlayerInfNades" }, false)
        } else {
            bot && tt("BotInfAmmo", false)
        };
        set_reserve(p, t.ammo, if endless { () } else { tt("Start" + name, t.reserve) });
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
    let ricochet = ricochet_hit(victim, attacker, amount, info);
    if ricochet != () {
        return ricochet;
    }
    let crit = raycast_crit(victim, attacker, amount, info);
    if crit != () {
        return crit;
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
    if me == () || !info.contains("y") || !head(me, info.y) {
        return ();
    }
    #{ amount: amount * multiplier, type: shot.type }
}

// TT_isRaycastHeadshot's test, as the projectiles' above.
fn head(me, y) {
    let scale = if me.scale > 0.0 { me.scale } else { 1.0 };
    let centre = if me.crouched { 2.0 } else { 5.3 };
    y - me.y > (centre - 3.3) * scale
}

// A ricocheting rifle's hit (ShortRifleKai's onRaycastDamage): every hit
// shoves the victim along the shooter's facing and up. While
// Emote_Critical is on, one the ray made after turning crits
// (isRaycastCritical): struck on the underside of the body, from below and
// not by its own shooter, `below` times the damage, else to the head
// `head` times, under the crit's kill message, with the crit's burst and
// sounds as raycast_crit plays them. `()` for any other shot.
fn ricochet_hit(victim, attacker, amount, info) {
    let ray = info.projectile;
    if !ray.ends_with("ray") {
        return ();
    }
    let image = ray.sub_string(0, ray.len() - 3);
    image.replace(":projectile/", ":image/");
    let shots = ricochet_shots();
    if !shots.contains(image) {
        return ();
    }
    let shot = shots[image];
    let me = player(victim);
    if me == () {
        return ();
    }
    let shooter = if attacker == () { () } else { player(attacker) };
    ricochet_shove("player:" + victim, shot, shooter, attacker);
    let critical = critical();
    if critical == () || !enabled(critical) || !shot.contains("after_turn")
        || !info.contains("bounces") || info.bounces < 1 || !info.contains("y") {
        return ();
    }
    let scale = if me.scale > 0.0 { me.scale } else { 1.0 };
    // Torque's player box: its underside is a face looking straight down,
    // met only by a ray climbing into it at the feet.
    let below = info.contains("dy") && info.dy > 0.0 && info.y - me.y < 0.1 * scale;
    let times = if below && attacker != victim {
        shot.below
    } else if head(me, info.y) {
        shot.head
    } else {
        return ();
    };
    spawn_explosion(victim, `${critical}:projectile/critprojectile`, scale);
    play_sound(victim, `${critical}:sound/critrecievesound`);
    if shooter != () && get_player(attacker, "crit_shot") != tick() {
        set_player(attacker, "crit_shot", tick());
        sound_at(`${critical}:sound/critfiresound`, shooter.cx, shooter.cy, shooter.cz);
        play_sound(attacker, `${critical}:sound/crithitsound`);
    }
    let kind = shot.type;
    if type_of(kind) == "string" {
        kind.replace("$DamageType::", "");
        if kind == "" {
            kind = ();
        }
    }
    #{ amount: amount * times, type: kind }
}

// onRaycastDamage's `%col.setVelocity(...)`: `ahead` along the shooter's
// level facing and `up` straight up, added to what `target` had.
fn ricochet_shove(target, shot, shooter, attacker) {
    if shooter == () {
        return;
    }
    let flat = (shooter.lx * shooter.lx + shooter.lz * shooter.lz).sqrt();
    let fx = if flat > 0.0 { shooter.lx / flat } else { 0.0 };
    let fz = if flat > 0.0 { shooter.lz / flat } else { 0.0 };
    push(target, fx * shot.ahead, shot.up, fz * shot.ahead, attacker);
}

// A ricocheting rifle's hit on a vehicle shoves it as one on a player
// does; a vehicle never crits.
fn on_vehicle_damage(vehicle, attacker, amount, info) {
    if info.kind != "weapon" || !info.contains("projectile") || !info.projectile.ends_with("ray") {
        return ();
    }
    let ray = info.projectile;
    let image = ray.sub_string(0, ray.len() - 3);
    image.replace(":projectile/", ":image/");
    let shots = ricochet_shots();
    if shots.contains(image) {
        let shooter = if attacker == () { () } else { player(attacker) };
        ricochet_shove("vehicle:" + vehicle, shots[image], shooter, attacker);
    }
    ()
}

// TT_onRaycastDamage on a crit: three times the damage under the gun's
// crit kill message, Emote_Critical's burst on the victim at their scale
// and its received sound in their ears, and once a shot its fire sound
// where the shooter stands and its hit sound in their ears. A hitscan
// gun's ray is `<ns>:projectile/<image>ray`.
fn raycast_crit(victim, attacker, amount, info) {
    let ray = info.projectile;
    if !ray.ends_with("ray") {
        return ();
    }
    let image = ray.sub_string(0, ray.len() - 3);
    image.replace(":projectile/", ":image/");
    let critical = critical();
    if !crit_images().contains(image) || critical == () || !enabled(critical) {
        return ();
    }
    let me = player(victim);
    if me == () || !info.contains("y") || !head(me, info.y) {
        return ();
    }
    spawn_explosion(victim, `${critical}:projectile/critprojectile`, me.scale);
    play_sound(victim, `${critical}:sound/critrecievesound`);
    if attacker != () && player(attacker) != () && get_player(attacker, "crit_shot") != tick() {
        set_player(attacker, "crit_shot", tick());
        let shooter = player(attacker);
        sound_at(`${critical}:sound/critfiresound`, shooter.cx, shooter.cy, shooter.cz);
        play_sound(attacker, `${critical}:sound/crithitsound`);
    }
    let types = crit_types();
    let kind = if types.contains(image) { types[image].tt_raycastcritdirectdamagetype } else { () };
    if type_of(kind) == "string" {
        kind.replace("$DamageType::", "");
        if kind == "" {
            kind = ();
        }
    }
    #{ amount: amount * 3.0, type: kind }
}

// Item_Ammo.cs: an ammo item adds each of its lines up to the type's most
// (Max<type>), and is used up when it added any or FullAmmoPickUp is on;
// bots leave them unless BotAmmoPickUp is on. Ammo_Unified.cs: the bag a
// dead player left holds what they carried.
fn on_pickup(p, item, info) {
    let me = player(p);
    if me == () {
        return ();
    }
    if me.bot && !tt("BotAmmoPickUp", false) && (item == bag() || ammo_items().contains(item)) {
        return false;
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
        let most = tt("Max" + line[0], t.max_reserve);
        let amount = if line[1] == -1 { most } else { line[1] };
        let room = most - now;
        if amount > 0 && room > 0 {
            set_reserve(p, t.ammo, now + if amount < room { amount } else { room });
            took = true;
        }
    }
    if took || tt("FullAmmoPickUp", false) {
        sound_at(sound, me.x, me.y, me.z);
        return "take";
    }
    false
}

// Ammo_Unified.cs: a dead player's ammo spills out in a bag thrown up and
// to a random side: every type they hold any of, but the heavy cache
// Frog's pack keeps, that its set lets them drop: a weapon's with
// PlayerAmmoDrop (BotAmmoDrop for a bot), a grenade's with PlayerNadeDrop
// (BotNadeDrop).
fn on_death(victim, killer) {
    let me = player(victim);
    if me == () {
        return;
    }
    let weapons = tt(if me.bot { "BotAmmoDrop" } else { "PlayerAmmoDrop" }, !me.bot);
    let grenades = tt(if me.bot { "BotNadeDrop" } else { "PlayerNadeDrop" }, !me.bot);
    let types = ammo_types();
    let held = #{};
    let any = false;
    for name in types.keys() {
        if kept_on_death().contains(name.to_lower()) {
            continue;
        }
        let grenade = grenade_types().contains(name.to_lower());
        if !((weapon_type(name) && weapons) || (grenade && grenades)) {
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

// pushDatablock: the gun in hand lays its player type over the holder's
// as it starts firing, when the preference guarding it is on (LMGSlow).
fn cmd_lay(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let table = laid();
    if !table.contains(me.image) {
        return;
    }
    let row = table[me.image];
    if row.contains("when") {
        let v = pref(row.when);
        if v != () && !v {
            return;
        }
    }
    if on(row.archetype) {
        push_archetype(p, row.archetype);
    }
}

// Whether the Add-On an archetype comes from is on: a gun may lay one of
// another pack's (Tier 2A's scope, Tier 2's LMGArmor), which does nothing
// without it, as LMGArmor.getID() found nothing.
fn on(archetype) {
    let at = archetype.index_of(':');
    at > 0 && enabled(archetype.sub_string(0, at))
}

// popDatablock: firing stopped, the gun ran dry or left the hand. The gun
// may already be gone, so every type this pack lays is lifted; one not
// laid on is left alone.
fn cmd_lift(p) {
    for row in laid().values() {
        if on(row.archetype) {
            pop_archetype(p, row.archetype);
        }
    }
}

// onAltTrigger: jet with the gun in hand mounts its second fire mode,
// which loads from the gun's own magazine.
fn cmd_alt_fire(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let table = alt_images();
    if table.contains(me.image) {
        mount_image(p, table[me.image].image);
    }
}

// The second fire mode's onDone: the gun's own image comes back.
fn cmd_alt_done(p) {
    mount_image(p, ());
}
