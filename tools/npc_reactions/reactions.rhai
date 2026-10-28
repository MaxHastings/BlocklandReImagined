// Policy belongs to this optional Add-On. No operations change NPC controls,
// damage, objectives, inventory or round rules. Time is server ticks (120 Hz).
fn roll(n) {
    let seed = (get("rng") * 1664525 + 1013904223) % 4294967296;
    set("rng", seed);
    (seed / 65536) % n
}
fn voice(p) {
    let v = get_player(p.id, "voice");
    if v.len() > 0 { return v; }
    #{next:0, combat:0, health:p.health, vy:p.vy, grounded:p.grounded,
      mounted:p.mounted, kills:0, last_kill:0, idle:tick() + 3600 + roll(3600)}
}
fn cue(bot, event, priority, chance) {
    let p = player(bot);
    if p == () || !p.bot { return; }
    note("eligible",event);
    let v = voice(p);
    if priority < 6 && tick() < v.next { return; }
    if roll(100) >= chance { return; }
    let queue = get("pending");
    // One pending reaction per bot: urgent outcomes supersede chatter.
    let pending = [];
    for q in queue {
        if q.expires > tick() && (q.bot != bot || q.priority > priority) { pending.push(q); }
        if q.bot == bot && q.priority > priority { return; }
    }
    if pending.len() >= 16 {
        let lowest = 0;
        for i in 1..pending.len() {
            if pending[i].priority < pending[lowest].priority { lowest = i; }
        }
        if pending[lowest].priority >= priority { return; }
        pending.remove(lowest);
    }
    pending.push(#{bot:bot, event:event, priority:priority,
                   expires:tick()+if priority>=6 {420} else {150},
                   x:p.x, y:p.y+1.0, z:p.z});
    set("pending", pending);
}
fn engaged(bot) {
    let p = player(bot);
    if p == () || !p.bot { return; }
    let v = voice(p);
    v.combat = tick() + 1800;
    set_player(bot, "voice", v);
}
fn on_bot_spotted(b, e) { engaged(b); cue(b,"spotted",3,38); }
fn on_bot_heard_gunfire(b, e) { engaged(b); cue(b,"heard_fire",1,20); }
fn on_bot_under_fire(b, e) { engaged(b); cue(b,"under_fire",3,28); }
fn on_bot_sees_grenade(b, e) { engaged(b); cue(b,"grenade",4,55); }
fn on_bot_friend_died(b, f) { cue(b,"friend_died",3,28); }
fn on_bot_lost_enemy(b, e) { cue(b,"lost_enemy",1,18); }
fn on_bot_out_of_ammo(b) { cue(b,"out_of_ammo",3,65); }
fn on_bot_objective_acquired(b) { cue(b,"objective_acquired",6,100); }
fn on_bot_objective_completed(b) { cue(b,"objective_completed",7,100); }
fn on_bot_stalled(b) { cue(b,"stalled",1,25); }
fn on_spawn(b) {
    let p = player(b);
    if p == () || !p.bot { return; }
    // Never carry health/fall history across lives.
    set_player(b,"voice",#{});
    cue(b,"spawn",0,12);
}
fn on_death(victim, killer) {
    let p = player(victim);
    if p != () && p.bot { cue(victim,"death",5,70); }
    if killer != () && killer != victim {
        let k = player(killer);
        if k != () && k.bot {
            if p != () && p.team != () && p.team == k.team && p.minigame == k.minigame { return; }
            let v = voice(k);
            v.kills = if tick() - v.last_kill < 900 { v.kills + 1 } else { 1 };
            v.last_kill = tick();
            set_player(killer,"voice",v);
            cue(killer, if v.kills >= 2 { "multi_kill" } else { "kill" }, 3, 45);
        }
    }
}
fn on_minigame(e) {
    if e.kind != "round_end" { return; }
    if e.players.len() == 0 && e.teams.len() == 0 { return; }
    for p in bots() {
        if !p.bot || p.minigame != e.game { continue; }
        let won = e.players.contains(p.id) || e.teams.contains(p.team);
        cue(p.id, if won { "round_win" } else { "round_loss" }, 4, 40);
    }
}
fn note(key, value) {
    let map = get(key);
    map[value] = if map.contains(value) { map[value]+1 } else { 1 };
    set(key, map);
}
fn mix() {
    let active = [];
    for a in get("active") { if a.expires > tick() { active.push(a); } }
    set("active", active);
    let pending = [];
    for q in get("pending") { if q.expires > tick() { pending.push(q); } }
    set("pending", pending);
    if tick() < get("next") || active.len() >= 2 || pending.len() == 0 { return; }
    // Shuffle ties so an owner's numeric ID does not dominate the mix.
    let choice = roll(pending.len());
    for i in 0..pending.len() {
        if pending[i].priority > pending[choice].priority { choice = i; }
    }
    let q = pending.remove(choice);
    set("pending", pending);
    let p = player(q.bot);
    if p == () || (!p.alive && q.event != "death") { return; }
    let v = voice(p);
    if tick() < v.next && q.priority < 6 { return; }
    let x = if q.event == "death" { q.x } else { p.x };
    let y = if q.event == "death" { q.y } else { p.y+1.0 };
    let z = if q.event == "death" { q.z } else { p.z };
    for a in active {
        if a.bot == q.bot || (x-a.x)*(x-a.x)+(z-a.z)*(z-a.z)<576.0 {
            if q.priority >= 6 { pending.push(q); set("pending",pending); }
            return;
        }
    }
    let recent = get("recent");
    let candidates = [];
    for c in clips(q.event) {
        if !recent.contains(c.id) || tick() >= recent[c.id] { candidates.push(c); }
    }
    if candidates.len() == 0 { return; }
    // Prefer short reactions (65/30/5 where the situation has all tiers).
    // Fall back only within the same supported situation, never to a wrong gag.
    let draw = roll(100);
    let tier = if draw < 65 { "micro" } else if draw < 95 { "quote" } else { "rare" };
    let preferred = [];
    for c in candidates { if c.tier == tier { preferred.push(c); } }
    let c = if preferred.len()>0 { preferred[roll(preferred.len())] } else { candidates[roll(candidates.len())] };
    let persona = q.bot % 4;
    let pitch = if c.tier == "micro" { 1.04 + persona.to_float()*0.04 }
                else if persona == 0 { 1.32 } else if persona == 1 { 0.86 }
                else if persona == 2 { 1.14 } else { 1.0 };
    sound_at(c.profile,x,y,z,0.72,pitch,38.0);
    active.push(#{bot:q.bot,expires:tick()+(c.seconds/pitch*120.0).ceil().to_int()+18,x:x,y:y,z:z});
    set("active",active);
    recent[c.id] = tick() + if c.iconic { 14400 } else { 5400 };
    set("recent",recent);
    v.next = tick()+1200+roll(720);
    set_player(p.id,"voice",v);
    set("next",tick()+360);
    set("played",get("played")+1);
    note("by_event",q.event); note("by_clip",c.id); note("by_tier",c.tier);
}
fn on_tick() {
    for p in bots() {
        if !p.bot || !p.alive { continue; }
        let v = voice(p);
        // Observe accepted health changes, never the pre-filter damage hook.
        let loss = v.health-p.health;
        if loss > 0.01 {
            if loss >= p.max_health*0.28 { cue(p.id,"heavy_damage",5,70); }
            else { cue(p.id,"minor_damage",3,40); }
            v.combat = tick()+1800;
        }
        if p.health-v.health > 5.0 { cue(p.id,"healed",1,35); }
        if !p.mounted {
            if !v.grounded && p.grounded && v.vy < -9.0 { cue(p.id,"landing",2,40); }
            if p.vy < -13.0 && v.vy >= -13.0 { cue(p.id,"fall",4,55); }
            if p.vy > 13.0 && v.vy <= 13.0 { cue(p.id,"launch",2,30); }
        }
        if p.mounted && !v.mounted { cue(p.id,"mounted",1,22); }
        if !p.mounted && v.mounted { cue(p.id,"dismounted",1,18); }
        // Outside games only. Standing guard or holding an objective is not a bug.
        if p.minigame == () && tick() > v.combat && tick() > v.idle {
            cue(p.id,"idle",0,35); v.idle = tick()+5400+roll(3600);
        }
        v.health=p.health; v.vy=p.vy; v.grounded=p.grounded; v.mounted=p.mounted;
        set_player(p.id,"voice",v);
    }
    mix();
}
