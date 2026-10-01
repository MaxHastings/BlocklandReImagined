// Sun shadow casters: scene vertices and instances into one cascade layer.
// gap.x is a tenth of a world unit in this cascade's shadow depth.
struct Caster { light:mat4x4<f32>, gap:vec4<f32> };
@group(0) @binding(0) var<uniform> caster:Caster;
@group(0) @binding(1) var tiled:sampler;
// Occluder passes only: this cascade's finished caster depth.
@group(0) @binding(2) var caster_depth:texture_depth_2d;
@group(1) @binding(0) var layer0:texture_2d<f32>;
@group(1) @binding(15) var<uniform> material:array<vec4<f32>,5>;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) alpha:f32,
    // Signed distance past the instance's clip plane; cut below zero.
    @location(2) clip:f32,
};
@vertex fn vs_main(@location(0) local_position:vec3<f32>,@location(1) local_normal:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) lightmap_uv:vec2<f32>,@location(4) local_color:vec4<f32>,
    @location(5) m0:vec4<f32>,@location(6) m1:vec4<f32>,@location(7) m2:vec4<f32>,@location(8) m3:vec4<f32>,@location(9) tint:vec4<f32>,
    @location(11) clip:vec4<f32>)->VertexOut {
    let model=mat4x4<f32>(m0,m1,m2,m3);
    var out:VertexOut;
    let world=model*vec4<f32>(local_position,1.0);
    out.position=caster.light*world;
    out.clip=dot(clip.xyz,world.xyz)+clip.w;
    out.uv=uv;
    out.alpha=local_color.a*tint.a;
    return out;
}
// Alpha-masked materials cut holes in their shadow as they do on screen.
@fragment fn fs_masked(v:VertexOut) {
    if v.clip<0.0 {discard;}
    if textureSample(layer0,tiled,v.uv).a*v.alpha<=material[0].y {discard;}
}
// Opaque casters cut by a clip plane (a body part way through a portal);
// every other opaque caster draws depth only.
@fragment fn fs_clipped(v:VertexOut) {
    if v.clip<0.0 {discard;}
}
// An occluder only matters beyond the caster along the sun. Keeping the
// nearest occluder past it (not the nearest to the sun, which may be a
// ceiling over the player) records the first surface its shadow reaches.
fn beyond_caster(v:VertexOut) {
    if v.clip<0.0 {discard;}
    let nearest=textureLoad(caster_depth,vec2<i32>(v.position.xy),0);
    if v.position.z<=nearest+caster.gap.x {discard;}
}
@fragment fn fs_occluder(v:VertexOut) {
    beyond_caster(v);
}
@fragment fn fs_occluder_masked(v:VertexOut) {
    let alpha=textureSample(layer0,tiled,v.uv).a*v.alpha;
    beyond_caster(v);
    if alpha<=material[0].y {discard;}
}
// Lamp face clear: a triangle over the tile's viewport at depth 1.
@vertex fn vs_clear(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let corner=vec2<f32>(f32((i<<1u)&2u),f32(i&2u));
    return vec4<f32>(corner*2.0-vec2<f32>(1.0),1.0,1.0);
}
