// The Adventure Pack's rules: magazines and reloads, ammo reserves and
// pickups, headshots, the hitscan guns and the taser. An original remake of
// the play of Bushido's Adventure Pack for Blockland; none of its scripts
// are used. The guns' images run these commands (`tool_only`, so nobody
// types them):
//   equip      the gun comes out: loaded or not
//   fired      a shot: spend rounds; a hitscan gun traces its ray here
//   check      may it reload? ammo on sends the image to Reload, off to Empty
//   reloaded   fill the magazine, or load one shell
//   interrupt  the trigger stops a shell-by-shell reload
//   reload     the light key
// Rounds left in each gun live in the player's `mags` (by image), reserves
// in `reserve` (by kind), and `hud` is what the ammo panel shows.

// BEGIN GENERATED
// The guns and ammo, written by tools/make_adventure_pack.py.
// The gun an image belongs to, or ().
fn gun(image) {
    switch image {
        "adventure-pack:image/assaultrifle" => #{ item: "adventure-pack:weapon/assaultrifle", name: "Assault Rifle", ammo: "machinerifle", mag: 30, uses: 1, shells: false, damage: 16.0, range: 0.0, head: "kill", taser: false, type: "Advassaultrifle" },
        "adventure-pack:image/automaticrifle" => #{ item: "adventure-pack:weapon/automaticrifle", name: "Automatic Rifle", ammo: "machinerifle", mag: 20, uses: 1, shells: false, damage: 22.0, range: 0.0, head: "kill", taser: false, type: "Advautomaticrifle" },
        "adventure-pack:image/burstrifle" => #{ item: "adventure-pack:weapon/burstrifle", name: "Burst Rifle", ammo: "rifle", mag: 30, uses: 1, shells: false, damage: 20.0, range: 0.0, head: "kill", taser: false, type: "Advburstrifle" },
        "adventure-pack:image/servicerifle" => #{ item: "adventure-pack:weapon/servicerifle", name: "Service Rifle", ammo: "rifle", mag: 15, uses: 1, shells: false, damage: 32.0, range: 0.0, head: "kill", taser: false, type: "Advservicerifle" },
        "adventure-pack:image/submachinegun" => #{ item: "adventure-pack:weapon/submachinegun", name: "Submachine Gun", ammo: "machinepistol", mag: 32, uses: 1, shells: false, damage: 12.0, range: 0.0, head: 2.5, taser: false, type: "Advsubmachinegun" },
        "adventure-pack:image/machinepistol" => #{ item: "adventure-pack:weapon/machinepistol", name: "Machine Pistol", ammo: "machinepistol", mag: 20, uses: 1, shells: false, damage: 10.0, range: 0.0, head: 2.5, taser: false, type: "Advmachinepistol" },
        "adventure-pack:image/revolver" => #{ item: "adventure-pack:weapon/revolver", name: "Revolver", ammo: "revolver", mag: 6, uses: 1, shells: false, damage: 48.0, range: 300.0, head: "kill", taser: false, type: "Advrevolver" },
        "adventure-pack:image/brushpistol" => #{ item: "adventure-pack:weapon/brushpistol", name: "Brush Pistol", ammo: "revolver", mag: 2, uses: 1, shells: false, damage: 40.0, range: 0.0, head: "kill", taser: false, type: "Advbrushpistol" },
        "adventure-pack:image/servicepistol" => #{ item: "adventure-pack:weapon/servicepistol", name: "Service Pistol", ammo: "pistol", mag: 12, uses: 1, shells: false, damage: 20.0, range: 0.0, head: "kill", taser: false, type: "Advservicepistol" },
        "adventure-pack:image/huntingmagnum" => #{ item: "adventure-pack:weapon/huntingmagnum", name: "Hunting Magnum", ammo: "revolver", mag: 5, uses: 1, shells: false, damage: 60.0, range: 0.0, head: "kill", taser: false, type: "Advhuntingmagnum" },
        "adventure-pack:image/automaticpistol" => #{ item: "adventure-pack:weapon/automaticpistol", name: "Automatic Pistol", ammo: "pistol", mag: 18, uses: 1, shells: false, damage: 13.0, range: 0.0, head: 2.5, taser: false, type: "Advautomaticpistol" },
        "adventure-pack:image/doubleshotgun" => #{ item: "adventure-pack:weapon/doubleshotgun", name: "Double Shotgun", ammo: "shotgun", mag: 2, uses: 1, shells: false, damage: 9.0, range: 0.0, head: 2.0, taser: false, type: "Advdoubleshotgun" },
        "adventure-pack:image/pairedshotgun" => #{ item: "adventure-pack:weapon/pairedshotgun", name: "Paired Shotgun", ammo: "shotgun", mag: 2, uses: 2, shells: false, damage: 8.0, range: 0.0, head: 2.0, taser: false, type: "Advpairedshotgun" },
        "adventure-pack:image/singleshotgun" => #{ item: "adventure-pack:weapon/singleshotgun", name: "Single Shotgun", ammo: "shotgun", mag: 1, uses: 1, shells: false, damage: 10.0, range: 0.0, head: 2.0, taser: false, type: "Advsingleshotgun" },
        "adventure-pack:image/levershotgun" => #{ item: "adventure-pack:weapon/levershotgun", name: "Lever Shotgun", ammo: "shotgun", mag: 5, uses: 1, shells: true, damage: 9.0, range: 0.0, head: 2.0, taser: false, type: "Advlevershotgun" },
        "adventure-pack:image/huntingshotgun" => #{ item: "adventure-pack:weapon/huntingshotgun", name: "Hunting Shotgun", ammo: "shotgun", mag: 6, uses: 1, shells: true, damage: 12.0, range: 0.0, head: 2.0, taser: false, type: "Advhuntingshotgun" },
        "adventure-pack:image/automaticshotgun" => #{ item: "adventure-pack:weapon/automaticshotgun", name: "Automatic Shotgun", ammo: "shotgun", mag: 8, uses: 1, shells: false, damage: 7.0, range: 0.0, head: 2.0, taser: false, type: "Advautomaticshotgun" },
        "adventure-pack:image/fieldrifle" => #{ item: "adventure-pack:weapon/fieldrifle", name: "Field Rifle", ammo: "rifle", mag: 5, uses: 1, shells: false, damage: 55.0, range: 0.0, head: "kill", taser: false, type: "Advfieldrifle" },
        "adventure-pack:image/leverrifle" => #{ item: "adventure-pack:weapon/leverrifle", name: "Lever Rifle", ammo: "rifle", mag: 8, uses: 1, shells: true, damage: 38.0, range: 0.0, head: "kill", taser: false, type: "Advleverrifle" },
        "adventure-pack:image/sniperrifle" => #{ item: "adventure-pack:weapon/sniperrifle", name: "Sniper Rifle", ammo: "sniper", mag: 5, uses: 1, shells: false, damage: 95.0, range: 600.0, head: "kill", taser: false, type: "Advsniperrifle" },
        "adventure-pack:image/sniperrepeater" => #{ item: "adventure-pack:weapon/sniperrepeater", name: "Sniper Repeater", ammo: "sniper", mag: 10, uses: 1, shells: false, damage: 50.0, range: 0.0, head: "kill", taser: false, type: "Advsniperrepeater" },
        "adventure-pack:image/huntingrifle" => #{ item: "adventure-pack:weapon/huntingrifle", name: "Hunting Rifle", ammo: "sniper", mag: 4, uses: 1, shells: true, damage: 75.0, range: 0.0, head: "kill", taser: false, type: "Advhuntingrifle" },
        "adventure-pack:image/taser" => #{ item: "adventure-pack:weapon/taser", name: "Taser", ammo: "battery", mag: 1, uses: 1, shells: false, damage: 5.0, range: 24.0, head: 1.0, taser: true, type: "Advtaser" },
        _ => ()
    }
}
// What a hit in the head does for a damage type: "kill", a multiplier, or ().
fn head_of(damage_type) {
    switch damage_type {
        "Advassaultrifle" => "kill",
        "Advautomaticrifle" => "kill",
        "Advburstrifle" => "kill",
        "Advservicerifle" => "kill",
        "Advsubmachinegun" => 2.5,
        "Advmachinepistol" => 2.5,
        "Advrevolver" => "kill",
        "Advbrushpistol" => "kill",
        "Advservicepistol" => "kill",
        "Advhuntingmagnum" => "kill",
        "Advautomaticpistol" => 2.5,
        "Advdoubleshotgun" => 2.0,
        "Advpairedshotgun" => 2.0,
        "Advsingleshotgun" => 2.0,
        "Advlevershotgun" => 2.0,
        "Advhuntingshotgun" => 2.0,
        "Advautomaticshotgun" => 2.0,
        "Advfieldrifle" => "kill",
        "Advleverrifle" => "kill",
        "Advsniperrifle" => "kill",
        "Advsniperrepeater" => "kill",
        "Advhuntingrifle" => "kill",
        "Advtaser" => 1.0,
        _ => ()
    }
}
// A reserve: what a life starts with, the most carried, what one pickup gives.
fn reserve(kind) {
    switch kind {
        "pistol" => #{ start: 36, most: 96, gives: 24 },
        "revolver" => #{ start: 18, most: 48, gives: 12 },
        "machinepistol" => #{ start: 64, most: 192, gives: 40 },
        "rifle" => #{ start: 45, most: 120, gives: 30 },
        "machinerifle" => #{ start: 60, most: 180, gives: 40 },
        "shotgun" => #{ start: 16, most: 48, gives: 10 },
        "sniper" => #{ start: 10, most: 30, gives: 6 },
        _ => ()
    }
}
// END GENERATED

