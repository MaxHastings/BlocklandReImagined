struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
}
@vertex fn vs_main(@location(0) position: vec4<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>) -> Output {
    var o: Output;
    o.position = position; o.normal = normal; o.color = color;
    return o;
}
@fragment fn fs_main(i: Output) -> @location(0) vec4<f32> {
    let diffuse = max(dot(normalize(i.normal), normalize(vec3(-0.5, 0.85, 0.65))), 0.0);
    return vec4(i.color.rgb * (0.3 + diffuse * 0.7), i.color.a);
}
