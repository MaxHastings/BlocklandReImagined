// Trench Warfare: Red against Blue on a field of dirt.
//
// The design comes from Blockland's Trench Digging, lilboarder32's remake
// of an old v8 mod, and Platypi's Trench Digging Plus, and from the Trench
// Wars servers that played them: dig with a pick, carry the dirt in a
// bag of limited size, pile it back up as cover, and fight across the
// trenches. This is our own implementation; none of their code or art
// is used.
//
// - Left click with the Trench Pick digs out the cube of dirt you aim at
//   into your bag. Right click piles one back onto the face you aim at.
//   A box outlines the cube the pick will dig.
// - Each round opens with a ceasefire to dig in. Then the shooting
//   starts; the first team to 20 kills wins the round, or the team ahead
//   when the clock runs out. The field keeps its trenches from round to
//   round.
// - The bases at each end are out of bounds for digging: sandbags guard
//   the spawn.
//
// The field is voxels of the generated world (field.json): 2 units a
// side, drawn as 4x cubes. x runs from -40 to 39 between the bases, z
// from -20 to 19; Red holds the low x end and Blue the high one, and the
// two halves mirror each other so neither side has better ground.

// ---- Tuning ----

fn pick() { "trench-kit:weapon/pick" }
// How far the pick reaches from the eye, in units.
fn reach() { 6.0 }
// The most dirt a bag holds, in cubes.
fn bag() { 50 }
// Seconds of ceasefire to dig in, of fighting, and between rounds.
fn dig_seconds() { 45 }
fn fight_seconds() { 480 }
fn over_seconds() { 10 }
// Kills that win a round.
fn target() { 20 }

// ---- The field ----

fn half_x() { 40 }
fn half_z() { 20 }
// The grass layer's height, in voxels (bedrock is layer 0).
fn ground() { 6 }
// |x| from here out is a team's base: sandbags at the line, no digging.
fn base_line() { 34 }
// Dirt piles no higher than the rim around the field.
fn ceiling() { 9 }

// Materials, as field.json lists them.
fn grass() { 0 }
fn dirt() { 1 }
fn clay() { 2 }
fn bedrock() { 3 }
fn rock() { 4 }
fn sandbag() { 5 }
fn dirt_id() { "trench:material/dirt" }

fn diggable(material) {
    material == "trench:material/grass"
        || material == "trench:material/dirt"
        || material == "trench:material/clay"
}

// The field is mirrored across x = -0.5: column x on Blue's side is
// column -x-1 on Red's.
fn mirrored(x) {
    if x < 0 { -x - 1 } else { x }
}

fn in_base(x) {
    mirrored(x) >= base_line()
}

fn inside(x, z) {
    x >= -half_x() && x < half_x() && z >= -half_z() && z < half_z()
}

// Integer division and remainder that round toward minus infinity, so the
// pattern carries on unbroken across zero.
fn floor_div(a, b) {
    let q = a / b;
    if a % b != 0 && (a < 0) != (b < 0) { q - 1 } else { q }
}

fn modulo(a, b) {
    ((a % b) + b) % b
}

// Shell holes: one per 9x9 block of no-man's-land, most of the time.
fn crater_depth(s, m, z) {
    let deepest = 0.0;
    let bx = floor_div(m, 9);
    let bz = floor_div(z, 9);
    for dx in -1..2 {
        for dz in -1..2 {
            let cx = bx + dx;
            let cz = bz + dz;
            if hash3(s, cx, 7, cz) > 0.65 { continue; }
            let px = cx * 9 + (hash3(s, cx, 8, cz) * 9.0).floor().to_int();
            let pz = cz * 9 + (hash3(s, cx, 9, cz) * 9.0).floor().to_int();
            let radius = 1.8 + hash3(s, cx, 10, cz) * 1.8;
            let ddx = (m - px).to_float();
            let ddz = (z - pz).to_float();
            let d = (ddx * ddx + ddz * ddz).sqrt();
            if d < radius {
                let depth = (1.0 - d / radius) * 2.6;
                if depth > deepest { deepest = depth; }
            }
        }
    }
    deepest
}

