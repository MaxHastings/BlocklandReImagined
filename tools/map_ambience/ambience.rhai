// Package policy only. Server ticks are 120 Hz; no player/gameplay state changes.
fn roll(n) {
    let seed = (get("rng") * 1664525 + 1013904223) % 4294967296;
    set("rng", seed);
    (seed / 65536) % n
}
fn later() { set("next", tick() + 4200 + roll(6001)); }
fn start() {
    if get("started") { return; }
    let p = palette();
    for clip in p.beds {
        play_ambient_all("map_ambience/"+clip, "map_ambience:"+clip, #{fade:2.0});
    }
    for local in p.locals {
        play_ambient_all("map_ambience/"+local.clip, "map_ambience:"+local.clip, #{fade:2.0,at:local.at});
    }
    later();
    set("started",true);
}
fn on_tick() {
    start();
    if tick() < get("next") { return; }
    later();
    let p = palette();
    if p.details.len() == 0 || p.anchors.len() == 0 { return; }
    // One global event per gap, not one broadcast per player. Only evidenced
    // fixed sources within reach of a living human can produce a call.
    let anchors = [];
    for i in 0..p.anchors.len() {
        if i == get("last_anchor") && p.anchors.len() > 1 { continue; }
        let a = p.anchors[i];
        let near = false;
        for human in players() {
            if human.bot || !human.alive { continue; }
            let dx = human.x-a[0]; let dy = human.y-a[1]; let dz = human.z-a[2];
            if dx*dx+dy*dy+dz*dz < 25600.0 { near=true; break; }
        }
        if near { anchors.push(i); }
    }
    if anchors.len() == 0 { return; }
    let clips = [];
    for clip in p.details {
        if clip != get("last_clip") || p.details.len() == 1 { clips.push(clip); }
    }
    let clip = clips[roll(clips.len())];
    let index = anchors[roll(anchors.len())];
    let a = p.anchors[index];
    // Profile owns range; preserve the recording's pitch and unity cue gain.
    sound_at("map_ambience:"+clip,a[0],a[1],a[2],1.0,1.0,0.0);
    set("last_clip",clip);
    set("last_anchor",index);
}
