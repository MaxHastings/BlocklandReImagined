// The Duplicator, after the v20 Duplicator Add-On. It runs on the host.
//
//   /dup (or /duplicator, or /duplorcator, the v20 Add-On's own name for
//   it) puts the Duplicator in your hand. Anyone may, as in v20.
//   Clicking a brick with it copies that brick and everything built on it:
//   every brick joined to it through studs, never below it, that you may
//   build on. The copy shows as a ghost over the original.
//   The brick keys move and turn the copy, the plant key plants it, and
//   Ctrl+Z takes a planted copy back. Planting follows the server's normal
//   plant rules; a copy that cannot all be planted plants nothing.

// Most bricks one copy may hold.
fn limit() {
    2000
}

fn tool() {
    "duplicator-tool:weapon/duplicator"
}

fn cmd_dup(player) {
    give_item(player, tool(), true);
}

fn cmd_duplicator(player) {
    cmd_dup(player);
}

fn cmd_duplorcator(player) {
    cmd_dup(player);
}

// The Duplicator's swing (`onFire` of its image).
fn cmd_select(player) {
    let hit = aim();
    if hit == () || hit.brick == () {
        return;
    }
    copy_build(player, hit.brick, limit(), true, tool());
}
