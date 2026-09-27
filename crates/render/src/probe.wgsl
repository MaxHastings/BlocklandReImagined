@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array(vec2(-0.8, -0.8), vec2(0.8, -0.8), vec2(0.0, 0.8));
    return vec4(positions[index], 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> {
    return vec4(0.9, 0.35, 0.05, 1.0);
}
