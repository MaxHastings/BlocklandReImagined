// Mirror and window surfaces (reflection.rs). A live mirror shows its
// plane's reflection, rendered from the mirrored view with x flipped: a
// screen position u samples it at mirror_u - u. A live window's view lines
// up with the screen: u samples it at u. An echo shows a plane's last
// picture where that surface point lay in the view it was seen in (a
// surface seen deeper than the passes reach). Others show their fallback
// colour (silver on a mirror).
struct Frame {
    view_projection:mat4x4<f32>, eye:vec4<f32>, fog_color:vec4<f32>,
    // Fog start, visible distance, unused, fog enabled.
    atmosphere:vec4<f32>,
    screen:vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame:Frame;
// reproject: the echoed plane's parent view; sample: mirror_u, then 1
// live, 2 echo, 0 fallback, then 1 flipped; parent and viewport: the parent
// view's and the plane's viewports in target pixels; size: the target's.
struct Slot {
    reproject:mat4x4<f32>, sample:vec4<f32>, parent:vec4<f32>, viewport:vec4<f32>, size:vec4<f32>,
};
@group(1) @binding(0) var<uniform> slot:Slot;
@group(1) @binding(1) var picture:texture_2d<f32>;
@group(1) @binding(2) var picture_sampler:sampler;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) world:vec3<f32>,
    @location(1) tint:vec4<f32>,
    @location(2) fallback:vec3<f32>,
};
@vertex fn vs_main(@location(0) position:vec3<f32>,@location(1) tint:vec4<f32>,@location(2) fallback:vec4<f32>)->VertexOut {
    var out:VertexOut;
    out.position=frame.view_projection*vec4<f32>(position,1.0);
    out.world=position;
    out.tint=tint;
    out.fallback=fallback.rgb;
    return out;
}
fn fog_amount(position:vec3<f32>)->f32 {
    return fog_along(position-frame.eye.xyz,frame.atmosphere);
}
// Where the echoed plane's picture holds this point, held to the part it
// drew (the edge stretches over what that view did not see), or a
// negative u behind that view's eye.
fn echo_uv(world:vec3<f32>)->vec2<f32> {
    let clip=slot.reproject*vec4<f32>(world,1.0);
    if clip.w<=0.0 {return vec2<f32>(-1.0);}
    let ndc=clip.xy/clip.w;
    let x=slot.parent.x+(ndc.x+1.0)*0.5*slot.parent.z;
    let y=slot.parent.y+(1.0-ndc.y)*0.5*slot.parent.w;
    let v=slot.viewport;
    let across=select(x,slot.sample.x*slot.size.x-x,slot.sample.z>0.5);
    let texel=clamp(vec2<f32>(across,y),v.xy+vec2<f32>(0.5),v.xy+v.zw-vec2<f32>(0.5));
    return texel/slot.size.xy;
}
@fragment fn fs_main(v:VertexOut)->@location(0) vec4<f32> {
    let screen=v.position.xy/frame.screen.xy;
    var uv=vec2<f32>(select(screen.x,slot.sample.x-screen.x,slot.sample.z>0.5),screen.y);
    var live=slot.sample.y>0.5;
    if slot.sample.y>1.5 {
        uv=echo_uv(v.world);
        live=uv.x>=0.0;
    }
    let stored=textureSampleLevel(picture,picture_sampler,uv,0.0).rgb;
    if live {
        // The reflection already went through output_color (fog and
        // colour-vision assistance included): tint it and store it the same
        // way, without applying them twice.
        var display=stored;
        if OUTPUT_ENCODED==0u {display=display_color(stored);}
        display*=v.tint.rgb;
        if OUTPUT_ENCODED==0u {return vec4<f32>(linear_color(display),v.tint.a);}
        return vec4<f32>(display,v.tint.a);
    }
    let silver=mix(v.fallback*v.tint.rgb,frame.fog_color.rgb,fog_amount(v.world));
    return vec4<f32>(output_color(silver),v.tint.a);
}
