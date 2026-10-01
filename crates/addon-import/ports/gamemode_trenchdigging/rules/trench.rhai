// Lilboarder's Gamemode_TrenchDigging (TrenchDigging.cs), rewritten for
// this game. The shovel takes dirt out of the Add-On's dirt bricks: a big
// cube splits into eight, the one nearest the hit splits again, down to a
// 2x cube that goes into your pocket (a flat splits into quarters down to
// a 1x1). The dirt item puts it back as 2x cubes (or 1x1 flats), and eight
// that make a cube merge back into the next size up, up to 64x (four
// flats into the next flat, up to 8x8). /dumpdirt piles your dirt as 4x
// cubes (or 4x4 flats), /speeddig and /speedplace hand admins the fast
// tools, /infinitedigging gives an admin endless dirt.
//
// Each function says which original function it stands for. The work is
// done in Torque's frame (x east, y north, z up, as the original reads
// positions), turned into this game's (x, up, -north) only to ask and
// plant. One command's edits are gathered first (`ctx`) and sent at the
// end, removals before plants, as the original's bricks are gone before
// the ones replacing them are planted.

fn brick_prefix() { "{{namespace}}:brick/" }
// $TrenchDig::dirtCount: server.cs sets 100 when RTB is not installed.
fn max_dirt() { 100 }
fn cube_names() {
    ["brick2xcubedirtdata", "brick4xcubedirtdata", "brick8xcubedirtdata",
     "brick16xcubedirtdata", "brick32xcubedirtdata", "brick64xcubedirtdata"]
}
fn flat_names() {
    ["brick1x1dirtdata", "brick2x2dirtdata", "brick4x4dirtdata",
     "brick8x8dirtdata", "brick8x16dirtdata"]
}
fn kind_of(flat, level) {
    brick_prefix() + if flat { flat_names()[level] } else { cube_names()[level] }
}
// The dirt sizes: a cube's edge, a flat's width (8x16 is 4 by 8), and the
// flats' height, in Torque units.
fn cube_edge(level) { [1.0, 2.0, 4.0, 8.0, 16.0, 32.0][level] }
fn flat_width(level) { [0.5, 1.0, 2.0, 4.0, 4.0][level] }
fn flat_height() { 0.6 }

// Torque's frame and this game's.
fn tq(n) { [n[0], -n[2], n[1]] }
fn nat(t) { [t[0], t[2], -t[1]] }
fn snap(v) { (v * 20.0).round() / 20.0 }
fn near(a, b) {
    (a[0] - b[0]).abs() < 0.02 && (a[1] - b[1]).abs() < 0.02 && (a[2] - b[2]).abs() < 0.02
}

// A trench dirt brick as the rules work on it, or () for any other brick
// (`getDatablock().isTrenchDirt`).
fn dirt_brick(id) {
    let b = brick(id);
    if b == () { return (); }
    let prefix = brick_prefix();
    if !b.kind.starts_with(prefix) { return (); }
    let name = b.kind.sub_string(prefix.len());
    let flat = false;
    let level = cube_names().index_of(name);
    if level < 0 {
        flat = true;
        level = flat_names().index_of(name);
        if level < 0 { return (); }
    }
    #{
        id: id, kind: b.kind, flat: flat, level: level,
        p: tq([b.x, b.y, b.z]),
        e: [snap(b.max[0] - b.min[0]), snap(b.max[2] - b.min[2]), snap(b.max[1] - b.min[1])],
        color: b.color, owner: b.owner,
    }
}

// A dirt brick of `level` the rules are about to plant.
fn new_dirt(flat, level, p, color, owner) {
    let e = if flat {
        let w = flat_width(level);
        [w, w, flat_height()]
    } else {
        let s = cube_edge(level);
        [s, s, s]
    };
    #{ id: 0, kind: kind_of(flat, level), flat: flat, level: level, p: p, e: e, color: color, owner: owner }
}

// findSmallBrick: the centre of the `w`-sized piece of `b` nearest `at`,
// the lower one on a tie, as the original's scan from its minimum keeps
// the first nearest.
fn nearest_piece(b, at, w) {
    let out = [];
    for a in 0..3 {
        let n = (b.e[a] / w[a]).round();
        if n < 1.5 {
            out.push(b.p[a]);
            continue;
        }
        let lo = b.p[a] - (b.e[a] - w[a]) / 2.0;
        let k = ((at[a] - lo) / w[a] - 0.5).ceiling();
        if k < 0.0 { k = 0.0; }
        if k > n - 1.0 { k = n - 1.0; }
        out.push(lo + k * w[a]);
    }
    out
}

