// Mirror surfaces (reflection.rs). A live mirror shows its plane's
// reflection, rendered from the mirrored view with x flipped: a screen
// position u samples it at mirror_u - u. Other mirrors are plain silver.
struct Frame {
    view_projection:mat4x4<f32>, eye:vec4<f32>, fog_color:vec4<f32>,
    // Fog start, visible distance, unused, fog enabled.
    atmosphere:vec4<f32>,
    screen:vec4<f32>,
};
@group(0) @binding(0) var<uniform> frame:Frame;
// mirror_u, 1 when live.
struct Slot { sample:vec4<f32> };
@group(1) @binding(0) var<uniform> slot:Slot;
@group(1) @binding(1) var picture:texture_2d<f32>;
@group(1) @binding(2) var picture_sampler:sampler;
struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) world:vec3<f32>,
    @location(1) tint:vec4<f32>,
};
@vertex fn vs_main(@location(0) position:vec3<f32>,@location(1) tint:vec4<f32>)->VertexOut {
    var out:VertexOut;
    out.position=frame.view_projection*vec4<f32>(position,1.0);
    out.world=position;
    out.tint=tint;
    return out;
}
// Unlit polished silver, display encoded.
const SILVER=vec3<f32>(0.55,0.57,0.6);
fn fog_amount(position:vec3<f32>)->f32 {
    let distance=length(position-frame.eye.xyz);
    let t=clamp((distance-frame.atmosphere.x)/max(frame.atmosphere.y-frame.atmosphere.x,0.001),0.0,1.0);
    return (1.0-(1.0-t)*(1.0-t))*frame.atmosphere.w;
}
@fragment fn fs_main(v:VertexOut)->@location(0) vec4<f32> {
    let uv=v.position.xy/frame.screen.xy;
    let stored=textureSample(picture,picture_sampler,vec2<f32>(slot.sample.x-uv.x,uv.y)).rgb;
    if slot.sample.y>0.5 {
        // The reflection already went through output_color (fog and
        // colour-vision assistance included): tint it and store it the same
        // way, without applying them twice.
        var display=stored;
        if OUTPUT_ENCODED==0u {display=display_color(stored);}
        display*=v.tint.rgb;
        if OUTPUT_ENCODED==0u {return vec4<f32>(linear_color(display),v.tint.a);}
        return vec4<f32>(display,v.tint.a);
    }
    let silver=mix(SILVER*v.tint.rgb,frame.fog_color.rgb,fog_amount(v.world));
    return vec4<f32>(output_color(silver),v.tint.a);
}
