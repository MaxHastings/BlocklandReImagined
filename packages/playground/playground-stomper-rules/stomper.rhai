// What touching a Stomper means: landed on from above, or run over by a
// vehicle going at a pace, it is flattened. The engine only says who met
// whom, from where and how fast (`on_contact`).
fn kind() { "playground-stomper:bot/stomper" }

fn on_contact(info) {
    if info.target == () { return; }
    let target = player(info.target);
    if target == () || !target.alive || target.bot_kind != kind() { return; }
    let stomped = info.kind == "body" && info.from == "above" && info.velocity[1] < -2.0;
    let run_over = info.kind == "vehicle" && info.speed > 4.0;
    if !stomped && !run_over { return; }
    // Stompers do not flatten each other.
    if info.by != () {
        let by = player(info.by);
        if by != () && by.bot_kind == kind() && info.kind == "body" { return; }
    }
    damage(info.target, 1000.0, info.by, "Flattened");
}
