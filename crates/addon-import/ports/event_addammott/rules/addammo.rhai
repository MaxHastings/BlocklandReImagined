// Host rules for Kai's Event_AddAmmoTT, written for this game from what
// its Player::addAmmoTT and Tier+Tactical's TT_addAmmo did: the AddAmmoTT
// wrench output hands the player (or bot) it aims at Tier+Tactical ammo.

// Tier+Tactical's ammo type => #{ ammo, reserve, max_reserve }.
fn ammo_types() { {{magazine_types}} }
// The type names in the order the output's list numbers them from 1.
fn ammo_order() { {{magazine_order}} }

// The most of a type a player carries: its Max preference as the host
// set it, else the type's own.
fn most(name, t) {
    let v = pref("$Pref::Server::TT::Max" + name);
    if v == () { t.max_reserve } else { v }
}

// TT_addAmmo: -1 is a full load; without Ignore Max the rounds stop at
// the most; nothing happens for none, or for a type that never runs out.
fn add_ammo(p, name, amount, ignore_max) {
    let t = ammo_types()[name];
    let most = most(name, t);
    if amount == -1 {
        amount = most;
    }
    if amount <= 0 {
        return;
    }
    let me = player(p);
    if me.reserves.contains(t.ammo) && me.reserves[t.ammo] == () {
        return; // never runs out already
    }
    let have = if me.reserves.contains(t.ammo) { me.reserves[t.ammo] } else { 0 };
    if !ignore_max {
        // Only up to the most, and none for a player already past it.
        if have + amount > most {
            amount = most - have;
        }
        if amount <= 0 {
            return;
        }
    }
    let rounds = have + amount;
    set_reserve(p, t.ammo, if rounds > 100000 { 100000 } else { rounds });
}

// Player::addAmmoTT: list choice 0 is every type, n the nth.
fn on_brick_output(output, target, params, info) {
    if output != "AddAmmoTT" || player(target) == () {
        return;
    }
    let order = ammo_order();
    let choice = params[0];
    if choice == 0 {
        // Every type a running pack registered: those its players carry.
        let carried = player(target).reserves;
        for name in order {
            if carried.contains(ammo_types()[name].ammo) {
                add_ammo(target, name, params[1], params[2]);
            }
        }
    } else if choice >= 1 && choice <= order.len() {
        add_ammo(target, order[choice - 1], params[1], params[2]);
    }
}