fn image_of(item) {
    let image = item;
    image.replace(":weapon/", ":image/");
    image
}

fn mag_of(p, image, g) {
    let rounds = get_player(p, "mags")[image];
    if rounds == () { g.mag } else { rounds }
}

fn set_mag(p, image, rounds) {
    let mags = get_player(p, "mags");
    mags[image] = rounds;
    set_player(p, "mags", mags);
}

// The taser's battery never runs out.
fn reserve_of(p, kind) {
    if kind == "battery" {
        return 1;
    }
    let left = get_player(p, "reserve")[kind];
    if left == () { reserve(kind).start } else { left }
}

fn set_reserve(p, kind, left) {
    if kind == "battery" {
        return;
    }
    let all = get_player(p, "reserve");
    all[kind] = left;
    set_player(p, "reserve", all);
}

// The ammo panel's line for the gun in hand.
fn show(p, image, g) {
    let rounds = mag_of(p, image, g);
    if g.ammo == "battery" {
        set_player(p, "hud", `${rounds}`);
    } else {
        set_player(p, "hud", `${rounds} | ${reserve_of(p, g.ammo)}`);
    }
}

fn held(p) {
    let me = player(p);
    if me == () || !me.alive {
        return ();
    }
    let g = gun(me.image);
    if g == () {
        return ();
    }
    #{ me: me, image: me.image, g: g }
}

