// Package-owned behavior contributes vocabulary to the SAME event rows.
// Its private switch state is deliberately independent of workshop variables.
// Reloading the package/server resets it; MiniGame Reset does not.
fn on_brick_output(output, target, params, info) {
    if output != "cycleRoute" { return; }
    let routes = get("routes");
    let key = `${target}`;
    let before = if routes.contains(key) { routes[key] } else { 0 };
    let next = (before % 3) + 1;
    routes[key] = next;
    set("routes", routes);
    let input = if next == 1 { "onWorkshopRed" }
                else if next == 2 { "onWorkshopGreen" }
                else { "onWorkshopBlue" };
    #{ input: input }
}
