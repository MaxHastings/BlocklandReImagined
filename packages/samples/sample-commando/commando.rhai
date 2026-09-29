// Commando: a sample total conversion. Every player becomes a commando
// holding the Commando Rifle; hits on players and target dummies score,
// and a kill refills the shooter's reserve. Everything here is an Add-On
// seam any package can use: archetypes, loadouts, damage hooks, ammo,
// prints and sounds.

fn rifle() { "sample-commando-rifle:weapon/rifle" }
fn commando() { "sample-commando:archetype/commando" }
fn dummy() { "sample-commando:entity/dummy" }

// Rounds a kill puts back in the shooter's reserve.
fn bounty() { 8 }

fn on_join(player) {
    set_archetype(player, commando());
    tell(player, "Commando: right click aims down the scope, G or /reload reloads.");
}

// Every fresh set of items (spawning, joining a minigame) is one rifle.
fn on_loadout(player) {
    give_item(player, rifle(), true);
}

fn on_spawn(player) {
    set_player(player, "streak", 0);
    center_print(player, "Commando\nRight click to aim, G to reload", 3.0);
}

fn cmd_reload(player) {
    reload(player);
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
    give_ammo(killer, rifle(), bounty());
    play_sound(killer, "sample-commando-rifle:equip");
    bottom_print(killer, `+1  kill  (${streak} in a row)`, 2.0);
}

fn stand(me) {
    steer(me.id, 0.0, 0.0, false);
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
    give_ammo(killer, rifle(), bounty());
    let e = ();
    for d in entities() {
        if d.id == entity { e = d; }
    }
    if e != () { sound_at("sample-commando-rifle:empty", e.x, e.y + 1.0, e.z); }
    bottom_print(killer, "+1  dummy down", 2.0);
}
