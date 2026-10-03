// Rotondo's pinned Bot_Shark policy. The engine supplies actor mechanics;
// this companion chooses appearance, buoyancy, contact capture and release.
fn shark(p) { p != () && p.model == "{{namespace}}:asset/shark.dts" }
fn owned_bot(p) { p != () && p.bot && p.bot_kind.starts_with("{{namespace}}:") }
fn owned_shark(p) { shark(p) && owned_bot(p) }
fn carries(id) {
    for p in players() { if p.mount == id { return true; } }
    for p in bots() { if p.mount == id { return true; } }
    false
}
fn roll(who, salt, count) { (hash3(seed(), who, tick(), salt) * count.to_float()).floor().to_int() % count }
fn vector(text) {
    let out = [];
    let trimmed = text;
    trimmed.trim();
    for n in trimmed.split(" ") { if n != "" { out.push(parse_float(n)); } }
    out
}
fn palette() {
    let lines = {{palette|text}}.split("\"");
    let colors = [];
    let i = 1;
    while i < lines.len() { colors.push(vector(lines[i])); i += 2; }
    colors
}
fn initialize(p) {
    let colors = palette();
    let choice = roll(p.id, 11, colors.len());
    let cool = p.name == "Cool Shark" || roll(p.id, 19, 51) == 0;
    let color = colors[choice];
    let fin = vector({{fins|text}});
    let paint = #{head: color, torso: color, hip: color, rarm: fin, larm: fin,
                  rhand: color, lhand: color, rleg: color, lleg: color};
    if cool { paint.hat = [0.2, 0.2, 0.2, 1.0]; paint.accent = paint.hat; }
    set_avatar_colors(p.id, paint);
    set_avatar_parts(p.id, #{hat: if cool { "helmet" } else { "none" },
                             accent: if cool { "visor" } else { "none" }});
    // The verified source's five-color palette assigns grey and navy to
    // its helper datablocks. The small importer stand-in has no helpers.
    if colors.len() == 5 {
        let kind = if choice == 1 { "sharkholebottop" }
                   else if choice == 2 { "sharkholebotbottom" } else { "sharkholebot" };
        set_archetype(p.id, "{{namespace}}:archetype/" + kind);
    }
    #{white: choice == 0, cool: cool}
}
fn release_grab(id, interrupted) {
    let grabs = get("grabs");
    let key = `${id}`;
    if !(key in grabs) { return; }
    let grab = grabs[key];
    grabs.remove(key);
    set("grabs", grabs);
    let victim = player(grab.victim);
    if victim != () {
        if victim.mount == id { unmount_object(victim.id); }
        orbit_camera(victim.id, ());
        if victim.alive { play_thread(victim.id, 0, "root"); }
    }
    let holder = player(id);
    if owned_bot(holder) {
        // The source delays startHoleLoop by 2000ms after a harm release.
        // Keep the existing resting adaptation through that exact interval.
        if interrupted && holder.alive {
            let restarts = get("restarts");
            restarts[key] = #{at: tick() + 240, game: grab.game, victim: grab.victim};
            set("restarts", restarts);
        } else { rest_bot(id, false); }
        if holder.alive { play_thread(id, 1, "root"); }
    }
}
fn cancel_restart(id) {
    let restarts = get("restarts");
    let key = `${id}`;
    if !(key in restarts) { return; }
    restarts.remove(key);
    set("restarts", restarts);
    let holder = player(id);
    if owned_bot(holder) { rest_bot(id, false); }
}
fn release_actor(id) {
    cancel_restart(id);
    let restarts = get("restarts");
    for key in restarts.keys() {
        if restarts[key].victim == id { cancel_restart(parse_int(key)); }
    }
    let grabs = get("grabs");
    for key in grabs.keys() {
        let grab = grabs[key];
        if grab.holder == id || grab.victim == id { release_grab(grab.holder, false); }
    }
}
fn on_spawn(id) {
    release_actor(id);
    let known = get("known");
    known.remove(`${id}`);
    set("known", known);
}
fn on_death(id, killer) { on_spawn(id); }
fn on_leave(id) { on_spawn(id); }
fn on_minigame(event) {
    if event.kind in ["reset", "loaded"] {
        let known = get("known");
        for p in bots() { if p.minigame == event.game { known.remove(`${p.id}`); } }
        set("known", known);
    }
    if event.kind in ["reset", "ended", "round_end", "configured", "loaded", "teams"] {
        let restarts = get("restarts");
        for key in restarts.keys() {
            if restarts[key].game == event.game { cancel_restart(parse_int(key)); }
        }
        let grabs = get("grabs");
        for key in grabs.keys() {
            if grabs[key].game == event.game { release_grab(grabs[key].holder, false); }
        }
    } else if event.kind in ["joined", "left", "team", "kicked"] && event.player != () {
        release_actor(event.player);
    }
}
fn on_brick(event, id, by, info) {
    if event in ["planted", "loaded"] { set_brick_shown(id, false, false, false); }
}
fn on_damage(victim_id, attacker, amount, info) {
    let grabs = get("grabs");
    let victim = player(victim_id);
    let held_key = `${victim_id}`;
    if held_key in grabs {
        let grab = grabs[held_key];
        // Source's 200ms grace prevents the captured victim's contact from
        // immediately cancelling the bite. Damage beyond it may free them.
        if tick() - grab.at < 24 { return 0.0; }
        if amount >= 50.0 || roll(victim_id, 31, 4) == 0 { release_grab(victim_id, true); }
    }
    if attacker == () || info.kind != "weapon" || info.type != "Bite" { return (); }
    let holder = player(attacker);
    if !owned_shark(holder) || !holder.bot || !holder.alive || victim == () || !victim.alive || holder.mounted {
        return ();
    }
    // v20 miniGameCanDamage returns -1 outside a mini-game; the source
    // collision policy refuses that case rather than creating a sandbox bite.
    if holder.minigame == () { return 0.0; }
    let restarts = get("restarts");
    if `${attacker}` in restarts { return 0.0; }
    if `${attacker}` in grabs { return 0.0; }
    if victim.mounted || victim.minigame != holder.minigame || !can_damage(attacker, victim_id) {
        return ();
    }
    // The ordinary mount operation rejects occupied mounts and riders who
    // already carry someone. Observe those relationships before queuing it.
    if carries(attacker) || carries(victim_id) { return (); }
    let human_body = victim.model == "v20.shape.m" || victim.model == "base/data/shapes/player/m.dts";
    // The source grabs on body contact. Its native body is 3.5 units
    // across after the engine quarters the raw PlayerData boundingBox.
    // Do not use that margin as permission for a remote capture.
    let victim_radius = if shark(victim) { 1.75 * victim.scale }
                        else if human_body { 0.625 * victim.scale } else { 0.0 };
    let victim_height = (victim.cy - victim.y) * 2.0;
    let dx = holder.x - victim.x;
    let dz = holder.z - victim.z;
    let contact = 1.75 * holder.scale + victim_radius + 0.1;
    if dx.abs() > contact || dz.abs() > contact ||
       holder.y > victim.y + victim_height + 0.1 ||
       victim.y > holder.y + (holder.cy - holder.y) * 2.0 + 0.1 { return 0.0; }
    let node = if human_body && holder.scale + 0.5 >= victim.scale { 2 }
               else if shark(victim) && holder.scale >= victim.scale + 0.5 { 3 } else { -1 };
    if node < 0 { return (); }
    let known = get("known");
    let white = `${attacker}` in known && known[`${attacker}`].white;
    if !white && roll(attacker, 23, 3) != 0 { return (); }
    if grabs.len() >= 16 { return (); }
    mount_object(attacker, victim_id, node, false, 0.0);
    if !victim.bot { orbit_camera(victim_id, attacker, 1.0, 10.0, 10.0); }
    rest_bot(attacker, true);
    if node == 3 { play_thread(attacker, 1, "biteReady"); play_thread(victim_id, 0, "biteFix"); }
    grabs[`${attacker}`] = #{holder: attacker, victim: victim_id, game: holder.minigame, at: tick()};
    set("grabs", grabs);
    0.0
}
fn on_tick() {
    // At most the engine's 16 bots. Gone/dead identities leave the bounded
    // initialization map; brick bots are intentionally not on_spawn clients.
    let known = get("known");
    let live = #{};
    for p in bots() {
        if owned_shark(p) && p.alive {
            let key = `${p.id}`;
            live[key] = if key in known { known[key] } else { initialize(p) };
        }
    }
    set("known", live);
    let restarts = get("restarts");
    for key in restarts.keys() {
        let holder = player(parse_int(key));
        let victim = player(restarts[key].victim);
        if !owned_shark(holder) || !holder.alive || holder.minigame != restarts[key].game ||
           victim == () || !victim.alive || victim.minigame != restarts[key].game ||
           !can_damage(holder.id, victim.id) ||
           tick() >= restarts[key].at { cancel_restart(parse_int(key)); }
    }
    let grabs = get("grabs");
    for key in grabs.keys() {
        let grab = grabs[key];
        let holder = player(grab.holder);
        let victim = player(grab.victim);
        if !owned_shark(holder) || !holder.alive || victim == () || !victim.alive ||
           victim.mount != grab.holder || holder.minigame != grab.game ||
           victim.minigame != grab.game || !can_damage(grab.holder, grab.victim) {
            release_grab(grab.holder, false);
        } else if tick() - grab.at >= 600 {
            // Permission and actual mount were observed just above. Ordinary
            // attributed damage retains score/death policy and cannot recurse.
            // Release first: ordinary lethal damage itself clears mounts,
            // so a later unmount would target an already-cleared relationship.
            release_grab(grab.holder, false);
            damage(grab.victim, 1000.0, grab.holder);
        }
    }
}
