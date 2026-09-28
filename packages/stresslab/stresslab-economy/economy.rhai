// Stress Lab Mining Economy. Everything here runs on the server: a client
// can only ask to mine what it aims at or to sell, and the balances it sees
// are the server's.

// What each material of stresslab-world is worth, and which pouch it fills.
// Dirt, grass and stone can be dug but are worth nothing.
fn ore(tag) {
    switch tag {
        "stresslab-world:material/coal" => #{ key: "coal", price: 2 },
        "stresslab-world:material/copper" => #{ key: "copper", price: 5 },
        "stresslab-world:material/gold" => #{ key: "gold", price: 20 },
        "stresslab-world:material/grass" => #{ key: (), price: 0 },
        "stresslab-world:material/dirt" => #{ key: (), price: 0 },
        "stresslab-world:material/stone" => #{ key: (), price: 0 },
        _ => ()
    }
}

fn price(key) {
    switch key { "coal" => 2, "copper" => 5, "gold" => 20, _ => () }
}

fn on_join(player) {
    tell(player, "Stress Lab: H mines what you aim at, G sells your ore for Bits.");
}

fn cmd_mine(player) {
    let hit = aim();
    if hit == () || hit.brick == () {
        return;
    }
    if hit.tag == () {
        tell(player, "Only the generated ground can be mined.");
        return;
    }
    let found = ore(hit.tag);
    if found == () {
        tell(player, "That cannot be mined.");
        return;
    }
    remove_brick(hit.brick);
    add_player(player, "mined", 1);
    if found.key != () {
        add_player(player, found.key, 1);
    }
}

fn sell_one(player, key) {
    let each = price(key);
    if each == () { return 0; }
    let count = get_player(player, key);
    if count <= 0 { return 0; }
    set_player(player, key, 0);
    add_player(player, "bits", count * each);
    set("sold", get("sold") + count);
    count * each
}

fn cmd_sell(player, key) {
    if price(key) == () {
        tell(player, "Nobody buys " + key + ".");
        return;
    }
    let earned = sell_one(player, key);
    tell(player, "Sold " + key + " for " + earned + " Bits.");
}

fn cmd_sell_all(player) {
    let earned = 0;
    for key in ["coal", "copper", "gold"] {
        earned += sell_one(player, key);
    }
    tell(player, "Sold your ore for " + earned + " Bits.");
}
