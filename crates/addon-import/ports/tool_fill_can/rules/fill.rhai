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
        refusal_seconds: 3
    });
}

// `fillcanProjectile::onCollision` on bricks and players.
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
            temp_look(hit.id, #{ paint: me.paint }, 2.5);
        } else {
            let faces = faces();
            let pick = (hash3(seed(), tick(), hit.id, me.id) * 20.0).to_int();
            let face = faces[pick % faces.len()];
            temp_look(hit.id, #{ face: face, alpha: #{ accent: 0.7 } }, 2.5);
        }
    }
}
