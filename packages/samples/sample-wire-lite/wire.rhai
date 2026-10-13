// Wire Lite: a small Wiremod. `/gate <kind>` turns the brick you aim at
// into a switch, an and, or, not or xor gate, or a lamp. `/wire` on one
// gate and then another feeds the first into the second; so does dragging
// one gate onto another in `/circuit`. Clicking a switch with an empty hand
// flips it, the signal settles through the gates, and every gate shows
// its output in its colour. The circuit is kept with the world.

fn kinds() { ["switch", "and", "or", "not", "xor", "lamp"] }
fn most_gates() { 32 }
fn most_inputs() { 4 }

// The palette colour nearest `rgb`.
fn nearest(rgb) {
    let best = 0;
    let gap = 1000.0;
    let i = 0;
    for c in palette() {
        let d = (c[0] - rgb[0]) * (c[0] - rgb[0]) + (c[1] - rgb[1]) * (c[1] - rgb[1])
            + (c[2] - rgb[2]) * (c[2] - rgb[2]);
        if d < gap { gap = d; best = i; }
        i += 1;
    }
    best
}

fn input_values(gates, g) {
    let values = [];
    for i in g.inputs {
        let from = gates[`${i}`];
        if from != () { values.push(from.on); }
    }
    values
}

// What a gate puts out for what comes in.
fn output(gates, g) {
    let n = 0;
    let on = 0;
    for v in input_values(gates, g) {
        n += 1;
        if v { on += 1; }
    }
    switch g.kind {
        "and" => n > 0 && on == n,
        "or" | "lamp" => on > 0,
        "not" => on == 0,
        "xor" => on % 2 == 1,
        _ => g.on
    }
}

// Let the signal run through the circuit until nothing changes (a loop
// that never settles stops after one pass per gate), forget gates whose
// brick is gone, and paint every gate whose output differs from the
// circuit as it was kept (a new gate is painted too).
fn settle(gates) {
    let kept = get("gates");
    for k in gates.keys() {
        if brick(k.parse_int()) == () { gates.remove(k); }
    }
    for k in gates.keys() {
        let g = gates[k];
        let kept = [];
        for i in g.inputs {
            if gates.contains(`${i}`) { kept.push(i); }
        }
        g.inputs = kept;
        gates[k] = g;
    }
    let passes = gates.len() + 1;
    for pass in 0..passes {
        let changed = false;
        for k in gates.keys() {
            let g = gates[k];
            let on = output(gates, g);
            if on != g.on {
                g.on = on;
                gates[k] = g;
                changed = true;
            }
        }
        if !changed { break; }
    }
    let lit = nearest([1.0, 0.85, 0.1]);
    let dark = nearest([0.25, 0.25, 0.25]);
    for k in gates.keys() {
        let on = gates[k].on;
        let was = kept[k];
        if was == () || was.on != on {
            set_brick_color(k.parse_int(), if on { lit } else { dark });
        }
    }
    gates
}

fn aimed(p) {
    let a = aim();
    if a == () || a.brick == () {
        tell(p, "Aim at a brick.");
        return ();
    }
    if !can_edit(a.brick) {
        tell(p, "That brick is not yours to wire.");
        return ();
    }
    a.brick
}

fn cmd_gate(p, kind) {
    kind = kind.to_lower();
    if !kinds().contains(kind) {
        tell(p, `A gate is one of: ${kinds()}`);
        return;
    }
    let b = aimed(p);
    if b == () { return; }
    let gates = get("gates");
    let key = `${b}`;
    let was = gates[key];
    if was == () && gates.len() >= most_gates() {
        tell(p, `A circuit has at most ${most_gates()} gates.`);
        return;
    }
    let inputs = if was == () || kind == "switch" { [] } else { was.inputs };
    gates[key] = #{ kind: kind, inputs: inputs, on: false };
    set("gates", settle(gates));
    tell(p, `Brick ${b} is now a ${kind}.`);
}

