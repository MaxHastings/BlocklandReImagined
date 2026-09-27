struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) lightmap_uv:vec2<f32>,
};
@group(0) @binding(0) var diffuse_sampler:sampler;
@group(0) @binding(1) var diffuse:texture_2d<f32>;
@group(0) @binding(2) var lightmap:texture_2d<f32>;
@group(0) @binding(3) var lightmap_sampler:sampler;
@vertex fn vs_main(@location(0) position:vec4<f32>,@location(1) uv:vec2<f32>,@location(2) lightmap_uv:vec2<f32>)->VertexOut {
    var out:VertexOut;out.position=position;out.uv=uv;out.lightmap_uv=lightmap_uv;return out;
}
@fragment fn fs_main(input:VertexOut)->@location(0) vec4<f32> {
    let albedo=textureSample(diffuse,diffuse_sampler,input.uv);
    let illumination=textureSample(lightmap,lightmap_sampler,input.lightmap_uv).rgb;
    return vec4<f32>(linear_color(display_color(albedo.rgb)*illumination),albedo.a);
}
