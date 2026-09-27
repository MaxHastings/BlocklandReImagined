// Sun shadow casters: scene vertices and instances into one cascade layer.
@group(0) @binding(0) var<uniform> light:mat4x4<f32>;
@group(0) @binding(1) var tiled:sampler;
@group(1) @binding(0) var layer0:texture_2d<f32>;
@group(1) @binding(15) var<uniform> material:array<vec4<f32>,4>;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) alpha:f32,
};
@vertex fn vs_main(@location(0) local_position:vec3<f32>,@location(1) local_normal:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) lightmap_uv:vec2<f32>,@location(4) local_color:vec4<f32>,
    @location(5) m0:vec4<f32>,@location(6) m1:vec4<f32>,@location(7) m2:vec4<f32>,@location(8) m3:vec4<f32>,@location(9) tint:vec4<f32>)->VertexOut {
    let model=mat4x4<f32>(m0,m1,m2,m3);
    var out:VertexOut;
    out.position=light*model*vec4<f32>(local_position,1.0);
    out.uv=uv;
    out.alpha=local_color.a*tint.a;
    return out;
}
// Alpha-masked materials cut holes in their shadow as they do on screen.
@fragment fn fs_masked(v:VertexOut) {
    if textureSample(layer0,tiled,v.uv).a*v.alpha<=material[0].y {discard;}
}