// Feed gate `from` into gate `to`; what went wrong, or "".
fn wire_up(from, to) {
    let gates = get("gates");
    let target = gates[`${to}`];
    if gates[`${from}`] == () || target == () { return "Both ends must be gates."; }
    if from == to { return "A gate cannot feed itself."; }
    if target.kind == "switch" { return "A switch takes no inputs; click it instead."; }
    if target.inputs.contains(from) { return "Those are already wired."; }
    if target.inputs.len() >= most_inputs() {
        return `A gate takes at most ${most_inputs()} inputs.`;
    }
    target.inputs.push(from);
    gates[`${to}`] = target;
    set("gates", settle(gates));
    ""
}

fn cmd_wire(p) {
    let b = aimed(p);
    if b == () { return; }
    if get("gates")[`${b}`] == () {
        tell(p, "Make it a gate first, such as /gate and.");
        return;
    }
    let from = get_player(p, "from");
    if from < 0 || from == b {
        set_player(p, "from", b);
        tell(p, "Now aim at the gate it feeds and /wire again.");
        return;
    }
    set_player(p, "from", -1);
    let problem = wire_up(from, b);
    tell(p, if problem == "" { `Wired ${from} into ${b}.` } else { problem });
}

// An empty-handed click on a switch flips it.
fn on_activate(p) {
    let me = player(p);
    let hit = raycast([me.ex, me.ey, me.ez], [me.lx, me.ly, me.lz], 8.0, p);
    if hit == () || hit.kind != "brick" { return false; }
    let gates = get("gates");
    let key = `${hit.id}`;
    let g = gates[key];
    if g == () || g.kind != "switch" { return false; }
    g.on = !g.on;
    gates[key] = g;
    set("gates", settle(gates));
    true
}

// The gates in the window's order, by brick id.
fn order() {
    let ids = [];
    for k in get("gates").keys() { ids.push(k.parse_int()); }
    ids.sort();
    ids
}

fn row(id, g) {
    let state = if g.on { "ON" } else { "off" };
    let from = if g.inputs.is_empty() { "" } else { `  <- ${g.inputs}` };
    `#${id} ${g.kind} ${state}${from}`
}

fn show(p, note) {
    let gates = get("gates");
    let rows = [];
    for id in order() { rows.push(row(id, gates[`${id}`])); }
    show_panel(p, #{
        id: "circuit", title: "Circuit", width: 360, height: 320,
        items: [
            #{ kind: "text", text: `Drag a gate onto another to feed it in.\n${note}` },
            #{ kind: "list", id: "gates", rows: rows, height: 160, drag: true },
            #{ kind: "buttons", buttons: [
                #{ id: "unwire", label: "Unwire" },
                #{ id: "remove", label: "Remove" }
            ] }
        ]
    });
}

fn cmd_circuit(p) { show(p, ""); }

fn on_panel(p, e) {
    if e.control == "close" { return; }
    let ids = order();
    let note = "";
    if e.drop != () {
        if e.drop.at == () {
            note = "Drop it on the gate it feeds.";
        } else if e.drop.row >= ids.len() || !e.drop.text.starts_with(`#${ids[e.drop.row]} `) {
            note = "The circuit changed; try again.";
        } else {
            let from = ids[e.drop.row];
            let to = ids[e.drop.at];
            if !can_edit(from) || !can_edit(to) {
                note = "Those bricks are not yours to wire.";
            } else {
                note = wire_up(from, to);
                if note == "" { note = `Wired ${from} into ${to}.`; }
            }
        }
    } else if e.control == "unwire" || e.control == "remove" {
        let picked = e.lists.gates;
        if picked == () || picked >= ids.len() {
            note = "Pick a gate first.";
        } else if !can_edit(ids[picked]) {
            note = "That brick is not yours to change.";
        } else {
            let gates = get("gates");
            let key = `${ids[picked]}`;
            if e.control == "unwire" {
                let g = gates[key];
                g.inputs = [];
                gates[key] = g;
            } else {
                gates.remove(key);
            }
            set("gates", settle(gates));
        }
    } else {
        return;
    }
    show(p, note);
}
