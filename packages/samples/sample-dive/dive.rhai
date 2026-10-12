// Players choose their body; the Diver's controller does the rest.
fn cmd_diver(player) {
    set_archetype(player, "sample-dive:archetype/diver");
}
fn cmd_blockhead(player) {
    set_archetype(player, "v20.player.playerstandardarmor");
}
