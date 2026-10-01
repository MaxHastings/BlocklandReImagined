// Tool_Fill_Can's host rules, ported from its server.cs. The imported
// Fill Can fires its own projectile; what it hits decides the rest.
// Values in double braces are read from this copy's script by the
// port's patterns.

// $Pref::Server::FillCan_MaxBricks's default: bricks one fill paints.
fn limit() {
    500
}

// What administrators may fill at once (`paintfill`'s `%limit`).
fn admin_limit() {
    {{admin_limit}}
}

fn image() {
    "{{namespace}}:image/fillcanimage"
}

fn projectile() {
    "{{namespace}}:projectile/fillcanprojectile"
}

// `$face0` to `$face19`.
fn faces() {
    [
        "smiley", "asciiTerror", "memeBlockMongler", "memeCats", "memeDesu",
        "memeGrinMan", "memeHappy", "memePBear", "memeYaranika", "smileyBlonde",
        "smileyCreepy", "smileyEvil1", "smileyEvil2", "smileyFemale1", "smileyOld",
        "smileyPirate1", "smileyPirate2", "smileyPirate3", "smileyRedBeard",
        "smileyRedBeard2"
    ]
}

fn painting_disabled(player) {
    center_print(player, "Painting is currently disabled.", {{disabled_seconds}});
}

// `serverCmdFillCan`: the Fill Can in hand, unless painting is off.
fn cmd_fillcan(player) {
    let me = player(player);
    if me == () {
        return;
    }
    if !me.may_paint {
        painting_disabled(player);
        return;
    }
    mount_image(player, image());
}

// `paintfill`, `paintFXfill` and `shapeFXfill`: the bricks of the hit
// brick's colour around it, each brick's box grown by the search box's
// margin, in the colour or FX can last picked.
fn fill(me, brick) {
    let paint = if me.fx_can == () {
        #{ color: me.paint }
    } else if me.fx_can > 6 {
        #{ shape_effect: me.fx_can - 7 }
    } else {
        #{ color_effect: me.fx_can }
    };
    paint_fill(me.id, brick, paint, #{
        limit: if me.admin { admin_limit() } else { limit() },
        reach: [{{reach_side}} / 2.0, {{reach_up}} / 2.0],
        stop_at_limit: true,
        limit_message: [
            "Reached Fill Can Brick Limit (" + limit() + ")",
            {{limit_seconds}}
        ],
        limit_error: true,
        refusal_seconds: {{refusal_seconds}}
    });
}

// `getRandom(0, 100) / 100`, one of a few drawn for this hit.
fn hundredth(hit, n) {
    (hash3(seed(), tick(), hit.id, n) * 101.0).to_int().min(100) / 100.0
}

// `fillcanProjectile::onCollision` on bricks, players and vehicles.
fn on_projectile_hit(hit) {
    if hit.projectile != projectile() || hit.by == () {
        return;
    }
    let me = player(hit.by);
    if me == () {
        return;
    }
    if hit.kind == "brick" {
        if !me.may_paint {
            painting_disabled(me.id);
            return;
        }
        fill(me, hit.id);
    } else if hit.kind == "player" {
        if me.fx_can == () {
            temp_look(hit.id, #{ paint: me.paint }, {{player_ms}} / 1000.0);
        } else {
            let faces = faces();
            let pick = (hash3(seed(), tick(), hit.id, me.id) * 20.0).to_int();
            let face = faces[pick % faces.len()];
            temp_look(
                hit.id,
                #{ face: face, alpha: #{ accent: {{visor_alpha}} } },
                {{face_ms}} / 1000.0
            );
        }
    } else if hit.kind == "vehicle" {
        // A colour can paints the vehicle (through its spawn brick when
        // the brick recolours it) and its riders for a moment; an FX can
        // gives it a random colour.
        if me.fx_can == () {
            paint_vehicle(me.id, hit.ref, #{ color: me.paint }, #{
                riders_seconds: {{riders_ms}} / 1000.0,
                refusal_seconds: {{refusal_seconds}}
            });
        } else {
            let rgb = [hundredth(hit, 0), hundredth(hit, 1), hundredth(hit, 2)];
            paint_vehicle(me.id, hit.ref, #{ rgb: rgb }, #{
                refusal_seconds: {{refusal_seconds}}
            });
        }
    }
}
