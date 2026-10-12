// Players choose their body; the Wall Kicker's controller does the rest.
fn cmd_wallkick(player) {
    set_archetype(player, "sample-wall-kick:archetype/kicker");
}
fn cmd_blockhead(player) {
    set_archetype(player, "v20.player.playerstandardarmor");
}
