// Screen-space ambient occlusion over the finished world. Each pixel's
// position and normal come from the depth buffer alone; sample points in a
// hemisphere above it that lie behind nearer geometry count as blocked, and
// the share blocked darkens the pixel, so creases, the feet of walls and the
// undersides of ledges deepen. Written as a multiply over the frame; the sky
// (depth 0) and distant pixels are left alone.
//
// view_projection and inverse map to and from clip space (reversed 0..1
// depth); eye.xyz is the camera; params is radius (world units), strength,
// fade start and fade end (distance from the eye); size.xy is the target in
// pixels, size.z the fog's below-horizon flag; atmosphere is the camera's
// fog (fog.wgsl), which the occlusion fades out under.
struct Occlusion {
    view_projection:mat4x4<f32>, inverse:mat4x4<f32>,
    eye:vec4<f32>, params:vec4<f32>, size:vec4<f32>, atmosphere:vec4<f32>,
};
@group(0) @binding(0) var<uniform> occlusion:Occlusion;
@group(0) @binding(1) var depth_texture:DEPTH_TEXTURE;
@group(0) @binding(2) var exclusion_mask:MASK_TEXTURE;
const TAPS:u32=12u;
struct VertexOut { @builtin(position) position:vec4<f32> };
@vertex
fn vs_main(@builtin(vertex_index) index:u32)->VertexOut {
    let uv=vec2<f32>(f32((index<<1u)&2u),f32(index&2u));
    var out:VertexOut;
    out.position=vec4<f32>(uv*2.0-1.0,0.0,1.0);
    return out;
}
fn depth_at(pixel:vec2<i32>,sample:u32)->f32 {
    let size=vec2<i32>(occlusion.size.xy);
    return textureLoad(depth_texture,clamp(pixel,vec2<i32>(0),size-vec2<i32>(1)),i32(sample));
}
fn world_at(pixel:vec2<i32>,depth:f32)->vec3<f32> {
    let uv=(vec2<f32>(pixel)+0.5)/occlusion.size.xy;
    let clip=vec4<f32>(uv.x*2.0-1.0,1.0-uv.y*2.0,depth,1.0);
    let world=occlusion.inverse*clip;
    return world.xyz/world.w;
}
// Per-pixel rotation, interleaved gradient noise.
fn noise(pixel:vec2<f32>)->f32 {
    return fract(52.9829189*fract(dot(pixel,vec2<f32>(0.06711056,0.00583715))));
}
@fragment
fn fs_main(in:VertexOut,@builtin(sample_index) sample:u32)->@location(0) vec4<f32> {
    let pixel=vec2<i32>(in.position.xy);
    let depth=depth_at(pixel,sample);
    if depth<=0.0 || textureLoad(exclusion_mask,pixel,i32(sample)).r>0.5 {return vec4<f32>(1.0);}
    let p=world_at(pixel,depth);
    let range=distance(p,occlusion.eye.xyz);
    let fog=fog_along(p-occlusion.eye.xyz,occlusion.atmosphere,occlusion.size.z);
    let fade=(1.0-smoothstep(occlusion.params.z,occlusion.params.w,range))*(1.0-fog);
    if fade<=0.0 {return vec4<f32>(1.0);}
    // The normal from the nearer neighbour on each axis, so an edge does
    // not bend it.
    let px=world_at(pixel+vec2<i32>(1,0),depth_at(pixel+vec2<i32>(1,0),sample));
    let nx=world_at(pixel-vec2<i32>(1,0),depth_at(pixel-vec2<i32>(1,0),sample));
    let py=world_at(pixel+vec2<i32>(0,1),depth_at(pixel+vec2<i32>(0,1),sample));
    let ny=world_at(pixel-vec2<i32>(0,1),depth_at(pixel-vec2<i32>(0,1),sample));
    let dx=select(p-nx,px-p,abs(distance(px,occlusion.eye.xyz)-range)<abs(distance(nx,occlusion.eye.xyz)-range));
    let dy=select(p-ny,py-p,abs(distance(py,occlusion.eye.xyz)-range)<abs(distance(ny,occlusion.eye.xyz)-range));
    var n=normalize(cross(dy,dx));
    if dot(n,occlusion.eye.xyz-p)<0.0 {n=-n;}
    let up=select(vec3<f32>(0.0,1.0,0.0),vec3<f32>(1.0,0.0,0.0),abs(n.y)>0.9);
    let t=normalize(cross(up,n));
    let b=cross(n,t);
    // Sample points grow with the radius a little for distant pixels, so the
    // effect keeps a steady size on screen without becoming grain up close.
    let radius=occlusion.params.x*(1.0+range*0.01);
    let spin=noise(in.position.xy)*6.2831853;
    var blocked=0.0;
    for(var i=0u;i<TAPS;i+=1u) {
        let u=(f32(i)+0.5)/f32(TAPS);
        let z=1.0-u*0.9;
        let r=sqrt(max(1.0-z*z,0.0));
        let phi=f32(i)*2.3999632+spin;
        let reach=mix(0.2,1.0,u*u);
        let direction=(t*cos(phi)*r+b*sin(phi)*r+n*z)*reach*radius;
        let at=p+direction+n*0.02*radius;
        let clip=occlusion.view_projection*vec4<f32>(at,1.0);
        if clip.w<=0.0 {continue;}
        let ndc=clip.xyz/clip.w;
        let screen=vec2<f32>(ndc.x*0.5+0.5,0.5-ndc.y*0.5)*occlusion.size.xy;
        if any(screen<vec2<f32>(0.0)) || any(screen>=occlusion.size.xy) {continue;}
        let sample_pixel=vec2<i32>(screen);
        let found=depth_at(sample_pixel,sample);
        if found<=0.0 {continue;}
        let seen=distance(world_at(sample_pixel,found),occlusion.eye.xyz);
        let wanted=distance(at,occlusion.eye.xyz);
        let behind=wanted-seen;
        // Blocked when geometry stands in front of the sample point, but
        // not when it is a far nearer object the point merely sits behind.
        if behind>0.03*radius {
            blocked+=1.0-smoothstep(radius*0.6,radius*1.8,behind);
        }
    }
    let open=1.0-blocked/f32(TAPS);
    let shade=mix(1.0,open,clamp(occlusion.params.y,0.0,1.0)*fade);
    return vec4<f32>(vec3<f32>(shade),1.0);
}
