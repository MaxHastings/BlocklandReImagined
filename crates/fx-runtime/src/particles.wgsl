struct Camera { view_projection:mat4x4<f32>, right:vec4<f32>, up:vec4<f32>, position:vec4<f32> }
@group(0) @binding(0) var<uniform> camera:Camera;
@group(1) @binding(0) var image:texture_2d<f32>;
@group(1) @binding(1) var image_sampler:sampler;
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) color:vec4<f32> }
@vertex fn vs_main(@builtin(vertex_index) vertex:u32,@location(0) position_size:vec4<f32>,@location(1) color:vec4<f32>,@location(2) axis_spin:vec4<f32>)->Out {
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
    out.uv=vec2(corner.x*0.5+0.5,0.5-corner.y*0.5);out.color=color;return out;
}
@fragment fn fs_main(input:Out)->@location(0) vec4<f32> {
    let texel=textureSample(image,image_sampler,input.uv);
    return vec4(output_color(display_color(texel.rgb)*input.color.rgb),texel.a*input.color.a);
}
