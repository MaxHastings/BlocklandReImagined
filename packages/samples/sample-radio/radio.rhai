fn on_join(id) { set_player(id, "transmitting", false); }
fn on_spawn(id) { set_player(id, "transmitting", false); }
fn cmd_radio_transmit(id, held) { set_player(id, "transmitting", held); }
fn cmd_radio_channel(id, channel) {
    if channel >= 1 && channel <= 16 { set_player(id, "channel", channel); }
}
fn voice_routes(speaker, listeners) {
    if !get_player(speaker, "transmitting") { return []; }
    let source = player(speaker);
    let channel = get_player(speaker, "channel");
    let routes = [];
    for id in listeners {
        if id == speaker { continue; }
        let target = player(id);
        let allowed = target.minigame == source.minigame
            && (source.minigame == () || source.alive || !target.alive)
            && get_player(id, "channel") == channel;
        routes.push(#{ listener: id, hear: allowed, spatial: false, range: 48.0, label: "Radio " + channel, priority: 10 });
    }
    routes
}