// The size of the pieces `b` splits into (fxDtsBrick::Refill's
// %wantSize).
fn piece_size(b) {
    if b.flat {
        let w = flat_width(b.level - 1);
        [w, w, b.e[2]]
    } else {
        let s = cube_edge(b.level - 1);
        [s, s, s]
    }
}

// fillPos: the centres of the pieces filling `b`.
fn piece_centres(b, w) {
    let n = [];
    for a in 0..3 { n.push((b.e[a] / w[a]).round().to_int()); }
    let out = [];
    for i in 0..n[0] {
        for j in 0..n[1] {
            for k in 0..n[2] {
                let c = [i, j, k];
                let p = [];
                for a in 0..3 {
                    p.push(b.p[a] + (c[a].to_float() - (n[a] - 1).to_float() / 2.0) * w[a]);
                }
                out.push(p);
            }
        }
    }
    out
}

// ---- One command's edits: `this` is #{ gone: [ids], planted: [bricks], next }.

fn take_out(b) {
    if b.id < 0 {
        let i = 0;
        while i < this.planted.len() {
            if this.planted[i].id == b.id {
                this.planted.remove(i);
                return;
            }
            i += 1;
        }
    } else if !this.gone.contains(b.id) {
        this.gone.push(b.id);
    }
}

fn put_in(b) {
    this.next += 1;
    b.id = -this.next;
    this.planted.push(b);
    b
}

fn send_edits() {
    for id in this.gone { remove_brick(id); }
    for b in this.planted { plant_brick(b.kind, nat(b.p), 0, b.color, b.owner); }
}

// The dirt bricks of `b`'s size and build whose centres sit within one
// size of `b`'s, as they will be once this command's edits are made.
fn same_beside(b, s) {
    let out = [];
    let c = nat(b.p);
    let up = if b.flat { 0.1 } else { s };
    for id in bricks_in([c[0] - s, c[1] - up, c[2] - s], [c[0] + s, c[1] + up, c[2] + s]) {
        if id == b.id || this.gone.contains(id) { continue; }
        let o = dirt_brick(id);
        if o != () && o.kind == b.kind && o.owner == b.owner { out.push(o); }
    }
    for o in this.planted {
        if o.id != b.id && o.kind == b.kind && o.owner == b.owner { out.push(o); }
    }
    out
}

// fxDtsBrick::Refill and continueRefill: split `b` and go on into the
// piece nearest `at`, until the smallest piece, which onTrenchDig takes.
fn dig_into(b, at, p) {
    loop {
        if b.level == 0 {
            this.take_dirt(b, p);
            return;
        }
        let w = piece_size(b);
        let target = nearest_piece(b, at, w);
        this.take_out(b);
        let next = ();
        for c in piece_centres(b, w) {
            let piece = this.put_in(new_dirt(b.flat, b.level - 1, c, b.color, b.owner));
            if near(c, target) { next = piece; }
        }
        b = next;
    }
}

// onTrenchDig
fn take_dirt(b, p) {
    let infinite = get_player(p, "infinite");
    let dirt = get_player(p, "dirt");
    if !infinite && dirt >= max_dirt() {
        center_print(p, "\\c3You do not have enough room for any more dirt!", 1);
        return;
    }
    if !infinite {
        let bricks = get_player(p, "bricks");
        bricks.push(b.color);
        set_player(p, "dirt", dirt + 1);
        set_player(p, "bricks", bricks);
    }
    this.take_out(b);
    update_dirt(p);
}

