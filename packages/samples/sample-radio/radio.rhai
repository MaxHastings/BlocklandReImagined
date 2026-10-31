fn on_join(id) { set_player(id, "transmitting", false); }
fn on_spawn(id) { set_player(id, "transmitting", false); }
fn cmd_radio_transmit(id, held) { set_player(id, "transmitting", held); }
fn cmd_radio_channel(id, channel) {
    if channel >= 1 && channel <= 16 { set_player(id, "channel", channel); }
}
fn voice_routes(speaker, listeners) {
    let source = connection(speaker);
    if source == () || source.loading || !get_player(speaker, "transmitting") { return []; }
    let channel = get_player(speaker, "channel");
    let routes = [];
    for id in listeners {
        if id == speaker { continue; }
        let target = connection(id);
        if target == () { continue; }
        let allowed = !target.loading && target.minigame == source.minigame
            && (source.minigame == () || source.alive || !target.alive)
            && get_player(id, "channel") == channel;
        routes.push(#{ listener: id, hear: allowed, spatial: false, range: 48.0, label: "Radio " + channel, conversation: "radio/" + channel, priority: 10 });
    }
    routes
}
