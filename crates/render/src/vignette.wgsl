// A full-screen vignette (bri_content::atmosphere::Vignette): the colour
// weighs in toward the screen's edges, over the frame (alpha) or times it
// (multiply). color is display-encoded, its alpha the strength; params.x is
// the width over height, params.y 1 in multiply mode.
struct Vignette { color:vec4<f32>, params:vec4<f32> };
@group(0) @binding(0) var<uniform> vignette:Vignette;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) uv:vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) index:u32)->VertexOut {
    let uv=vec2<f32>(f32((index<<1u)&2u),f32(index&2u));
    var out:VertexOut;
    out.position=vec4<f32>(uv*2.0-1.0,0.0,1.0);
    out.uv=uv*2.0-1.0;
    return out;
}
// 0 at the centre, 1 at the corners of the screen's height-fitted circle.
fn vignette_weight(uv:vec2<f32>)->f32 {
    let aspect=max(vignette.params.x,0.01);
    let p=vec2<f32>(uv.x*sqrt(aspect),uv.y/sqrt(aspect));
    let r=length(p)/sqrt(aspect+1.0/aspect);
    return smoothstep(0.35,1.0,r*1.15)*clamp(vignette.color.a,0.0,1.0);
}
@fragment
fn fs_main(in:VertexOut)->@location(0) vec4<f32> {
    let w=vignette_weight(in.uv);
    if vignette.params.y>0.5 {
        return vec4<f32>(output_color(mix(vec3<f32>(1.0),vignette.color.rgb,w)),1.0);
    }
    return vec4<f32>(output_color(vignette.color.rgb),w);
}
