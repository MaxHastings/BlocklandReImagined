// Commando: a sample total conversion. Every player becomes a commando
// holding the Commando Rifle; hits on players and target dummies score,
// and a kill refills the shooter's reserve. Everything here is an Add-On
// seam any package can use: archetypes, loadouts, damage hooks, image
// state hooks, prints and sounds.
//
// The engine knows nothing of clips or reloads. The rifle's rounds are
// this rule's own player state (`clip`, `reserve`), counted in
// `on_image_script` as the rifle's states run, and the rule steers the
// rifle's state machine the way v20 scripts did, with two flags:
//   - loaded false (`set_image_loaded`): it needs a reload and has rounds
//     for one, so the rifle's Ready goes to Reload;
//   - ammo false (`set_image_ammo`): the clip is empty, so Ready goes to
//     Empty, which clicks.

fn rifle() { "sample-commando-rifle:weapon/rifle" }
fn rifle_image() { "sample-commando-rifle:image/rifle" }
fn magazine() { 8 }
fn commando() { "sample-commando:archetype/commando" }
fn dummy() { "sample-commando:entity/dummy" }
fn sentry() { "sample-commando:entity/sentry" }

// Rounds a kill puts back in the shooter's reserve.
fn bounty() { 8 }

fn on_join(player) {
    set_archetype(player, commando());
    tell(player, "Commando: right click aims down the scope, G or /reload reloads.");
}

// Every fresh set of items (spawning, joining a minigame) is one rifle,
// full.
fn on_loadout(player) {
    set_player(player, "clip", magazine());
    set_player(player, "reserve", 24);
    give_item(player, rifle(), true);
}

// Tell the held rifle what the rounds allow.
fn sync_rifle(player) {
    let clip = get_player(player, "clip");
    let reserve = get_player(player, "reserve");
    set_image_ammo(player, clip > 0);
    set_image_loaded(player, !(clip == 0 && reserve > 0));
}

fn holds_rifle(player) {
    player(player).item == rifle()
}

// The rifle's scripted states: equipping it, each shot, and the end of a
// reload, which moves rounds from the reserve into the clip.
fn on_image_script(player, image, script) {
    if image != rifle_image() { return; }
    if script == "onFire" {
        let clip = get_player(player, "clip");
        if clip > 0 { set_player(player, "clip", clip - 1); }
    } else if script == "onReload" {
        let clip = get_player(player, "clip");
        let reserve = get_player(player, "reserve");
        let moved = magazine() - clip;
        if moved > reserve { moved = reserve; }
        set_player(player, "clip", clip + moved);
        set_player(player, "reserve", reserve - moved);
    }
    sync_rifle(player);
}

fn on_spawn(player) {
    set_player(player, "streak", 0);
    center_print(player, "Commando\nRight click to aim, G to reload", 3.0);
}

// A reload on demand: only with room in the clip and rounds to fill it.
fn cmd_reload(player) {
    if !holds_rifle(player) { return; }
    if get_player(player, "clip") < magazine() && get_player(player, "reserve") > 0 {
        set_image_loaded(player, false);
    }
}

// Three dummies in a row ahead of the player who asks.
fn cmd_dummies(player) {
    let p = player(player);
    let across_x = -p.lz;
    let across_z = p.lx;
    for i in -1..2 {
        let along = 12.0;
        let side = i * 3.0;
        spawn_entity(
            dummy(),
            p.x + p.lx * along + across_x * side,
            p.y + 0.5,
            p.z + p.lz * along + across_z * side
        );
    }
    tell(player, "Three target dummies are out.");
}

// A sentry fifteen units ahead of the player who asks.
fn cmd_sentry(player) {
    let p = player(player);
    spawn_entity(sentry(), p.x + p.lx * 15.0, p.y + 0.5, p.z + p.lz * 15.0);
    tell(player, "A sentry is out. It fires at the nearest commando within 40 units.");
}

// Commandos land hard: falls hurt half as much.
fn on_damage(victim, attacker, amount, info) {
    if info.kind == "fall" { return amount / 2.0; }
    ()
}

fn on_death(victim, killer) {
    if killer == () || killer == victim { return; }
    let streak = get_player(killer, "streak") + 1;
    set_player(killer, "streak", streak);
    add_player(killer, "score", 1);
    reward(killer);
    play_sound(killer, "sample-commando-rifle:equip");
    bottom_print(killer, `+1  kill  (${streak} in a row)`, 2.0);
}

// Rounds for the reserve; an empty rifle in hand reloads with them.
fn reward(player) {
    add_player(player, "reserve", bounty());
    if holds_rifle(player) { sync_rifle(player); }
}

fn stand(me) {
    steer(me.id, 0.0, 0.0, false);
}

// Every 3/4 second a sentry fires one round at the nearest living
// player's chest, from a muzzle clear of its own body. Its rounds are the
// Add-On's own (no `by`), so they hurt anyone, in a minigame or not.
fn sentry_round() { "sample-commando-rifle:projectile/sentry" }
fn sentry(me) {
    steer(me.id, 0.0, 0.0, false);
    let eye_y = me.y + 1.5;
    let target = ();
    let nearest = 40.0 * 40.0;
    for p in players() {
        if !p.alive { continue; }
        let dx = p.x - me.x;
        let dy = p.y + 1.4 - eye_y;
        let dz = p.z - me.z;
        let d = dx * dx + dy * dy + dz * dz;
        if d < nearest { nearest = d; target = [dx, dy, dz]; }
    }
    if target == () { return; }
    let length = nearest.sqrt();
    let dx = target[0] / length;
    let dy = target[1] / length;
    let dz = target[2] / length;
    let speed = 90.0;
    fire(sentry_round(),
        me.x + dx * 1.5, eye_y + dy * 1.5, me.z + dz * 1.5,
        dx * speed, dy * speed, dz * speed);
}

// Dummies only take shots from players: blasts from other Add-Ons pass.
fn on_entity_damage(entity, attacker, amount, info) {
    if attacker == () { return 0; }
    ()
}

fn on_entity_death(entity, killer, info) {
    set("dummies_down", get("dummies_down") + 1);
    if killer == () { return; }
    add_player(killer, "score", 1);
    reward(killer);
    let e = ();
    for d in entities() {
        if d.id == entity { e = d; }
    }
    if e != () { sound_at("sample-commando-rifle:empty", e.x, e.y + 1.0, e.z); }
    bottom_print(killer, "+1  target down", 2.0);
}
