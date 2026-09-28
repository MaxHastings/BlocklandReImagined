// Stress Lab Strata: a generated world of grass, dirt and stone over
// bedrock, with coal, copper and gold veins deeper down.
//
// The engine calls generate_chunk(cx, cz) for each chunk near a player and
// expects [[x, y, z, material], ...] in voxel coordinates. Materials index
// world.json's list: 0 grass, 1 dirt, 2 stone, 3 coal, 4 copper, 5 gold,
// 6 bedrock.

fn surface(s, x, z) {
    let broad = noise(s, x * 0.07, z * 0.07) * 3.5;
    let fine = noise(s + 1, x * 0.21, z * 0.21) * 1.2;
    (6.0 + broad + fine).floor().to_int()
}

fn material(s, x, y, z, top) {
    if y == 0 { return 6; }
    if y == top { return 0; }
    if y >= top - 1 { return 1; }
    let r = hash3(s, x, y, z);
    if y <= 2 && r < 0.05 { return 5; }
    if y <= 4 && r < 0.09 { return 4; }
    if r < 0.12 { return 3; }
    2
}

fn generate_chunk(cx, cz) {
    let s = seed();
    let out = [];
    for lx in 0..8 {
        for lz in 0..8 {
            let x = cx * 8 + lx;
            let z = cz * 8 + lz;
            let top = surface(s, x, z);
            for y in 0..=top {
                out.push([x, y, z, material(s, x, y, z, top)]);
            }
        }
    }
    out
}