// The top voxel's height of column (x, z).
fn surface(s, x, z) {
    let m = mirrored(x);
    if m >= base_line() - 2 {
        return ground();
    }
    // Rolling ground, flattening out toward the bases.
    let fade = 1.0;
    if m > base_line() - 8 {
        fade = (base_line() - 2 - m).to_float() / 6.0;
    }
    let roll = noise(s, m.to_float() * 0.12, z.to_float() * 0.12) * 1.3 * fade;
    let h = ground().to_float() + roll - crater_depth(s, m, z) * fade;
    let top = h.round().to_int();
    if top < ground() - 3 { top = ground() - 3; }
    if top > ground() + 1 { top = ground() + 1; }
    top
}

// The sandbag line at each base: gaps every few cubes to go through.
fn parapet(x, z) {
    mirrored(x) == base_line() && modulo(z + half_z(), 7) >= 2
}

fn generate_chunk(cx, cz) {
    let s = seed();
    let out = [];
    for lx in 0..8 {
        for lz in 0..8 {
            let x = cx * 8 + lx;
            let z = cz * 8 + lz;
            let m = mirrored(x);
            if m > half_x() || z < -half_z() - 1 || z > half_z() {
                continue;
            }
            if !inside(x, z) {
                // The rim: a rock wall around the field.
                for y in 0..=ceiling() {
                    out.push([x, y, z, rock()]);
                }
                continue;
            }
            let top = surface(s, x, z);
            for y in 0..=top {
                let material = if y == 0 {
                    bedrock()
                } else if y == top {
                    grass()
                } else if y <= 2 {
                    clay()
                } else {
                    dirt()
                };
                out.push([x, y, z, material]);
            }
            if parapet(x, z) {
                out.push([x, top + 1, z, sandbag()]);
            }
        }
    }
    out
}

// ---- Teams ----

fn red() { "Red" }
fn blue() { "Blue" }

fn team_of(p) {
    get_player(p, "team")
}

fn count(team) {
    let n = 0;
    for p in players() {
        if get_player(p.id, "team") == team { n += 1; }
    }
    n
}

// The smaller team, or the one behind, or Red.
fn pick_team() {
    let r = count(red());
    let b = count(blue());
    if r < b { return red(); }
    if b < r { return blue(); }
    if get("blue") < get("red") { return blue(); }
    red()
}

fn uniform(p, team) {
    let coat = if team == red() { [0.72, 0.12, 0.1] } else { [0.12, 0.27, 0.72] };
    let trousers = if team == red() { [0.34, 0.08, 0.07] } else { [0.08, 0.13, 0.33] };
    set_avatar_colors(p, #{
        torso: coat, larm: coat, rarm: coat,
        hip: trousers, lleg: trousers, rleg: trousers
    });
}

fn join_team(p) {
    let team = pick_team();
    set_player(p, "team", team);
    uniform(p, team);
    team
}

fn ensure_team(p) {
    let team = team_of(p);
    if team == "" { team = join_team(p); }
    team
}

fn on_join(p) {
    let team = join_team(p);
    tell(p, `Trench Warfare: you are on ${team}. Left click digs dirt into your bag, right click piles it back up. Dig in before the ceasefire ends.`);
}

// Spawn behind your own sandbags, somewhere along the line.
fn on_spawn(p) {
    let team = ensure_team(p);
    let s = seed();
    let lane = (hash3(s, tick(), p, 1) * 17.0).floor().to_int() - 8;
    let depth = (hash3(s, tick(), p, 2) * 3.0).floor().to_int();
    let column = base_line() + 2 + depth;
    let x = if team == red() { -column - 1 } else { column };
    let feet = (ground() + 1).to_float() * 2.0 + 0.05;
    teleport(p, x.to_float() * 2.0 + 1.0, feet, lane.to_float() * 2.0 + 1.0);
    let phase = get("phase");
    if phase == "dig" {
        center_print(p, `<color:ffd060>Dig in!<color:ffffff>\nLeft click digs, right click piles dirt`, 3.0);
    }
}

fn cmd_teams(p) {
    tell(p, `Red: ${count(red())} players, ${get("red")} kills.  Blue: ${count(blue())} players, ${get("blue")} kills.`);
}

// ---- Digging ----