// fxDtsBrick::checkBricks and finishRegrouping: when `b` and the bricks
// beside it make a brick of the next size (eight cubes, four flats), they
// become it, in the colour most of them have, and the new one is checked
// in turn: up to a 64x cube, or an 8x8 flat.
fn regroup(b) {
    loop {
        if b.flat && b.level > 2 || !b.flat && b.level > 4 { return; }
        let s = b.e[0];
        let found = #{};
        for o in this.same_beside(b, s) {
            let d = [];
            let fits = true;
            for a in 0..3 {
                let v = (b.p[a] - o.p[a]) / s;
                let r = v.round();
                if (v - r).abs() > 0.02 || r.abs() > 1.0 { fits = false; }
                d.push(r.to_int());
            }
            if fits && (!b.flat || d[2] == 0) && (d[0] != 0 || d[1] != 0 || d[2] != 0) {
                found[`${d[0]},${d[1]},${d[2]}`] = o;
            }
        }
        let need = if b.flat { 3 } else { 7 };
        if found.len() < need { return; }
        // The first fullest direction, in the original's order.
        let best = ();
        let best_count = -1;
        let zs = if b.flat { [0] } else { [-1, 1] };
        for x in [-1, 1] {
            for y in [-1, 1] {
                for z in zs {
                    let count = 0;
                    for k in found.keys() {
                        let d = k.split(",");
                        let dx = parse_int(d[0]);
                        let dy = parse_int(d[1]);
                        let dz = parse_int(d[2]);
                        if (dx == 0 || dx == x) && (dy == 0 || dy == y) && (dz == 0 || dz == z) {
                            count += 1;
                        }
                    }
                    if count > best_count {
                        best_count = count;
                        best = [x, y, z];
                    }
                }
            }
        }
        if best_count < need { return; }
        let members = [b];
        let colors = #{};
        colors[`${b.color}`] = 1;
        for k in found.keys() {
            let d = k.split(",");
            let dx = parse_int(d[0]);
            let dy = parse_int(d[1]);
            let dz = parse_int(d[2]);
            if (dx == 0 || dx == best[0]) && (dy == 0 || dy == best[1]) && (dz == 0 || dz == best[2]) {
                let o = found[k];
                members.push(o);
                let c = `${o.color}`;
                colors[c] = if c in colors { colors[c] + 1 } else { 1 };
            }
        }
        // The colour most of them have, the lowest on a tie.
        let color = b.color;
        let most = 0;
        for c in colors.keys() {
            let n = colors[c];
            let id = parse_int(c);
            if n > most || n == most && id < color {
                most = n;
                color = id;
            }
        }
        let at = [];
        for a in 0..3 { at.push(b.p[a] - best[a].to_float() * s / 2.0); }
        for m in members { this.take_out(m); }
        b = this.put_in(new_dirt(b.flat, b.level + 1, at, color, b.owner));
    }
}

// GameConnection::updateDirt
fn update_dirt(p) {
    let shown = if get_player(p, "infinite") { "∞" } else { `${get_player(p, "dirt")}` };
    bottom_print(p, "\\c3" + shown + "\\c6/\\c3" + max_dirt() + " dirt", 0);
}

