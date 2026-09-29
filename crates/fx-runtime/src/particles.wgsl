struct Camera { view_projection:mat4x4<f32>, right:vec4<f32>, up:vec4<f32>, position:vec4<f32> }
@group(0) @binding(0) var<uniform> camera:Camera;
@group(1) @binding(0) var images:texture_2d_array<f32>;
@group(1) @binding(1) var image_sampler:sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) color:vec4<f32>, @location(2) @interpolate(flat) image:vec4<f32> }
@vertex fn vs_main(@builtin(vertex_index) vertex:u32,@location(0) position_size:vec4<f32>,@location(1) color:vec4<f32>,@location(2) axis_spin:vec4<f32>,@location(3) image:vec4<f32>)->Out {
    let corners=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(1.,1.),vec2(-1.,-1.),vec2(1.,1.),vec2(-1.,1.));
    let corner=corners[vertex];
    var right=camera.right.xyz;var up=camera.up.xyz;
    if dot(axis_spin.xyz,axis_spin.xyz)>0.00001 {
        up=normalize(axis_spin.xyz);
        let crossed=cross(position_size.xyz-camera.position.xyz,up);
        if dot(crossed,crossed)>0.00001 {right=normalize(crossed);} else {right=normalize(cross(up,camera.right.xyz+camera.up.xyz*0.37));}
    } else {
        let c=cos(axis_spin.w);let s=sin(axis_spin.w);
        right=camera.right.xyz*c+camera.up.xyz*s;up=camera.up.xyz*c-camera.right.xyz*s;
    }
    var out:Out;
    out.position=camera.view_projection*vec4(position_size.xyz+(right*corner.x+up*corner.y)*position_size.w*0.5,1.);
    out.uv=vec2(corner.x*0.5+0.5,0.5-corner.y*0.5);out.color=color;out.image=image;return out;
}
@fragment fn fs_main(input:Out)->@location(0) vec4<f32> {
    // The texture fills the layer's corner: sample it there, clamped to its
    // edge texels as a texture of its own size is.
    let size=input.image.yz;
    let layer_size=vec2<f32>(textureDimensions(images));
    let uv=clamp(input.uv*size,vec2(0.5),size-vec2(0.5))/layer_size;
    let texel=textureSample(images,image_sampler,uv,i32(input.image.x));
    let rgb=output_color(display_color(texel.rgb)*input.color.rgb);
    let a=texel.a*input.color.a;
    // Premultiplied: alpha blending, additive, additive colour.
    if input.image.w<0.5 {return vec4(rgb*a,a);}
    if input.image.w<1.5 {return vec4(rgb*a,0.);}
    return vec4(rgb,0.);
}
