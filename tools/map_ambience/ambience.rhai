// Simple package-owned bases and measured local appliance. No random events.
fn on_tick() {
    if get("started") { return; }
    let p = palette();
    for clip in p.beds {
        play_ambient_all("ma/"+clip, "map_ambience:"+clip, #{fade:2.0});
    }
    for local in p.locals {
        play_ambient_all("ma/"+local.clip, "map_ambience:"+local.clip, #{fade:2.0,at:local.at});
    }
    set("started",true);
}