fn cmd_equip(p) {
    let h = held(p);
    if h == () {
        return;
    }
    set_image_ammo(p, mag_of(p, h.image, h.g) > 0);
    show(p, h.image, h.g);
}

fn cmd_fired(p) {
    let h = held(p);
    if h == () {
        return;
    }
    let g = h.g;
    let left = mag_of(p, h.image, g) - g.uses;
    if left < 0 {
        left = 0;
    }
    set_mag(p, h.image, left);
    if g.range > 0.0 {
        trace(p, h.me, g);
    }
    if left <= 0 {
        set_image_ammo(p, false);
    }
    show(p, h.image, g);
}

fn cmd_check(p) {
    let h = held(p);
    if h == () {
        return;
    }
    let rounds = mag_of(p, h.image, h.g);
    let can = rounds < h.g.mag && reserve_of(p, h.g.ammo) > 0;
    set_image_ammo(p, can || rounds > 0);
}

fn cmd_reloaded(p) {
    let h = held(p);
    if h == () {
        return;
    }
    let g = h.g;
    let rounds = mag_of(p, h.image, g);
    let spare = reserve_of(p, g.ammo);
    let take = if g.shells { 1 } else { g.mag - rounds };
    if take > spare {
        take = spare;
    }
    if take < 0 {
        take = 0;
    }
    rounds += take;
    set_mag(p, h.image, rounds);
    set_reserve(p, g.ammo, spare - take);
    if g.shells {
        // On to the next shell (ammo off) until full or out of shells.
        let done = rounds >= g.mag || spare - take <= 0;
        set_image_ammo(p, done && rounds > 0);
    } else {
        set_image_ammo(p, rounds > 0);
    }
    show(p, h.image, g);
}

fn cmd_interrupt(p) {
    let h = held(p);
    if h == () {
        return;
    }
    set_image_ammo(p, mag_of(p, h.image, h.g) > 0);
}

fn cmd_reload(p) {
    let h = held(p);
    if h == () {
        return;
    }
    if mag_of(p, h.image, h.g) < h.g.mag && reserve_of(p, h.g.ammo) > 0 {
        // The image leaves Ready for Check, which sends it to Reload.
        set_image_ammo(p, false);
    }
}

