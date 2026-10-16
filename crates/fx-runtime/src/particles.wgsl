struct PointLight { position_radius:vec4<f32>, color:vec4<f32> }
struct Camera { view_projection:mat4x4<f32>, right:vec4<f32>, up:vec4<f32>, position:vec4<f32>, atmosphere:vec4<f32>, fog_color:vec4<f32>, haze:vec4<f32>, ambient:vec4<f32>, sun_direction:vec4<f32>, sun_color:vec4<f32>, light_count:vec4<u32>, lights:array<PointLight,256> }
@group(0) @binding(0) var<uniform> camera:Camera;
@group(1) @binding(0) var images:texture_2d_array<f32>;
@group(1) @binding(1) var image_sampler:sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) color:vec4<f32>, @location(2) @interpolate(flat) image:vec4<f32>, @location(3) world:vec3<f32>, @location(4) @interpolate(flat) origin:vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) vertex:u32,@location(0) position_size:vec4<f32>,@location(1) color:vec4<f32>,@location(2) axis_spin:vec4<f32>,@location(3) image:vec4<f32>,@location(4) origin:vec2<f32>,@location(5) sprite_frame:vec4<f32>,@location(6) plane_lighting:vec4<f32>)->Out {
    let plane_right=plane_lighting.xyz;
    let corners=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(1.,1.),vec2(-1.,-1.),vec2(1.,1.),vec2(-1.,1.));
    let corner=corners[vertex];
    var right=camera.right.xyz;var up=camera.up.xyz;
    if dot(plane_right,plane_right)>0.00001 && dot(axis_spin.xyz,axis_spin.xyz)>0.00001 {
        let normal=normalize(axis_spin.xyz);
        var projected=plane_right-normal*dot(normal,plane_right);
        // A velocity-oriented normal can turn through the original right axis.
        if dot(projected,projected)<0.00001 {
            var fallback=vec3(1.,0.,0.);
            if abs(normal.x)>0.9 {fallback=vec3(0.,0.,1.);}
            projected=fallback-normal*dot(normal,fallback);
        }
        let base_right=normalize(projected);
        let base_up=normalize(cross(normal,base_right));
        let c=cos(axis_spin.w);let s=sin(axis_spin.w);
        right=base_right*c+base_up*s;up=base_up*c-base_right*s;
    } else if dot(axis_spin.xyz,axis_spin.xyz)>0.00001 {
        up=normalize(axis_spin.xyz);
        let crossed=cross(position_size.xyz-camera.position.xyz,up);
        if dot(crossed,crossed)>0.00001 {right=normalize(crossed);} else {right=normalize(cross(up,camera.right.xyz+camera.up.xyz*0.37));}
    } else {
        let c=cos(axis_spin.w);let s=sin(axis_spin.w);
        right=camera.right.xyz*c+camera.up.xyz*s;up=camera.up.xyz*c-camera.right.xyz*s;
    }
    var out:Out;
    let local=(corner*0.5+0.5-vec2(sprite_frame.z,1.-sprite_frame.w))*sprite_frame.xy;
    let world=position_size.xyz+(right*local.x+up*local.y)*position_size.w;
    var tint=color;
    if plane_lighting.w>0.5 {
        let crossed=cross(right,up);
        var normal=crossed/max(length(crossed),0.00001);
        // Thin sprite faces receive illumination on the side being viewed.
        if dot(normal,camera.position.xyz-position_size.xyz)<0. { normal=-normal; }
        var light=camera.ambient.rgb+camera.sun_color.rgb*max(dot(normal,-camera.sun_direction.xyz),0.);
        for(var i=0u;i<min(camera.light_count.x,256u);i+=1u) {
            let lamp=camera.lights[i];
            let delta=lamp.position_radius.xyz-world;
            let distance=length(delta);
            if distance<lamp.position_radius.w {
                // Same quadratic point attenuation as native vertex-lit models.
                light+=lamp.color.rgb*max(dot(normal,delta/max(distance,0.00001)),0.)/(1.+0.1*distance*distance);
            }
        }
        tint=vec4(color.rgb*light,color.a);
    }
    out.position=camera.view_projection*vec4(world,1.);out.world=world;
    out.uv=vec2(corner.x*0.5+0.5,0.5-corner.y*0.5);out.color=tint;out.image=image;out.origin=origin;return out;
}
@fragment fn fs_main(input:Out)->@location(0) vec4<f32> {
    // The texture fills the layer's corner: sample it there, clamped to its
    // edge texels as a texture of its own size is.
    let size=input.image.yz;
    let layer_size=vec2<f32>(textureDimensions(images));
    let uv=(input.origin+clamp(input.uv*size,vec2(0.5),size-vec2(0.5)))/layer_size;
    let texel=textureSample(images,image_sampler,uv,i32(input.image.x));
    let own=display_color(texel.rgb)*input.color.rgb;
    // The scene's fog: blended sprites take its colour, added light fades;
    // toward the visible distance sprites fade out into what is behind.
    let offset=input.world-camera.position.xyz;
    let fog=fog_along(offset,camera.atmosphere,camera.fog_color.w,camera.haze);
    let kept=1.-fog_edge(offset,camera.atmosphere,camera.haze);
    let a=texel.a*input.color.a*kept;
    // Premultiplied: alpha blending, additive, additive colour.
    if input.image.w<0.5 {return vec4(output_color(mix(own,camera.fog_color.rgb,fog))*a,a);}
    if input.image.w<1.5 {return vec4(output_color(own*(1.-fog))*a,0.);}
    return vec4(output_color(own*(1.-fog)*kept),0.);
}