// The voxel the player's pick meets, with the face it meets: #{ v, hit },
// or () when they aim at nothing of the field within reach.
fn aimed(me) {
    let hit = raycast([me.ex, me.ey, me.ez], [me.lx, me.ly, me.lz], reach(), me.id);
    if hit == () || hit.kind != "brick" { return (); }
    let v = voxel(hit.id);
    if v == () { return (); }
    #{ v: v, hit: hit }
}

fn cmd_dig(p) {
    let me = player(p);
    if me == () || !me.alive { return; }
    let a = aimed(me);
    if a == () { return; }
    let v = a.v;
    if !diggable(v.material) {
        bottom_print(p, "Too hard to dig", 1.5);
        return;
    }
    if in_base(v.x) {
        bottom_print(p, "No digging at the bases", 1.5);
        return;
    }
    let carried = get_player(p, "dirt");
    if carried >= bag() {
        bottom_print(p, `Your bag is full (${bag()}). Right click to pile dirt up.`, 2.0);
        return;
    }
    remove_brick(a.hit.id);
    set_player(p, "dirt", carried + 1);
    set_player(p, "aim", "");
}

// The axis the hit face faces along: the voxel next to it on that side.
fn beside(v, hit) {
    let nx = hit.nx;
    let ny = hit.ny;
    let nz = hit.nz;
    let ax = nx.abs();
    let ay = ny.abs();
    let az = nz.abs();
    if ay >= ax && ay >= az {
        return [v.x, v.y + (if ny > 0.0 { 1 } else { -1 }), v.z];
    }
    if ax >= az {
        return [v.x + (if nx > 0.0 { 1 } else { -1 }), v.y, v.z];
    }
    [v.x, v.y, v.z + (if nz > 0.0 { 1 } else { -1 })]
}

fn cmd_place(p) {
    let me = player(p);
    if me == () || !me.alive { return; }
    let a = aimed(me);
    if a == () { return; }
    let carried = get_player(p, "dirt");
    if carried <= 0 {
        bottom_print(p, "Your bag is empty. Left click to dig some dirt.", 2.0);
        return;
    }
    let c = beside(a.v, a.hit);
    if !inside(c[0], c[2]) || c[1] < 1 { return; }
    if in_base(c[0]) {
        bottom_print(p, "No building at the bases", 1.5);
        return;
    }
    if c[1] > ceiling() {
        bottom_print(p, "That's as high as dirt piles", 1.5);
        return;
    }
    if !can_place_voxel(c[0], c[1], c[2]) {
        bottom_print(p, "Something is in the way", 1.5);
        return;
    }
    place_voxel(c[0], c[1], c[2], dirt_id());
    set_player(p, "dirt", carried - 1);
    set_player(p, "aim", "");
    sound_at("trench-kit:place", c[0].to_float() * 2.0 + 1.0, c[1].to_float() * 2.0 + 1.0, c[2].to_float() * 2.0 + 1.0);
}

// /givedirt 10 Name: hand dirt to a teammate.
fn cmd_givedirt(p, amount, name) {
    if amount <= 0 { return; }
    let carried = get_player(p, "dirt");
    let team = team_of(p);
    let wanted = name.to_lower();
    for other in players() {
        if other.id == p || !other.name.to_lower().starts_with(wanted) { continue; }
        if get_player(other.id, "team") != team {
            tell(p, `${other.name} is not on your team.`);
            return;
        }
        let room = bag() - get_player(other.id, "dirt");
        let given = amount;
        if given > carried { given = carried; }
        if given > room { given = room; }
        if given <= 0 {
            tell(p, if carried <= 0 { "You have no dirt to give." } else { `${other.name}'s bag is full.` });
            return;
        }
        set_player(p, "dirt", carried - given);
        add_player(other.id, "dirt", given);
        tell(p, `You gave ${given} dirt to ${other.name}.`);
        tell(other.id, `${me_name(p)} gave you ${given} dirt.`);
        return;
    }
    tell(p, `Nobody here is called ${name}.`);
}

fn me_name(p) {
    let me = player(p);
    if me == () { "Someone" } else { me.name }
}