// A hitscan shot: the first thing on the line from the eye, a tracer from
// the muzzle, damage under the minigame's rules; the taser tumbles players.
fn trace(p, me, g) {
    let eye = [me.ex, me.ey, me.ez];
    let look = [me.lx, me.ly, me.lz];
    let hit = raycast(eye, look, g.range, p);
    let end = if hit == () {
        [me.ex + me.lx * g.range, me.ey + me.ly * g.range, me.ez + me.lz * g.range]
    } else {
        [hit.x, hit.y, hit.z]
    };
    let colour = if g.taser { [0.55, 0.8, 1.0] } else { [1.0, 0.85, 0.55] };
    beam([me.mx, me.my, me.mz], end, #{ color: colour, width: 0.035, seconds: 0.08, muzzle: p });
    if hit == () || hit.ref == () || !can_damage(p, hit.ref) {
        return;
    }
    let amount = g.damage;
    if hit.kind == "player" && hit.region == "head" {
        amount = headshot(p, hit.id, g.head, amount);
    }
    if g.taser && hit.kind == "player" {
        tumble(hit.id, me.lx * 6.0, 4.0, me.lz * 6.0, p);
    }
    damage(hit.ref, amount, p, g.type);
}

fn headshot(p, victim, head, amount) {
    if head == () || head == 1.0 {
        return amount;
    }
    if p != () && p != victim {
        play_sound(p, "adventure-pack:headshot");
    }
    if head == "kill" { 1000.0 } else { amount * head }
}

// Rounds hit where they strike: the engine says which part of the body.
fn on_damage(victim, attacker, amount, info) {
    if info.kind != "weapon" || info.region != "head" {
        return ();
    }
    let head = head_of(info.type);
    if head == () {
        return ();
    }
    headshot(attacker, victim, head, amount)
}

fn on_spawn(p) {
    set_player(p, "mags", #{});
    set_player(p, "reserve", #{});
    set_player(p, "hud", "");
}

// Adds to a reserve up to its most; false when it is full already.
fn add_ammo(p, kind, amount) {
    let r = reserve(kind);
    if r == () {
        return false;
    }
    let left = reserve_of(p, kind);
    if left >= r.most {
        return false;
    }
    let more = left + amount;
    if more > r.most {
        more = r.most;
    }
    set_reserve(p, kind, more);
    let h = held(p);
    if h != () && h.g.ammo == kind {
        show(p, h.image, h.g);
        if h.me.image_state == "Empty" {
            // An empty gun reloads from what was just picked up.
            set_image_ammo(p, true);
        }
    }
    true
}

// The kinds of ammo the guns a player carries take.
fn carried(p) {
    let kinds = [];
    for item in player(p).tools {
        if item != "" {
            let g = gun(image_of(item));
            if g != () && !(g.ammo in kinds) {
                kinds.push(g.ammo);
            }
        }
    }
    kinds
}

// Ammo boxes and spare guns. A box feeds a gun the player carries (double
// what a spare gun gives); the pile feeds every kind they carry; a gun they
// already carry gives its ammo. Otherwise it stays for someone else.
fn on_pickup(p, item, info) {
    let box = "adventure-pack:weapon/ammo";
    if item.starts_with(box) {
        let kind = item.sub_string(box.len());
        let kinds = carried(p);
        let gave = false;
        if kind == "pile" {
            for k in kinds {
                if add_ammo(p, k, reserve(k).gives) {
                    gave = true;
                }
            }
        } else if kind in kinds {
            gave = add_ammo(p, kind, reserve(kind).gives * 2);
        }
        if gave {
            play_sound(p, "adventure-pack:ammo");
            return "take";
        }
        return false;
    }
    let image = image_of(item);
    let g = gun(image);
    if g == () {
        return ();
    }
    if item in player(p).tools {
        if g.ammo != "battery" && add_ammo(p, g.ammo, reserve(g.ammo).gives) {
            play_sound(p, "adventure-pack:ammo");
            return "take";
        }
        return false;
    }
    // A new gun comes with the magazine it was dropped with, or full.
    let rounds = if info.data == () { g.mag } else { info.data.rounds };
    set_mag(p, image, rounds);
    ()
}

// A dropped gun keeps its magazine.
fn on_drop(p, item, slot) {
    let image = image_of(item);
    let g = gun(image);
    if g == () {
        return ();
    }
    let rounds = mag_of(p, image, g);
    let mags = get_player(p, "mags");
    mags.remove(image);
    set_player(p, "mags", mags);
    #{ rounds: rounds }
}
