// Players choose their body; the archetype sets the Trooper's speeds and
// gravity, and its controller steers it in the air.
fn cmd_trooper(player) {
    set_archetype(player, "sample-trooper:archetype/trooper");
}
fn cmd_blockhead(player) {
    set_archetype(player, "v20.player.playerstandardarmor");
}
