// Dynamic textured triangles: a texel combined with a per-vertex colour the way
// a fixed-function pipeline would, with optional linear distance fog and an
// alpha test. See dynamic_sprites.rs.
struct Globals { view_projection:mat4x4<f32> };
struct Batch { modes:vec4<f32>, constant:vec4<f32>, fog:vec4<f32>, fog_color:vec4<f32> };
@group(0) @binding(0) var<uniform> globals:Globals;
@group(1) @binding(0) var<uniform> batch:Batch;
@group(1) @binding(1) var picture:texture_2d<f32>;
@group(1) @binding(2) var picture_sampler:sampler;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) color:vec4<f32>,
    @location(1) uv:vec2<f32>,
    @location(2) uv2:vec2<f32>,
    @location(3) depth:f32,
};
@vertex
fn vs_main(@location(0) position:vec3<f32>,@location(1) color:vec4<f32>,@location(2) uv:vec2<f32>,@location(3) uv2:vec2<f32>)->VertexOut {
    var out:VertexOut;
    out.position=globals.view_projection*vec4<f32>(position,1.0);
    out.color=color;
    out.uv=uv;
    out.uv2=uv2;
    out.depth=out.position.w;
    return out;
}
fn combine(code:f32,texel:vec4<f32>,vertex:vec4<f32>,share:f32)->vec4<f32> {
    if code<0.5 { return texel*vertex; }
    if code<1.5 { return texel*vertex*2.0; }
    if code<2.5 { return texel*vertex*4.0; }
    if code<3.5 { return texel; }
    if code<4.5 { return vertex; }
    return texel*share+vertex*(1.0-share);
}
@fragment
fn fs_main(in:VertexOut)->@location(0) vec4<f32> {
    var vertex=in.color;
    if batch.fog.x>0.5 { vertex=batch.constant; }
    var texel=textureSample(picture,picture_sampler,in.uv);
    texel=vec4<f32>(display_color(texel.rgb),texel.a);
    if batch.modes.z>0.5 {
        var other=textureSample(picture,picture_sampler,in.uv2);
        other=vec4<f32>(display_color(other.rgb),other.a);
        texel=mix(other,texel,clamp(in.color.a,0.0,1.0));
        vertex=vec4<f32>(vertex.rgb,1.0);
    }
    var rgb=combine(batch.modes.x,texel,vertex,in.color.a).rgb;
    let alpha=combine(batch.modes.y,texel,vertex,in.color.a).a;
    if batch.modes.w>=0.0 && alpha<batch.modes.w { discard; }
    if batch.fog.w>0.5 {
        let share=clamp((batch.fog.z-in.depth)/max(batch.fog.z-batch.fog.y,0.0001),0.0,1.0);
        rgb=mix(batch.fog_color.rgb,rgb,share);
    }
    return vec4<f32>(output_color(clamp(rgb,vec3<f32>(0.0),vec3<f32>(1.0))),clamp(alpha,0.0,1.0));
}
