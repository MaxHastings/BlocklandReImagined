// Projected textures: a picture landed on the geometry the depth buffer holds, inside
// a volume. See projected_textures.rs.
struct Globals {
    inverse_view_projection:mat4x4<f32>,
    view_projection:mat4x4<f32>,
    camera:vec4<f32>,
    size:vec4<f32>,
    fog:vec4<f32>,
    fog_color:vec4<f32>,
};
struct Projection {
    world_to_texture:mat4x4<f32>,
    gradient:mat4x4<f32>,
    planes:array<vec4<f32>,6>,
    modes:vec4<f32>,
    forward:vec4<f32>,
    extra:vec4<u32>,
};
@group(0) @binding(0) var<uniform> globals:Globals;
@group(0) @binding(1) var scene_depth:texture_depth_2d;
@group(0) @binding(2) var surface_info:texture_2d<f32>;
@group(1) @binding(0) var<uniform> projection:Projection;
@group(1) @binding(1) var picture:texture_2d<f32>;
@group(1) @binding(2) var gradient_picture:texture_2d<f32>;
@group(1) @binding(3) var picture_sampler:sampler;
@group(1) @binding(4) var gradient_sampler:sampler;

@vertex
fn vs_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4<f32> {
    let x=f32((index<<1u)&2u);
    let y=f32(index&2u);
    return vec4<f32>(x*2.0-1.0,1.0-y*2.0,0.0,1.0);
}

fn world_at(pixel:vec2<f32>,depth:f32)->vec3<f32> {
    let ndc=vec2<f32>(pixel.x/globals.size.x*2.0-1.0,1.0-pixel.y/globals.size.y*2.0);
    let h=globals.inverse_view_projection*vec4<f32>(ndc,depth,1.0);
    return h.xyz/h.w;
}

@fragment
fn fs_main(@builtin(position) frag:vec4<f32>)->@location(0) vec4<f32> {
    let depth=textureLoad(scene_depth,vec2<i32>(frag.xy),0);
    let world=world_at(frag.xy,max(depth,0.000001));
    // Derivatives need uniform control flow, so the facing is found before any discard.
    var n=normalize(cross(dpdx(world),dpdy(world)));
    if dot(n,globals.camera.xyz-world)<0.0 { n=-n; }
    if depth<=0.0 { discard; }
    // The volume.
    let planes=i32(projection.modes.x);
    for (var i=0;i<planes;i=i+1) {
        let plane=projection.planes[i];
        if dot(plane.xyz,world)-plane.w<0.0 { discard; }
    }
    let tc=projection.world_to_texture*vec4<f32>(world,1.0);
    var uv=tc.xy;
    if projection.modes.y>0.5 {
        if tc.z<=0.0 { discard; }
        uv=tc.xy/tc.z;
    }
    var info=vec4<f32>(1.0);
    if globals.size.z>0.5 {
        info=textureLoad(surface_info,vec2<i32>(frag.xy),0);
        let category=u32(round(info.r*255.0));
        if (category&projection.extra.x)==0u { discard; }
    }
    var texel=textureSampleLevel(picture,picture_sampler,uv,0.0);
    texel=vec4<f32>(display_color(texel.rgb),texel.a);
    var fade=1.0;
    if projection.modes.w>0.5 {
        let g=projection.gradient*vec4<f32>(world,1.0);
        fade=fade*textureSampleLevel(gradient_picture,gradient_sampler,vec2<f32>(0.0,g.y),0.0).a;
    }
    if projection.forward.w>0.5 {
        // The surface's facing from the screen-space slope of the world position, turned to
        // face the camera.
        fade=fade*max(0.0,-dot(n,projection.forward.xyz));
    }
    if projection.extra.y!=0u { fade=fade*info.a; }
    let grey=127.0/255.0;
    var color:vec4<f32>;
    if projection.modes.z<0.5 {
        color=vec4<f32>(mix(vec3<f32>(grey),texel.rgb,fade),1.0);
    } else if projection.modes.z<1.5 {
        color=vec4<f32>(texel.rgb,texel.a*fade);
    } else {
        color=vec4<f32>(texel.rgb*fade,1.0);
    }
    if globals.size.w>0.5 {
        // Distance fog on the projection pass: toward a neutral colour by blend.
        let view_depth=(globals.view_projection*vec4<f32>(world,1.0)).w;
        let share=clamp((globals.fog.y-view_depth)/max(globals.fog.y-globals.fog.x,0.0001),0.0,1.0);
        var neutral=globals.fog_color.rgb;
        if projection.modes.z<0.5 { neutral=vec3<f32>(grey); }
        else if projection.modes.z>1.5 { neutral=vec3<f32>(0.0); }
        color=vec4<f32>(mix(neutral,color.rgb,share),color.a);
    }
    return vec4<f32>(output_color(clamp(color.rgb,vec3<f32>(0.0),vec3<f32>(1.0))),clamp(color.a,0.0,1.0));
}
