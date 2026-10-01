// The probe's box holds its whole build: 1000 by 1000 one-stud plates.
fn low() {
    [0.0, 0.0, 0.0]
}

fn high() {
    [500.0, 0.2, 500.0]
}

fn cmd_sel(player) {
    copy_box(player, low(), high(), 1000000, "advanced-duplicator-tool:weapon/advanced-duplicator");
}

fn cmd_cut(player) {
    cut_copy(player);
}

fn cmd_supercut(player) {
    super_cut(player, low(), high());
}
