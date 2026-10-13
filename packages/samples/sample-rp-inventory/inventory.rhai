// Roleplay Backpack and Chest: `/chest` opens a window with the player's
// backpack, a chest every player shares and a shop. Items are dragged
// between the backpack and the chest; the shop sells through the Roleplay
// Bank (`call_addon`), so a player who cannot pay gets nothing. Backpacks
// and the chest are kept in the host's data for this Add-On, so they
// survive restarts.

fn bank() { "sample-rp-bank" }
fn room(id) { if id == "pack" { 8 } else { 12 } }

fn shop() {
    [#{ name: "Bread", price: 2 }, #{ name: "Rope", price: 5 },
     #{ name: "Lamp", price: 8 }, #{ name: "Pickaxe", price: 15 }]
}

fn kept(key) {
    let v = host_data(key);
    if v == () { [] } else { v }
}

fn backpack(p) {
    let all = host_data("packs");
    if all == () { return []; }
    let mine = all[player(p).uid];
    if mine == () { [] } else { mine }
}

fn set_backpack(p, items) {
    let all = host_data("packs");
    if all == () { all = #{}; }
    all[player(p).uid] = items;
    set_host_data("packs", all);
}

fn show(p, note) {
    let money = call_addon(bank(), "balance", p);
    let rows = [];
    for item in shop() { rows.push(`${item.name}  $${item.price}`); }
    show_panel(p, #{
        id: "chest", title: "Backpack and Chest", width: 360, height: 420,
        items: [
            #{ kind: "text", text: `You have $${money}. Drag items between your backpack and the chest.\n${note}` },
            #{ kind: "list", id: "pack", rows: backpack(p), height: 96, drag: true },
            #{ kind: "list", id: "chest", rows: kept("chest"), height: 96, drag: true },
            #{ kind: "list", id: "shop", rows: rows, height: 72, selected: 0 },
            #{ kind: "buttons", buttons: [#{ id: "buy", label: "Buy" }] }
        ]
    });
}

// Everyone with the window open sees the chest as it is now.
fn refresh_others(p) {
    for v in get("viewers") {
        if v != p { show(v, ""); }
    }
}

fn cmd_chest(p) {
    let viewers = get("viewers");
    if !viewers.contains(p) { viewers.push(p); set("viewers", viewers); }
    show(p, "");
}

fn list_of(p, id) {
    if id == "pack" { backpack(p) } else { kept("chest") }
}

fn set_list(p, id, items) {
    if id == "pack" { set_backpack(p, items); } else { set_host_data("chest", items); }
}

fn move_item(p, drop, to) {
    let from = drop.from;
    let items = list_of(p, from);
    // The row must still hold what the player dragged: someone else may
    // have taken it from the chest since their window was drawn.
    if drop.row >= items.len() || items[drop.row] != drop.text {
        return "Someone moved that first.";
    }
    let item = items.remove(drop.row);
    if from == to {
        let at = if drop.at == () || drop.at > items.len() { items.len() } else { drop.at };
        items.insert(at, item);
        set_list(p, from, items);
        return "";
    }
    let into = list_of(p, to);
    if into.len() >= room(to) {
        return if to == "pack" { "Your backpack is full." } else { "The chest is full." };
    }
    let at = if drop.at == () { into.len() } else { drop.at };
    into.insert(at, item);
    set_list(p, from, items);
    set_list(p, to, into);
    ""
}

fn buy(p, row) {
    if row == () { return "Pick something to buy."; }
    let item = shop()[row];
    let items = backpack(p);
    if items.len() >= room("pack") { return "Your backpack is full."; }
    try {
        call_addon(bank(), "pay", p, item.price);
    } catch {
        return `You cannot afford the ${item.name}.`;
    }
    items.push(item.name);
    set_backpack(p, items);
    `Bought the ${item.name}.`
}

fn on_panel(p, e) {
    if e.control == "close" {
        let viewers = get("viewers");
        let at = viewers.index_of(p);
        if at >= 0 { viewers.remove(at); set("viewers", viewers); }
        return;
    }
    let note = "";
    if e.drop != () {
        note = move_item(p, e.drop, e.control);
    } else if e.control == "buy" {
        note = buy(p, e.lists.shop);
    } else {
        return;
    }
    show(p, note);
    refresh_others(p);
}

fn on_leave(p) {
    let viewers = get("viewers");
    let at = viewers.index_of(p);
    if at >= 0 { viewers.remove(at); set("viewers", viewers); }
}

// Money handed out elsewhere shows at once in an open window.
fn on_addon_event(from, name, data) {
    if get("viewers").contains(data.player) { show(data.player, ""); }
}
