// All recipient/playback defaults live here, not in the engine.
fn voice_routes(speaker, listeners) {
    let source = player(speaker);
    let game = source.minigame;
    let mode = if game == () { server_setting("outside_mode") } else { setting(game, "mode") };
    let range = if game == () { server_setting("outside_range") } else { setting(game, "range") };
    let routes = [];
    for id in listeners {
        if id == speaker { continue; }
        let target = player(id);
        if target.minigame != game { continue; }
        if game != () && !source.alive && target.alive { continue; }
        if mode == "team" && source.team != target.team { continue; }
        routes.push(#{ listener: id, spatial: mode == "proximity", range: range.to_float(), label: "", priority: 0 });
    }
    routes
}
