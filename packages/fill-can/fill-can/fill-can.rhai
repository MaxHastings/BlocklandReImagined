// The Fill Can: paint a whole area in one go. Our own tool, after
// Mr.Noßody's Fill Can for Blockland. It runs on the host.
//
//   /fillcan puts the Fill Can in your hand.
//   Pick a colour with the paint keys as for the spray can, then take the
//   Fill Can out again: it shows the colour it will paint. Click a brick,
//   and it and every brick of the same colour touching it (side by side,
//   stacked or hanging under, joined by studs or not) turn that colour.
//   A fill flows around bricks you may not paint and stops at other
//   colours. Ctrl+Z takes the whole fill back.

// Most bricks one fill may paint. Painting costs the host well under a
// microsecond a brick and each player's game redraws the chunks it
// touched; 5000 matches the Advanced Duplicator's copies.
fn limit() {
    5000
}

fn tool() {
    "fill-can-tool:weapon/fill-can"
}

fn cmd_fillcan(player) {
    give_item(player, tool(), true);
    bottom_print(player, "Fill Can: pick a colour with your paint keys, then click a brick. /fillhelp for more", 4);
}

fn cmd_fillhelp(player) {
    tell(player, "Fill Can: /fillcan takes it out. It paints in the colour you last picked with your paint keys.");
    tell(player, "Click a brick: it and every brick of its colour touching it turn your colour, up to " + limit() + " at once.");
    tell(player, "Bricks you may not paint are left as they are. Ctrl+Z takes a fill back.");
}

// The Fill Can's spray (`onFire` of its image).
fn cmd_fill(player) {
    let hit = aim();
    if hit == () || hit.brick == () {
        return;
    }
    let me = player(player);
    if me != () {
        paint_fill(player, hit.brick, me.paint, limit());
    }
}
