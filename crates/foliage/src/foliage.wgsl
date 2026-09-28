struct Camera {vp:mat4x4<f32>, position:vec4<f32>, right:vec4<f32>, time_fog:vec4<f32>};
struct Plant {position_width:vec4<f32>,shape:vec4<f32>,sway:vec4<f32>,light:vec4<f32>,top:vec4<f32>,bottom:vec4<f32>,fade:vec4<f32>,alpha:vec4<f32>};
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<storage,read> plants:array<Plant>;
@group(1) @binding(0) var image:texture_2d<f32>;
@group(1) @binding(1) var samp:sampler;
struct Output {@builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) color:vec4<f32>,@location(2) cutoff:f32};
fn phase(initial:f32,rate:f32)->f32{return floor((initial+rate*camera.time_fog.x)%720.0)*6.28318530718/720.0;}
@vertex fn vs_main(@builtin(vertex_index) vertex:u32,@location(0) index:u32)->Output {
 let p=plants[index];let corners=array<vec2<f32>,6>(vec2(-0.5,0.),vec2(0.5,0.),vec2(-0.5,1.),vec2(-0.5,1.),vec2(0.5,0.),vec2(0.5,1.));let c=corners[vertex];
 var right=vec3(cos(p.shape.y),0.,-sin(p.shape.y));if p.shape.w>0.5 {right=camera.right.xyz;}
 let front=cross(vec3(0.,1.,0.),right);let angle=phase(p.sway.z,p.sway.w);let sway=p.sway.xy*vec2(cos(angle),sin(angle))*p.alpha.z;
 let world=p.position_width.xyz+right*(c.x*p.position_width.w+c.y*sway.x)+vec3(0.,c.y*p.shape.x,0.)+front*(c.y*sway.y);
 let distance=length(p.position_width.xyz-camera.position.xyz);var opacity=1.;if distance<p.fade.x{opacity=clamp(1.-(p.fade.x-distance)/max(p.fade.z,0.00001),0.,1.);}else if distance>p.fade.y{opacity=clamp(1.-(distance-p.fade.y)/max(p.fade.w,0.00001),0.,1.);}
 let fog=1.-clamp((distance-camera.time_fog.y)/max(camera.time_fog.z-camera.time_fog.y,0.00001),0.,1.);opacity=min(opacity,fog);
 let luminance=select(1.,(p.light.z+p.light.w)*0.5+(p.light.w-p.light.z)*0.5*cos(phase(p.light.x,p.light.y)),p.alpha.w>0.5);
 let color=mix(p.bottom,p.top,c.y);var out:Output;out.position=camera.vp*vec4(world,1.);out.uv=vec2(select(c.x+0.5,0.5-c.x,p.shape.z>0.5),1.-c.y);out.color=vec4(color.rgb*luminance,color.a*mix(min(p.alpha.x,opacity),opacity,c.y));out.cutoff=p.alpha.y;return out;
}
@fragment fn fs_main(input:Output)->@location(0) vec4<f32>{let texel=textureSample(image,samp,input.uv);let alpha=texel.a*input.color.a;if alpha<=input.cutoff{discard;}return vec4(output_color(display_color(texel.rgb)*input.color.rgb),alpha);}
