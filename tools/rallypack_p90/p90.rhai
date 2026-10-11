// Native host policy derived from Maxwell's Rallypack/P90 source.
// Original assets remain in the separate shared data Add-On.
fn fresh(p) {
    set_player(p, "rounds", 50);
    set_player(p, "spares", [50, 50]);
    set_player(p, "gun", #{ last: -1000, release: -1000, shot: 0, climb: 0.0,
        bound: 0.0, aiming: false, reloading: false, sounding: false, tail: -1,
        reload_start: 0, reload_end: 0, distant: [] });
}
fn state(p) {
    let s = get_player(p, "gun");
    if !("last" in s) { fresh(p); s = get_player(p, "gun"); }
    s
}
fn owns(image) { image.starts_with("weapon_rallypack_p90:image/r_p90") }
fn show(p) {
    let mags = get_player(p, "spares");
    let n = get_player(p, "rounds");
    bottom_print(p, `P90: ${n}/50   Spare magazines: ${mags}`, 30.0);
}
fn on_loadout(p) { fresh(p); }
fn on_death(victim, killer) {
    let s = state(victim);
    s.reloading = false;
    s.aiming = false;
    s.sounding = false;
    s.tail = -1;
    s.distant = [];
    set_player(victim, "gun", s);
    set_speed_scale(victim, 1.0);
    set_fov(victim, ());
}
fn cmd_mount(p) {
    let me = player(p);
    if me == () || !owns(me.image) { return; }
    let s = state(p);
    s.aiming = me.image.ends_with("r_p90aimingimage");
    if !me.image.ends_with("r_p90reloadingimage") { s.reloading = false; }
    set_player(p, "gun", s);
    set_speed_scale(p, if s.aiming { 0.6 } else { 1.0 });
    set_fov(p, if s.aiming { 75.0 } else { () });
    set_image_ammo(p, get_player(p, "rounds") > 0);
    show(p);
}
fn stop(p) {
    let s = state(p);
    if s.sounding { s.tail = tick() + 6; s.sounding = false; }
    s.release = tick();
    set_player(p, "gun", s);
}
fn cmd_unmount(p) {
    let me = player(p);
    // Image swaps use the same weapon; only an actual put-away cancels it.
    if me != () && owns(me.image) { return; }
    stop(p);
    let s = state(p);
    s.reloading = false;
    s.aiming = false;
    set_player(p, "gun", s);
    set_speed_scale(p, 1.0);
    set_fov(p, ());
}
fn cmd_ready(p) { stop(p); }
fn cmd_check(p) {
    let me = player(p);
    if me == () || !owns(me.image) { return; }
    let n = get_player(p, "rounds");
    set_image_ammo(p, n > 0);
    if n <= 0 && !me.image.ends_with("r_p90reloadingimage") { cmd_reload(p); }
}
fn cmd_reload(p) {
    let me = player(p);
    if me == () || !owns(me.image) { return; }
    let s = state(p);
    if s.reloading { return; }
    let largest = 0;
    for n in get_player(p, "spares") { if n > largest { largest = n; } }
    let n = get_player(p, "rounds");
    // Rallypack accepts equal-size spares when a chambered round can be kept.
    if largest <= 0 || largest < n { show(p); return; }
    stop(p);
    s = state(p);
    s.reloading = true;
    s.reload_start = tick();
    s.reload_end = tick() + if n > 0 { 216 } else { 258 };
    s.aiming = false;
    set_player(p, "gun", s);
    mount_image(p, "weapon_rallypack_p90:image/r_p90reloadingimage");
    set_image_ammo(p, n > 0);
    set_speed_scale(p, 1.0);
    set_fov(p, ());
}
fn cmd_finish(p) {
    let s = state(p);
    if !s.reloading { return; }
    let mags = get_player(p, "spares");
    let best = -1;
    let largest = 0;
    for i in 0..mags.len() { if mags[i] > largest { largest = mags[i]; best = i; } }
    if best >= 0 {
        mags.remove(best);
        let old = get_player(p, "rounds");
        if old > 0 {
            largest += 1;
            if old > 1 { mags.push(old - 1); }
        }
        set_player(p, "spares", mags);
        set_player(p, "rounds", largest);
    }
    s.reloading = false;
    set_player(p, "gun", s);
    mount_image(p, "weapon_rallypack_p90:image/r_p90image");
    set_image_ammo(p, largest > 0);
    show(p);
}
fn cmd_dry(p) {
    stop(p);
    let me = player(p);
    if me != () {
        sound_at("weapon_rallypack_p90:sound/r_submachinereload3sound", me.x, me.y, me.z);
        play_thread(p, 3, "plant");
    }
    show(p);
}
fn cmd_aim(p) {
    let me = player(p);
    if me == () || !owns(me.image) { return; }
    let s = state(p);
    if s.reloading { return; }
    s.aiming = !s.aiming;
    set_player(p, "gun", s);
    mount_image(p, if s.aiming { "weapon_rallypack_p90:image/r_p90aimingimage" }
                  else { "weapon_rallypack_p90:image/r_p90image" });
    set_speed_scale(p, if s.aiming { 0.6 } else { 1.0 });
    set_fov(p, if s.aiming { 75.0 } else { () });
}
fn cmd_shot(p) {
    let me = player(p);
    if me == () || !owns(me.image) { return; }
    let s = state(p);
    let n = get_player(p, "rounds");
    if n <= 0 || s.reloading { return; }
    if tick() - s.last < 31 || tick() - s.release < 24 { s.shot += 1; }
    else { s.shot = 1; s.climb = 0.0; s.bound = 0.0; }
    let cm = if s.aiming { 0.08 } else { 0.85 };
    let bm = if s.aiming { 0.06 } else { 0.6 };
    if me.crouched { bm *= 0.25; }
    // Torque cross(look, world down) in native x-right/y-up/-z-forward axes.
    let rx = me.lz;
    let rz = -me.lx;
    let dx = me.lx + rx * s.bound;
    let dy = me.ly + s.climb;
    let dz = me.lz + rz * s.bound;
    if s.shot > 3 {
        let pattern = [4, 3, 3, 9, 8, 7]; // authored spraySeed 433987
        let digit = pattern[s.shot % 6];
        let hs = [5.0,-4.0,7.0,9.0,0.0,1.0,-6.0,2.0,8.0,-2.0];
        let vs = [3.0,-2.0,6.0,-1.0,4.0,6.0,-5.0,9.0,-3.0,5.0];
        // Pattern uses the image's authored modifiers, before crouch damping.
        let authored_bm = if s.aiming { 0.06 } else { 0.6 };
        let m = (cm + authored_bm) * 0.5 * 3.5 * 0.002 * (1.0 - s.shot.to_float() / 50.0);
        dx += rx * hs[digit] * m;
        dz += rz * hs[digit] * m;
        dy += vs[digit] * m;
    }
    s.climb = if s.shot >= 3 { (s.shot - 2).to_float().ln() * cm * 0.1 } else { 0.0 };
    s.bound = if s.shot >= 3 { bm / (s.shot - 1).to_float() * (3.141592653589793 * 0.1 * (s.shot - 3).to_float()).sin() }
              else { 0.0 };
    // The original deliberately uses the unnormalized recoil-adjusted vector.
    fire("weapon_rallypack_p90:projectile/r_assaultrifleprojectile",
         me.mx, me.my, me.mz, dx * 190.0, dy * 190.0, dz * 190.0, p);
    let shake = 0.05 * s.shot.to_float();
    if shake < 0.10 { shake = 0.10; }
    if shake > 0.55 { shake = 0.55; }
    spawn_explosion(p, "weapon_rallypack_p90:projectile/r_shotgunrecoilprojectile", shake);
    if s.shot > 3 { play_thread(p, 2 + s.shot % 2, "jump"); }
    s.last = tick();
    s.sounding = true;
    s.tail = -1;
    // Distance-specific source sounds stay private to each eligible listener.
    for person in players() {
        let other = person.id;
        let target = player(other);
        if target != () && target.alive {
            let x = target.x - me.x;
            let y = target.y - me.y;
            let z = target.z - me.z;
            let distance = (x*x + y*y + z*z).sqrt();
            if distance >= 95.0 && s.distant.len() < 64 {
                s.distant.push(#{ to: other, due: tick() + (distance * 0.6).ceil().to_int() });
            }
        }
    }
    set_player(p, "gun", s);
    set_player(p, "rounds", n - 1);
    set_image_ammo(p, n > 1);
    show(p);
}
fn on_tick() {
    for person in players() {
        let p = person.id;
        let s = get_player(p, "gun");
        if !("last" in s) { continue; }
        let me = player(p);
        if me == () { continue; }
        if s.tail >= 0 && tick() >= s.tail {
            sound_at("weapon_rallypack_p90:sound/r_p90soundtail", me.x, me.y, me.z);
            s.tail = -1;
        }
        let waiting = [];
        for cue in s.distant {
            if tick() >= cue.due { play_sound(cue.to, "weapon_rallypack_p90:sound/rdistsound_pistol"); }
            else { waiting.push(cue); }
        }
        s.distant = waiting;
        if s.reloading && !me.image.ends_with("r_p90reloadingimage") { s.reloading = false; }
        set_player(p, "gun", s);
    }
}
fn on_drop(p, item, slot) {
    if item == "weapon_rallypack_p90:weapon/r_p90item" { #{ rounds: get_player(p, "rounds") } }
    else { () }
}
fn on_pickup(p, item, info) {
    if item == "weapon_rallypack_p90:weapon/r_p90item" && info.data != () && "rounds" in info.data {
        state(p);
        set_player(p, "rounds", info.data.rounds);
    }
    ()
}

