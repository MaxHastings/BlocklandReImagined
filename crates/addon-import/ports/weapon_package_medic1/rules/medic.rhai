// Host rules for Tier+Tactical's Medical pack, written for this game from
// what its server.cs (TT_canHeal, TT_projectileHeal), Item_GauzeGun.cs and
// Item_Stimpack.cs did, with Kai's default settings. Filled in at import
// from the player's own copy: the tables are its datablocks' numbers and
// its scripts' words.

// Projectile id => #{ burst }: the health its hit gives at once
// (TT_projectileHeal(%obj, %col, burst) in its onCollision).
fn heals() { {{heals}} }
// Heal image id => #{ hp, times }: what each onHeal gives the wearer, and
// how many before the image comes off (medigunHealImage::onHeal).
fn over_time() { {{over_time}} }
// Syringe image id => what its onFire does: #{ cooldown (ticks), wait,
// wait_seconds, arm, sound, notice_ticks, cross_at, cross, image, heal }.
fn syringes() { {{syringes}} }
// Image id => #{ projectile, speed, inherit, arm, sound }: its
// onAltTrigger's throw on jet.
fn throws() { {{throws}} }
// TT_projectileHeal's cross, heal image and bottom prints (the words
// before and after the name).
fn heal_shown() { {{heal}} }
// stimRechargedNotice: #{ text, seconds, sound }.
fn notice() { {{notice}} }
// $Pref::Server::TT::MedicHealBots, Kai's default.
fn heal_bots() { {{heal_bots}} }

// A new body: no syringe used, no heal running, no notice waiting (the old
// body's schedule went with it).
fn on_spawn(p) {
    set_player(p, "stim_at", -1);
    set_player(p, "healing", 0);
    let due = get("notices");
    let key = `${p}`;
    if due.contains(key) {
        due.remove(key);
        set("notices", due);
    }
}

// TT_canHeal with Kai's defaults: a player (bots only with MedicHealBots);
// in a minigame, one in the healer's; outside minigames, on a single-player
// or LAN game, one in none; with MedicHealBots on, the healer's own bots.
// Slayer's team check (MedicHealEnemy off) comes with Slayer.
fn can_heal(by, target) {
    if by == () || (target.bot && !heal_bots()) {
        return false;
    }
    let healer = player(by);
    if healer == () {
        return false;
    }
    if healer.minigame != () {
        return healer.minigame == target.minigame;
    }
    if lan() && target.minigame == () {
        return true;
    }
    target.bot && target.bot_owner == by
}

// TT_projectileHeal: the burst at once, a cross on anyone hurt by at least
// it, a bottom print to each, and the heal over time on the body.
fn heal_from(by, target, burst) {
    if !can_heal(by, target) {
        return;
    }
    let shown = heal_shown();
    if target.max_health - target.health >= burst {
        spawn_explosion(target.id, shown.cross, target.scale);
    }
    if target.alive {
        heal(target.id, burst);
    }
    let healer = player(by);
    let hide = shown.hide_bar != 0;
    if !healer.bot && target.name != "" {
        bottom_print(by, shown.healed[0] + target.name + shown.healed[1], shown.seconds, hide);
    }
    if !target.bot && healer.name != "" {
        bottom_print(target.id, shown.healing[0] + healer.name + shown.healing[1], shown.seconds, hide);
    }
    // A dead body's heal image comes off at its first onHeal.
    if target.alive {
        wear_heal(target.id, shown.image);
    }
}

// medigunHealImage::onMount: the count starts again.
fn wear_heal(p, image) {
    set_player(p, "healing", 0);
    emote(p, image);
}

fn on_projectile_hit(hit) {
    if hit.kind != "player" {
        return;
    }
    let table = heals();
    if !table.contains(hit.projectile) {
        return;
    }
    let target = player(hit.id);
    if target != () {
        heal_from(hit.by, target, table[hit.projectile].burst);
    }
}

// medigunHealImage::onHeal: a little health each pass while alive; after
// enough passes, or on a dead wearer, the image comes off.
fn cmd_heal_tick(p) {
    let me = player(p);
    if me == () {
        return;
    }
    let table = over_time();
    if !table.contains(me.emote) {
        return;
    }
    let row = table[me.emote];
    if !me.alive {
        emote(p, ());
        return;
    }
    heal(p, row.hp);
    let n = get_player(p, "healing") + 1;
    set_player(p, "healing", n);
    if n >= row.times {
        emote(p, ());
    }
}

// stimpackImage::onFire: while recharging it only says so; otherwise the
// arm and sound, the recharge notice due, the cross and heal image when
// hurt by at least cross_at, and the heal.
fn cmd_stim(p) {
    let me = player(p);
    if me == () || !syringes().contains(me.image) {
        return;
    }
    let s = syringes()[me.image];
    let last = get_player(p, "stim_at");
    if last >= 0 && last + s.cooldown > tick() {
        center_print(p, s.wait, s.wait_seconds);
        return;
    }
    play_thread(p, 2, s.arm);
    set_player(p, "stim_at", tick());
    sound_at(s.sound, me.x, me.y, me.z);
    let due = get("notices");
    due[`${p}`] = tick() + s.notice_ticks;
    set("notices", due);
    if me.max_health - me.health >= s.cross_at {
        spawn_explosion(p, s.cross, me.scale);
        wear_heal(p, s.image);
    }
    heal(p, s.heal);
}

// stimpackImage::onAltTrigger: jet throws the syringe from the muzzle along
// the aim, at its projectile's speed.
fn cmd_stim_throw(p) {
    let me = player(p);
    if me == () || !throws().contains(me.image) {
        return;
    }
    let t = throws()[me.image];
    play_thread(p, 2, t.arm);
    sound_at(t.sound, me.x, me.y, me.z);
    fire(t.projectile, me.mx, me.my, me.mz,
        me.lx * t.speed + me.vx * t.inherit,
        me.ly * t.speed + me.vy * t.inherit,
        me.lz * t.speed + me.vz * t.inherit, p);
}

// stimRechargedNotice, scheduled on the body: it says so to the same body
// if it is still alive.
fn on_tick() {
    let due = get("notices");
    if due.len() == 0 {
        return;
    }
    let now = tick();
    let left = #{};
    let n = notice();
    for key in due.keys() {
        if due[key] > now {
            left[key] = due[key];
            continue;
        }
        let me = player(parse_int(key));
        if me != () && me.alive {
            center_print(me.id, n.text, n.seconds);
            sound_at(n.sound, me.x, me.y, me.z);
        }
    }
    set("notices", left);
}
