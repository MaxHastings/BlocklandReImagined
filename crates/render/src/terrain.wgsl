struct VertexOut {
    @builtin(position) position:vec4<f32>,
    @location(0) uv:vec2<f32>,
    @location(1) grid_uv:vec2<f32>,
};
@group(0) @binding(0) var tiled:sampler;
@group(0) @binding(1) var layer0:texture_2d<f32>;
@group(0) @binding(2) var layer1:texture_2d<f32>;
@group(0) @binding(3) var layer2:texture_2d<f32>;
@group(0) @binding(4) var layer3:texture_2d<f32>;
@group(0) @binding(5) var layer4:texture_2d<f32>;
@group(0) @binding(6) var layer5:texture_2d<f32>;
@group(0) @binding(7) var layer6:texture_2d<f32>;
@group(0) @binding(8) var layer7:texture_2d<f32>;
@group(0) @binding(9) var lightmap:texture_2d<f32>;
@group(0) @binding(10) var weights0:texture_2d<f32>;
@group(0) @binding(11) var weights1:texture_2d<f32>;
@vertex fn vs_main(@location(0) position:vec4<f32>,@location(1) uv:vec2<f32>,@location(2) grid_uv:vec2<f32>)->VertexOut {
    var out:VertexOut;out.position=position;out.uv=uv;out.grid_uv=grid_uv;return out;
}
@fragment fn fs_main(v:VertexOut)->@location(0) vec4<f32> {
    // Weight samples sit at vertices; offset to texture texel centers.
    let weight_uv=v.grid_uv+vec2<f32>(0.5)/vec2<f32>(textureDimensions(weights0));
    let a=textureSample(weights0,tiled,weight_uv);
    let b=textureSample(weights1,tiled,weight_uv);
    let color=(display_color(textureSample(layer0,tiled,v.uv).rgb)*a.r
        +display_color(textureSample(layer1,tiled,v.uv).rgb)*a.g
        +display_color(textureSample(layer2,tiled,v.uv).rgb)*a.b
        +display_color(textureSample(layer3,tiled,v.uv).rgb)*a.a
        +display_color(textureSample(layer4,tiled,v.uv).rgb)*b.r
        +display_color(textureSample(layer5,tiled,v.uv).rgb)*b.g
        +display_color(textureSample(layer6,tiled,v.uv).rgb)*b.b
        +display_color(textureSample(layer7,tiled,v.uv).rgb)*b.a);
    let light_uv=v.grid_uv+vec2<f32>(0.5)/vec2<f32>(textureDimensions(lightmap));
    return vec4<f32>(linear_color(color*textureSample(lightmap,tiled,light_uv).rgb),1.0);
}