// The cube the pick would dig, outlined while it is in hand. Sent only
// when it changes.
fn preview() {
    let rays = 0;
    for me in players() {
        if !me.alive || me.item != pick() { continue; }
        if rays >= 60 { break; }
        rays += 1;
        let key = "";
        let a = aimed(me);
        if a != () && diggable(a.v.material) && !in_base(a.v.x) {
            key = `${a.v.x} ${a.v.y} ${a.v.z}`;
        }
        if key == get_player(me.id, "aim") { continue; }
        set_player(me.id, "aim", key);
        if key == "" {
            hide_box(me.id);
        } else {
            let v = a.v;
            let low = [v.x.to_float() * 2.0, v.y.to_float() * 2.0, v.z.to_float() * 2.0];
            show_box(me.id, low, [low[0] + 2.0, low[1] + 2.0, low[2] + 2.0], pick());
        }
    }
}

// Anyone who got out of the field (over the rim, under the map) comes
// back at their base.
fn keep_in_field() {
    for me in players() {
        if !me.alive { continue; }
        let x = (me.x / 2.0).floor().to_int();
        let z = (me.z / 2.0).floor().to_int();
        if !inside(x, z) || me.y < 1.0 {
            respawn(me.id);
        }
    }
}

// ---- Rounds ----

fn clock(seconds) {
    let m = seconds / 60;
    let s = seconds % 60;
    if s < 10 { `${m}:0${s}` } else { `${m}:${s}` }
}

fn start_fight() {
    set("phase", "fight");
    set("left", fight_seconds());
    center_print((), "<color:ff6040>Over the top!<color:ffffff>\nThe ceasefire is over", 3.0);
    for p in players() {
        play_sound(p.id, "trench-kit:whistle");
    }
}

fn end_round() {
    let r = get("red");
    let b = get("blue");
    let text = if r > b {
        set("red_rounds", get("red_rounds") + 1);
        "<color:ff5040>Red<color:ffffff> wins the round!"
    } else if b > r {
        set("blue_rounds", get("blue_rounds") + 1);
        "<color:5080ff>Blue<color:ffffff> wins the round!"
    } else {
        "The round is a draw."
    };
    center_print((), `${text}\nRed ${r}, Blue ${b}`, 5.0);
    broadcast(`Round ${get("round")}: Red ${r}, Blue ${b}.`);
    set("phase", "over");
    set("left", over_seconds());
    set("status", "Round over");
}

fn new_round() {
    set("round", get("round") + 1);
    set("red", 0);
    set("blue", 0);
    set("phase", "dig");
    set("left", dig_seconds());
    for p in players() {
        respawn(p.id);
    }
}

fn second() {
    let phase = get("phase");
    let left = get("left") - 1;
    set("left", left);
    if phase == "dig" {
        if left <= 0 {
            start_fight();
            set("status", `Fight!  ${clock(fight_seconds())}`);
            return;
        }
        set("status", `Ceasefire  ${left} s`);
        if left == 10 {
            center_print((), "10 seconds until the shooting starts", 2.0);
        }
    } else if phase == "fight" {
        if left <= 0 {
            end_round();
            return;
        }
        set("status", `Fight!  ${clock(left)}`);
    } else if left <= 0 {
        new_round();
        set("status", `Ceasefire  ${dig_seconds()} s`);
    }
}

fn cmd_newround(p) {
    broadcast(`${me_name(p)} started a new round.`);
    new_round();
    set("status", `Ceasefire  ${dig_seconds()} s`);
}

fn on_tick() {
    if tick() % 120 == 0 {
        second();
        keep_in_field();
    }
    preview();
}

// No friendly fire, and nobody hurts anybody during the ceasefire or
// between rounds. Falls and the like still hurt.
fn on_damage(victim, attacker, amount, info) {
    if attacker == () || attacker == victim { return (); }
    if get("phase") != "fight" { return 0; }
    let team = team_of(victim);
    if team != "" && team == team_of(attacker) { return 0; }
    ()
}

fn on_death(victim, killer) {
    if get("phase") != "fight" || killer == () || killer == victim { return; }
    let team = team_of(killer);
    if team == "" || team == team_of(victim) { return; }
    let key = if team == red() { "red" } else { "blue" };
    let kills = get(key) + 1;
    set(key, kills);
    if kills >= target() {
        end_round();
    }
}