fn edits() { #{ gone: [], planted: [], next: 0 } }

// The dirt brick the command was aimed at, or ().
fn aimed_dirt() {
    let a = aim();
    if a == () || a.brick == () { return (); }
    let b = dirt_brick(a.brick);
    if b == () { return (); }
    b.at = tq([a.x, a.y, a.z]);
    b.n = tq([a.nx.round(), a.ny.round(), a.nz.round()]);
    b
}

// TakeChunk: TrenchShovelImage::onFire and AdminShovelImage::onFire.
fn cmd_dig(p) {
    let a = aim();
    if a == () || a.brick == () { return; }
    if !get_player(p, "infinite") && get_player(p, "dirt") >= max_dirt() {
        center_print(p, "\\c3You do not have enough room for any more dirt!", 1);
        return;
    }
    let b = aimed_dirt();
    if b == () || !can_edit(b.id) { return; }
    // Fixing getting stuck within new bricks: a hop off the brick you
    // stand on.
    let me = player(p);
    let under = raycast([me.x, me.y, me.z], [0.0, -1.0, 0.0], 0.1, p);
    if under != () && under.kind == "brick" && under.id == b.id {
        push("player:" + p, 0.0, 2.0, 0.0);
    }
    let ctx = edits();
    ctx.dig_into(b, b.at, p);
    ctx.send_edits();
}

// ShootChunk: TrenchDirtImage::onFire and AdminDirtImage::onFire.
fn cmd_place(p) {
    let infinite = get_player(p, "infinite");
    let dirt = get_player(p, "dirt");
    // Parent::onFire: the shot of dirt, while there is dirt to show.
    if !infinite && dirt > 0 {
        let me = player(p);
        fire("{{namespace}}:projectile/trenchdirtprojectile", me.mx, me.my, me.mz,
            me.lx * 40.0 + me.vx, me.ly * 40.0 + me.vy, me.lz * 40.0 + me.vz, p);
    }
    if dirt <= 0 && !infinite {
        center_print(p, "\\c3You have no dirt to release!", 1);
        return;
    }
    let b = aimed_dirt();
    if b == () || !can_edit(b.id) { return; }
    let at = [];
    if b.flat {
        let piece = nearest_piece(b, b.at, [0.5, 0.5, b.e[2]]);
        if piece[2] - b.at[2] >= 0.29 {
            // Under a flat: below it.
            at = [piece[0], piece[1], piece[2] - flat_height()];
        } else {
            for a in 0..3 { at.push(piece[a] + b.n[a] * [0.5, 0.5, flat_height()][a]); }
        }
    } else {
        let piece = nearest_piece(b, b.at, [1.0, 1.0, 1.0]);
        for a in 0..3 { at.push(piece[a] + b.n[a]); }
    }
    let kind = kind_of(b.flat, 0);
    if !can_plant(kind, nat(at), 0) { return; }
    let color = 0;
    let bricks = get_player(p, "bricks");
    if !infinite && dirt <= bricks.len() { color = bricks[dirt - 1]; }
    let ctx = edits();
    let placed = ctx.put_in(new_dirt(b.flat, 0, at, color, b.owner));
    if !infinite {
        bricks.truncate(dirt - 1);
        set_player(p, "dirt", dirt - 1);
        set_player(p, "bricks", bricks);
        update_dirt(p);
    }
    ctx.regroup(placed);
    ctx.send_edits();
}

// serverCmdDumpDirt: your dirt as 4x cubes (eight each) or 4x4 flats
// (sixteen each), piled up from the first free spot beside where you look.
fn cmd_dumpdirt(p) {
    if get_player(p, "infinite") {
        center_print(p, "\\c3You have infinite digging mode on! Why would you want to dump infinite blocks?", 2);
        return;
    }
    let b = aimed_dirt();
    if b == () || !can_edit(b.id) { return; }
    let start = ();
    let per = 8;
    let rise = 2.0;
    let level = 1;
    if b.flat {
        let piece = nearest_piece(b, b.at, [0.5, 0.5, b.e[2]]);
        let base = [];
        for a in 0..3 { base.push(piece[a] + b.n[a] * 0.5); }
        base[2] -= flat_height();
        for x in [-0.75, -0.25, 0.25, 0.75] {
            for y in [-0.75, -0.25, 0.25, 0.75] {
                let spot = [base[0] + x, base[1] + y, base[2] + flat_height()];
                if start == () && free(spot, [1.9, 1.9, 0.5]) { start = spot; }
            }
        }
        per = 16;
        rise = flat_height();
        level = 2;
    } else {
        let piece = nearest_piece(b, b.at, [1.0, 1.0, 1.0]);
        let base = [];
        for a in 0..3 { base.push(piece[a] + b.n[a]); }
        base[2] -= 1.0;
        for o in [[0.5, 0.5], [0.5, -0.5], [-0.5, 0.5], [-0.5, -0.5]] {
            let spot = [base[0] + o[0], base[1] + o[1], base[2] + 1.5];
            if start == () && free(spot, [1.5, 1.5, 1.5]) { start = spot; }
        }
    }
    let dirt = get_player(p, "dirt");
    let bricks = get_player(p, "bricks");
    let whole = dirt / per;
    let ctx = edits();
    let dumped = 0;
    let at = start;
    for i in 0..whole {
        let color = if dirt >= 2 && dirt - 1 <= bricks.len() { bricks[dirt - 2] } else { 0 };
        if at == () || !can_plant(kind_of(b.flat, level), nat(at), 0) {
            if dumped == 0 {
                bottom_print(p, "\\c3You can't dump dirt there!", 2);
                // %client.schedule(2000, updateDirt)
                set_player(p, "reprint", tick() + 63);
            }
            break;
        }
        ctx.put_in(new_dirt(b.flat, level, at, color, b.owner));
        dumped += 1;
        dirt -= per;
        at = [at[0], at[1], at[2] + rise];
    }
    if dumped > 0 {
        bricks.truncate(dirt);
        set_player(p, "dirt", dirt);
        set_player(p, "bricks", bricks);
        update_dirt(p);
    }
    ctx.send_edits();
}

// getBrickAtPos(%pos, %box) == 0
fn free(at, size) {
    let c = nat(at);
    let h = [size[0] / 2.0, size[2] / 2.0, size[1] / 2.0];
    bricks_in([c[0] - h[0], c[1] - h[1], c[2] - h[2]], [c[0] + h[0], c[1] + h[1], c[2] + h[2]]).len() == 0
}

fn on_tick() {
    for me in players() {
        let at = get_player(me.id, "reprint");
        if at > 0 && tick() >= at {
            set_player(me.id, "reprint", 0);
            update_dirt(me.id);
        }
    }
}

// serverCmdSpeedDig, serverCmdSpeedPlace
fn cmd_speeddig(p) { mount_image(p, "{{namespace}}:image/adminshovelimage"); }
fn cmd_speedplace(p) { mount_image(p, "{{namespace}}:image/admindirtimage"); }

// serverCmdInfiniteDigging
fn cmd_infinitedigging(p) {
    if get_player(p, "infinite") {
        set_player(p, "infinite", false);
        set_player(p, "dirt", get_player(p, "old"));
        tell(p, "\\c3Infinite Digging is now \\c6OFF");
    } else {
        set_player(p, "infinite", true);
        set_player(p, "old", get_player(p, "dirt"));
        tell(p, "\\c3Infinite Digging is now \\c6ON");
    }
    update_dirt(p);
}
