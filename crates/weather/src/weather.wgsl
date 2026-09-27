@group(0) @binding(0) var<uniform> view_projection:mat4x4<f32>;
@group(1) @binding(0) var atlas:texture_2d_array<f32>;
@group(1) @binding(1) var atlas_sampler:sampler;
struct Out {@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) color:vec4<f32>,@location(2) @interpolate(flat) layer:i32}
@vertex fn vs_main(@builtin(vertex_index) v:u32,@location(0) position:vec4<f32>,@location(1) right:vec4<f32>,@location(2) up:vec4<f32>,@location(3) uv:vec4<f32>,@location(4) color:vec4<f32>)->Out {
    let corners=array<vec2<f32>,6>(vec2(-1.,1.),vec2(1.,1.),vec2(1.,-1.),vec2(-1.,1.),vec2(1.,-1.),vec2(-1.,-1.));let c=corners[v];
    var out:Out;out.position=view_projection*vec4(position.xyz+right.xyz*c.x+up.xyz*c.y,1.);out.uv=mix(uv.xy,uv.zw,vec2(c.x*0.5+0.5,0.5-c.y*0.5));out.uv*=vec2(right.w,up.w);out.layer=i32(position.w);out.color=color;return out;
}
@fragment fn fs_main(input:Out)->@location(0) vec4<f32> {let color=textureSample(atlas,atlas_sampler,input.uv,input.layer)*input.color;if color.a<1./255. {discard;}return color;}
