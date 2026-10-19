// One picture of the sun's lens flare (bri_content::environment::FlareElement),
// added to the frame: a quad at `place.xy` (clip space) of half size
// `place.zw`, its picture times color.rgb times color.a.
struct Element { place:vec4<f32>, color:vec4<f32> };
@group(0) @binding(0) var<uniform> element:Element;
@group(0) @binding(1) var picture:texture_2d<f32>;
@group(0) @binding(2) var picture_sampler:sampler;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) uv:vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) index:u32)->VertexOut {
    var corners=array<vec2<f32>,6>(
        vec2<f32>(-1.0,-1.0),vec2<f32>(1.0,-1.0),vec2<f32>(1.0,1.0),
        vec2<f32>(-1.0,-1.0),vec2<f32>(1.0,1.0),vec2<f32>(-1.0,1.0));
    let c=corners[index];
    var out:VertexOut;
    out.position=vec4<f32>(element.place.xy+c*element.place.zw,0.0,1.0);
    out.uv=vec2<f32>(c.x*0.5+0.5,0.5-c.y*0.5);
    return out;
}
@fragment
fn fs_main(in:VertexOut)->@location(0) vec4<f32> {
    let texel=textureSample(picture,picture_sampler,in.uv);
    let rgb=texel.rgb*texel.a*element.color.rgb*element.color.a;
    return vec4<f32>(output_color(rgb),0.0);
}
