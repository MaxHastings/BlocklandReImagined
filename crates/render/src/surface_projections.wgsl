// Projected surfaces: given triangles redrawn with a projected picture. See
// surface_projections.rs.
struct Globals { view_projection:mat4x4<f32>, camera:vec4<f32> };
struct Batch {
    world_to_texture:mat4x4<f32>,
    gradient:mat4x4<f32>,
    modes:vec4<f32>,
    forward:vec4<f32>,
    extra:vec4<f32>,
    direction:mat3x3<f32>,
    fog:vec4<f32>,
    fog_color:vec4<f32>,
};
@group(0) @binding(0) var<uniform> globals:Globals;
@group(1) @binding(0) var<uniform> batch:Batch;
@group(1) @binding(1) var picture:texture_2d<f32>;
@group(1) @binding(2) var gradient_picture:texture_2d<f32>;
@group(1) @binding(3) var mask_picture:texture_2d<f32>;
@group(1) @binding(4) var base_picture:texture_2d<f32>;
@group(1) @binding(5) var picture_sampler:sampler;
@group(1) @binding(6) var clamp_sampler:sampler;
@group(1) @binding(7) var repeat_sampler:sampler;
@group(1) @binding(8) var cube_picture:texture_cube<f32>;

struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) world:vec3<f32>,
    @location(1) normal:vec3<f32>,
    @location(2) uv:vec2<f32>,
    @location(3) fade:f32,
    @location(4) depth:f32,
};
@vertex
fn vs_main(@location(0) position:vec3<f32>,@location(1) normal:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) fade:f32)->VertexOut {
    var out:VertexOut;
    out.position=globals.view_projection*vec4<f32>(position,1.0);
    out.world=position;
    out.normal=normal;
    out.uv=uv;
    out.fade=fade;
    out.depth=out.position.w;
    return out;
}

@fragment
fn fs_main(in:VertexOut)->@location(0) vec4<f32> {
    let tc=batch.world_to_texture*vec4<f32>(in.world,1.0);
    var st=tc.xy;
    if batch.modes.x>0.5 {
        if tc.z<=0.0 { discard; }
        st=tc.xy/tc.z;
    }
    var texel=textureSampleLevel(picture,picture_sampler,st,0.0);
    if batch.extra.w>0.5 {
        // The view ray reflected off the (viewer-facing) normal, in the cube's axes.
        let incident=normalize(in.world-globals.camera.xyz);
        var n=normalize(in.normal);
        if dot(n,incident)>0.0 { n=-n; }
        let r=incident-2.0*dot(incident,n)*n;
        texel=textureSampleLevel(cube_picture,clamp_sampler,batch.direction*r,0.0);
    }
    texel=vec4<f32>(display_color(texel.rgb),texel.a);
    // The surface's own picture first, when it is blended in.
    if batch.extra.y>0.5 {
        var base=textureSampleLevel(base_picture,repeat_sampler,in.uv,0.0);
        base=vec4<f32>(display_color(base.rgb),base.a);
        if batch.extra.y<1.5 {
            texel=vec4<f32>(clamp(texel.rgb*base.rgb*2.0,vec3<f32>(0.0),vec3<f32>(1.0)),texel.a);
        } else {
            texel=vec4<f32>(texel.rgb*texel.a+base.rgb*(1.0-texel.a),texel.a);
        }
    }
    var fade=1.0;
    if batch.extra.x>0.5 { fade=fade*clamp(in.fade,0.0,1.0); }
    if batch.modes.z>0.5 {
        let g=batch.gradient*vec4<f32>(in.world,1.0);
        fade=fade*textureSampleLevel(gradient_picture,clamp_sampler,vec2<f32>(0.0,g.y),0.0).a;
    }
    if batch.modes.w>0.5 {
        fade=fade*textureSampleLevel(mask_picture,repeat_sampler,in.uv,0.0).a;
    }
    if batch.forward.w>0.5 {
        let n=normalize(in.normal);
        fade=fade*max(0.0,-dot(n,batch.forward.xyz));
    }
    let grey=127.0/255.0;
    var color:vec4<f32>;
    if batch.modes.y<0.5 {
        color=vec4<f32>(mix(vec3<f32>(grey),texel.rgb,fade),1.0);
    } else if batch.modes.y<1.5 {
        color=vec4<f32>(texel.rgb,texel.a*fade);
    } else {
        color=vec4<f32>(texel.rgb*fade,1.0);
    }
    if batch.extra.z>0.5 {
        let share=clamp((batch.fog.y-in.depth)/max(batch.fog.y-batch.fog.x,0.0001),0.0,1.0);
        var neutral=batch.fog_color.rgb;
        if batch.modes.y<0.5 { neutral=vec3<f32>(grey); }
        else if batch.modes.y>1.5 { neutral=vec3<f32>(0.0); }
        color=vec4<f32>(mix(neutral,color.rgb,share),color.a);
    }
    return vec4<f32>(output_color(clamp(color.rgb,vec3<f32>(0.0),vec3<f32>(1.0))),clamp(color.a,0.0,1.0));
}
