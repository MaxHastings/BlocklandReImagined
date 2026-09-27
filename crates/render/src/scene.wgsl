override OUTPUT_ENCODED: u32 = 0u;
fn output_color(display:vec3<f32>)->vec3<f32> {
    if OUTPUT_ENCODED == 1u { return display; }
    return linear_color(display);
}
struct Camera {
    view_projection:mat4x4<f32>, eye:vec4<f32>, sun_direction:vec4<f32>,
    sun_color:vec4<f32>, ambient:vec4<f32>, fog_color:vec4<f32>, atmosphere:vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera:Camera;
struct PointLight { position_radius:vec4<f32>, color:vec4<f32> };
struct PointLights { count:vec4<u32>, values:array<PointLight,256> };
@group(0) @binding(1) var<uniform> lights:PointLights;
// Smooth finite-radius native falloff; shadowing and exact v20 falloff remain separate.
fn point_illumination(position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    var result=vec3<f32>(0.0);
    let n=normal/max(length(normal),0.0001);
    for(var i=0u;i<min(lights.count.x,256u);i+=1u) {
        let light=lights.values[i];
        let delta=light.position_radius.xyz-position;
        let distance=length(delta);
        let falloff=max(1.0-distance/max(light.position_radius.w,0.0001),0.0);
        result+=light.color.rgb*falloff*falloff*max(dot(n,delta/max(distance,0.0001)),0.0);
    }
    return result;
}
@group(1) @binding(0) var layer0:texture_2d<f32>;
@group(1) @binding(1) var layer1:texture_2d<f32>;
@group(1) @binding(2) var layer2:texture_2d<f32>;
@group(1) @binding(3) var layer3:texture_2d<f32>;
@group(1) @binding(4) var layer4:texture_2d<f32>;
@group(1) @binding(5) var layer5:texture_2d<f32>;
@group(1) @binding(6) var layer6:texture_2d<f32>;
@group(1) @binding(7) var layer7:texture_2d<f32>;
@group(1) @binding(8) var lightmap:texture_2d<f32>;
@group(1) @binding(9) var weights0:texture_2d<f32>;
@group(1) @binding(10) var weights1:texture_2d<f32>;
@group(1) @binding(11) var detail:texture_2d<f32>;
@group(1) @binding(12) var bump:texture_2d<f32>;
// Diffuse images use the player's filtering; lightmaps and weights always
// sample their base level bilinearly.
@group(0) @binding(2) var tiled:sampler;
@group(0) @binding(3) var clamped:sampler;
@group(0) @binding(4) var tiled_exact:sampler;
@group(0) @binding(5) var clamped_exact:sampler;
// Cascaded sun shadows of bricks, players and models (see shadow.rs); one
// layer per cascade. forward_count.w==0 disables them.
struct Shadows {
    matrices:array<mat4x4<f32>,4>, splits:vec4<f32>, texels:vec4<f32>,
    forward_count:vec4<f32>, params:vec4<f32>,
};
@group(0) @binding(6) var shadow_map:texture_depth_2d_array;
@group(0) @binding(7) var shadow_sampler:sampler_comparison;
@group(0) @binding(8) var<uniform> shadows:Shadows;
struct ShadowCoord { uv:vec2<f32>, depth:f32, cascade:i32, strength:f32 };
fn shadow_coord(position:vec3<f32>,normal:vec3<f32>)->ShadowCoord {
    var out:ShadowCoord;
    out.cascade=-1;
    let count=i32(shadows.forward_count.w);
    let view_depth=dot(position-camera.eye.xyz,shadows.forward_count.xyz);
    for(var i=0;i<count;i+=1) {
        if view_depth<shadows.splits[i] {out.cascade=i;break;}
    }
    if out.cascade<0 {return out;}
    // Offset along the normal by the cascade's texel size against acne.
    let n=normal/max(length(normal),0.0001);
    let clip=shadows.matrices[out.cascade]*vec4<f32>(position+n*shadows.texels[out.cascade]*1.5,1.0);
    out.uv=clip.xy*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5);
    out.depth=clip.z;
    // Fade out over the last tenth of the shadow distance.
    let last=shadows.splits[count-1];
    out.strength=clamp((last-view_depth)/(last*0.1),0.0,1.0);
    return out;
}
// 3x3 percentage-closer filter; 1 is fully lit.
fn shadow_lit(c:ShadowCoord)->f32 {
    if c.cascade<0 {return 1.0;}
    let layer=c.cascade;
    let texel=1.0/shadows.params.y;
    var lit=0.0;
    for(var y=-1;y<=1;y+=1) {
        for(var x=-1;x<=1;x+=1) {
            lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,c.uv+vec2<f32>(f32(x),f32(y))*texel,layer,c.depth);
        }
    }
    return mix(1.0,lit/9.0,c.strength);
}
// Sun light reaching a vertex-lit surface past bricks, players and models.
fn sun_visibility(position:vec3<f32>,normal:vec3<f32>)->f32 {
    return shadow_lit(shadow_coord(position,normal));
}
// Lightmaps already hold the map's own sun shadow. A caster darkens them to
// at most the ambient level, so baked shadow is never darkened twice.
fn shadowed_lightmap(lightmap:vec3<f32>,position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    let c=shadow_coord(position,normal);
    if c.cascade<0 {return lightmap;}
    return mix(min(lightmap,camera.ambient.rgb),lightmap,shadow_lit(c));
}
@group(1) @binding(15) var<uniform> material:array<vec4<f32>,4>;
// v20 brick FX (blocklandv20.exe quad emitter 0x52ed70, docs/audits/bricks.md).
// fx.w packs 1 + color + 8*shape + 32*corner + 128*depthStuds; fx.xyz is the
// brick centre. Only brick materials read it.
struct BrickFx { color:u32, shape:u32, corner:u32, depth:f32 };
fn brick_fx(fx:vec4<f32>)->BrickFx {
    var out=BrickFx(0u,0u,0u,1.0);
    if (material[0].x==2.0 || material[0].x==3.0) && fx.w>=1.0 {
        let code=u32(fx.w)-1u;
        out.color=code%8u;out.shape=(code/8u)%4u;out.corner=(code/32u)%4u;out.depth=f32(code/128u);
    }
    return out;
}
// The v20 millisecond clock mod `period`, folded at half: 0 -> 1 -> 0.
fn folded(ms:f32,period:f32)->f32 {
    let t=ms-floor(ms/period)*period;
    return min(t,period-t)/(period*0.5);
}
const TAU:f32=6.2831853;
struct VertexOut {
    @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>,
    @location(1) lightmap_uv:vec2<f32>, @location(2) color:vec4<f32>, @location(3) normal:vec3<f32>,
    @location(4) world_position:vec3<f32>,
    @location(5) @interpolate(flat) fx:vec2<u32>,
};
@vertex fn vs_main(@location(0) local_position:vec3<f32>,@location(1) local_normal:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) lightmap_uv:vec2<f32>,@location(4) local_color:vec4<f32>,
    @location(5) m0:vec4<f32>,@location(6) m1:vec4<f32>,@location(7) m2:vec4<f32>,@location(8) m3:vec4<f32>,@location(9) tint:vec4<f32>,
    @location(10) fx_data:vec4<f32>)->VertexOut {
    let model=mat4x4<f32>(m0,m1,m2,m3);
    let position=(model*vec4<f32>(local_position,1.0)).xyz;
    // Cofactor matrix is det(M)*inverse-transpose(M). Positive affine
    // determinants are validated on the CPU; preserve nonuniform scale normals.
    let cofactor=mat3x3<f32>(cross(m1.xyz,m2.xyz),cross(m2.xyz,m0.xyz),cross(m0.xyz,m1.xyz));
    let normal=(cofactor*local_normal)/dot(m0.xyz,cross(m1.xyz,m2.xyz));
    let color=local_color*tint;
    var out:VertexOut;out.position=camera.view_projection*vec4<f32>(position,1.0);
    out.uv=uv;out.lightmap_uv=lightmap_uv;out.color=color;out.normal=normal;out.world_position=position;
    let fx=brick_fx(fx_data);out.fx=vec2<u32>(fx.color,fx.shape);
    let ms=camera.atmosphere.z*1000.0;
    // Torque axes: x = native x, y = -native z, z = native y.
    var world=position;
    if fx.shape==1u {
        // Undulo: only within 100 units, amplitude 0.08 at full strength.
        let distance=length(fx_data.xyz-camera.eye.xyz);
        if distance<100.0 {
            let amplitude=clamp(100.0-distance,0.0,10.0)*0.1*0.08;
            let s=(position.x-position.z+position.y)*1.256637+fract(ms/1000.0)*TAU;
            world+=vec3<f32>(sin(s+2.094395),sin(s+TAU),-sin(s+4.18879))*amplitude;
        }
    } else if fx.shape==2u {
        // Water: waves along x lower the vertex; GL leaves the normal unnormalised.
        let a=position.x*1.256637+fract(ms/2000.0)*TAU;
        world.y-=0.1*(sin(a+TAU)+1.0);
        out.normal.y+=0.25*(sin(a+2.094395)+1.0);
    }
    var paint=color;
    let toward=normalize(fx_data.xyz-camera.eye.xyz);
    switch fx.color {
        case 1u: {
            // Pearl: near side clamp(1.6 paint), far side 0.9 paint.
            let t=clamp(dot(world-fx_data.xyz,toward)/(fx.depth*0.125)+0.5,0.0,1.0);
            paint=vec4<f32>(min(color.rgb*1.6,vec3<f32>(1.0))*(1.0-t)+color.rgb*0.9*t,color.a*(1.0-0.1*t));
        }
        case 2u: {
            // Chrome: near half blends to white, far half to half paint.
            let t=clamp(dot(world-fx_data.xyz,toward)/(fx.depth*0.5)+0.5,0.0,1.0);
            if t>0.5 {
                let k=clamp((t-0.5)*2.0,0.0,1.0);
                paint=vec4<f32>(mix(color.rgb,color.rgb*0.5,k),color.a);
            } else {
                let k=clamp(t*2.0,0.0,1.0);
                paint=vec4<f32>(color.rgb*k+vec3<f32>(1.0-k),color.a);
            }
        }
        case 4u: {paint=vec4<f32>(color.rgb*(0.7+0.6*folded(ms,1000.0)),color.a);}
        case 5u: {paint=vec4<f32>(color.rgb*(0.4+0.6*folded(ms+250.0*f32(fx.corner),1000.0)),color.a);}
        case 6u: {
            let phase=fract(ms/1000.0)*TAU;
            let x=world.x;let y=-world.z;let z=world.y;
            let wave=vec3<f32>(sin(y+z-x+phase+2.094395),sin(x-y+z+phase+4.18879),sin(y+x-z+phase+TAU));
            paint=vec4<f32>(max(wave*color.rgb,vec3<f32>(0.0)),color.a);
        }
        default: {}
    }
    out.color=paint;
    if fx.shape!=0u {
        out.world_position=world;
        out.position=camera.view_projection*vec4<f32>(world,1.0);
    }
    if material[0].x==6.0 {
        let phase=vec2<f32>(position.x+1024.0,1024.0-position.z)*0.05+vec2<f32>(camera.atmosphere.z);
        out.world_position.y+=(sin(phase.x)+sin(phase.y))*material[1].z*0.25;
        out.normal=normalize(vec3<f32>(-cos(phase.x)*material[1].z*0.0125,1.0,cos(phase.y)*material[1].z*0.0125));
        out.position=camera.view_projection*vec4<f32>(out.world_position,1.0);
    }
    if (material[0].x==4.0 || material[0].x==5.0) {
        out.position=camera.view_projection*vec4<f32>(camera.eye.xyz+position,1.0);
        out.position.z=out.position.w;
        if material[0].x==5.0 {out.uv=uv+fract(normal.xy*camera.atmosphere.z);}
    }
    return out;
}
fn fog_amount(position:vec3<f32>)->f32 {
    let distance=length(position-camera.eye.xyz);
    let t=clamp((distance-camera.atmosphere.x)/max(camera.atmosphere.y-camera.atmosphere.x,0.001),0.0,1.0);
    return (1.0-(1.0-t)*(1.0-t))*camera.atmosphere.w;
}
fn fogged(display:vec3<f32>,position:vec3<f32>)->vec3<f32> {
    return output_color(mix(display,camera.fog_color.rgb,fog_amount(position)));
}
// Classic TerrainRender frame-buffer passes, evaluated per pixel in display
// space. material[1]: zero-detail distance, zero-bump distance, detail texture
// repeats per unit (s,t). material[2]: bump repeats per cell, sun-derived
// emboss offset (s,t), flags (1 detail, 2 bump). material[3].xyz: cell size,
// terrain origin x/z (texture generation is in terrain object space).
fn terrain_passes(lit:vec3<f32>,position:vec3<f32>)->vec3<f32> {
    let distance=length(position-camera.eye.xyz);
    let fog=fog_amount(position);
    let local=vec2<f32>(position.x-material[3].y,material[3].z-position.z);
    let flags=u32(material[2].w);
    var color=clamp(mix(lit,camera.fog_color.rgb,fog),vec3<f32>(0.0),vec3<f32>(1.0));
    let zero_bump=material[1].y;
    if (flags&2u)!=0u && distance<zero_bump {
        // Halved bump plus halved-inverted bump shifted toward the sun,
        // faded to neutral grey over the last quarter, then modulate-2x.
        let uv=local/material[3].x*material[2].x;
        let b0=textureSampleLevel(bump,tiled_exact,uv,0.0).rgb;
        let b1=textureSampleLevel(bump,tiled_exact,uv+material[2].yz,0.0).rgb;
        let emboss=clamp(vec3<f32>(127.0/255.0)+(b0-b1)*0.5,vec3<f32>(0.0),vec3<f32>(1.0));
        let fade=clamp((distance/zero_bump-0.75)*4.0,0.0,1.0);
        color=clamp(color*2.0*mix(emboss,vec3<f32>(127.0/255.0),fade),vec3<f32>(0.0),vec3<f32>(1.0));
    }
    let zero_detail=material[1].x;
    if (flags&1u)!=0u && distance<zero_detail {
        let d=textureSampleLevel(detail,tiled_exact,local*material[1].zw,0.0);
        let c=(1.0-fog)*clamp((zero_detail-distance)/zero_detail,0.0,1.0);
        color=color*(d.rgb*c+vec3<f32>(1.0-d.a*c));
    }
    return output_color(color);
}
@fragment fn fs_main(v:VertexOut)->@location(0) vec4<f32> {
    if material[0].x==6.0 {
        let time=camera.atmosphere.z;
        let phase=vec2<f32>(v.world_position.x+1024.0,1024.0-v.world_position.z)*material[2].x+vec2<f32>(time/material[2].z);
        let distortion=vec2<f32>(cos(phase.x),sin(phase.y))*material[2].y;
        var drift=vec2<f32>(time*0.02,cos(time*0.785398163)*0.03);
        if material[2].w>0.5 {drift=material[1].xy*time;}
        let base=v.uv*material[3].x+distortion;
        let second=mat2x2<f32>(vec2<f32>(0.8660254,0.5),vec2<f32>(-0.5,0.8660254))*(base+drift*material[3].w);
        let first_rgb=display_color(textureSample(layer0,tiled,base+drift).rgb);
        let second_rgb=display_color(textureSample(layer0,tiled,second).rgb);
        let masks=textureSample(lightmap,clamped_exact,v.lightmap_uv);
        let a=masks.r;
        // Collapse the two classic straight-alpha surface passes into one.
        var alpha=1.0-(1.0-a)*(1.0-a);
        var rgb=(first_rgb*a*(1.0-a)+second_rgb*a)/max(alpha,0.00001);
        let shore=display_color(textureSample(layer1,tiled,v.uv*material[3].y+distortion+drift).rgb);
        let sa=masks.g;
        let combined=sa+alpha*(1.0-sa);
        rgb=(shore*sa+rgb*alpha*(1.0-sa))/max(combined,0.00001);
        alpha=combined;
        let reflected=reflect(normalize(v.world_position-camera.eye.xyz),normalize(v.normal));
        let reflection_uv=vec2<f32>(atan2(reflected.x,reflected.z)*0.159154943+0.5,acos(clamp(reflected.y,-1.0,1.0))*0.318309886);
        let reflection=display_color(textureSample(layer2,clamped,reflection_uv).rgb);
        rgb=mix(rgb,reflection,clamp(material[3].z*(0.5+0.15*(cos(phase.x)+sin(phase.y))),0.0,1.0));
        if alpha<=0.00001 {discard;}
        return vec4<f32>(fogged(rgb,v.world_position),alpha);
    }
    if (material[0].x==4.0 || material[0].x==5.0) {
        var sky=textureSample(layer0,clamped,v.uv);
        if material[0].x==5.0 {sky=textureSample(layer0,tiled,v.uv);}
        return vec4<f32>(output_color(display_color(sky.rgb)*v.color.rgb),sky.a*v.color.a);
    }
    if material[0].x==1.0 {
        let weight_uv=v.lightmap_uv+vec2<f32>(0.5)/vec2<f32>(textureDimensions(weights0));
        let a=textureSample(weights0,tiled_exact,weight_uv);let b=textureSample(weights1,tiled_exact,weight_uv);
        let diffuse=display_color(textureSample(layer0,tiled,v.uv).rgb)*a.r
            +display_color(textureSample(layer1,tiled,v.uv).rgb)*a.g
            +display_color(textureSample(layer2,tiled,v.uv).rgb)*a.b
            +display_color(textureSample(layer3,tiled,v.uv).rgb)*a.a
            +display_color(textureSample(layer4,tiled,v.uv).rgb)*b.r
            +display_color(textureSample(layer5,tiled,v.uv).rgb)*b.g
            +display_color(textureSample(layer6,tiled,v.uv).rgb)*b.b
            +display_color(textureSample(layer7,tiled,v.uv).rgb)*b.a;
        let light_uv=v.lightmap_uv+vec2<f32>(0.5)/vec2<f32>(textureDimensions(lightmap));
        let terrain_light=shadowed_lightmap(textureSample(lightmap,tiled_exact,light_uv).rgb,v.world_position,v.normal);
        return vec4<f32>(terrain_passes(diffuse*v.color.rgb*(terrain_light+point_illumination(v.world_position,v.normal)),v.world_position),v.color.a);
    }
    let fx=v.fx;let time=camera.atmosphere.z;
    // v20 brickSIDE: GL_CLAMP with nearest magnification. Snap to base-level
    // texel centres and keep the unsnapped derivatives for mip selection.
    let size=vec2<f32>(textureDimensions(layer0));
    let dx=dpdx(v.uv);let dy=dpdy(v.uv);
    let snapped=(floor(clamp(v.uv,vec2<f32>(0.),vec2<f32>(1.))*size-vec2<f32>(0.0001))+vec2<f32>(0.5))/size;
    let edge=textureSampleGrad(layer0,clamped,clamp(snapped,vec2<f32>(0.5)/size,vec2<f32>(1.)-vec2<f32>(0.5)/size),dx,dy);
    var albedo=textureSampleGrad(layer0,tiled,v.uv,dx,dy);
    if material[0].z==1.0 {albedo=edge;}
    let base_color=v.color.rgb;
    var alpha=albedo.a*v.color.a;
    var pigment=display_color(albedo.rgb)*base_color;
    let decal=material[0].x==3.0;
    if decal {
        // v20 fxBrickBatcher uses GL_DECAL (0x52d120): lit paint first, then
        // the raw UNORM overlay mixed by its coverage alpha. Alpha is paint's.
        pigment=base_color;
        alpha=v.color.a;
    }
    if material[0].x==7.0 {
        // Original brick masks have low coverage alpha (e.g. TOP <=46/255).
        // They overlay pigment on an opaque painted surface, not cut holes.
        pigment=mix(base_color,albedo.rgb,albedo.a);
        alpha=v.color.a;
    }
    if material[0].w==1.0 {
        // v20 temp brick (0x52e6b4): t = ms mod 800 folded at 400, then
        // alpha = offset 0.3 + range 0.3 * t/400, ignoring paint alpha.
        let t=fract(time/0.8)*0.8;
        alpha=0.3+0.3*min(t,0.8-t)/0.4;
    }
    if alpha<=material[0].y {discard;}
    if material[0].x==7.0 || material[0].x==8.0 {
        return vec4<f32>(fogged(pigment,v.world_position),alpha);
    }
    var illumination=textureSample(lightmap,clamped_exact,v.lightmap_uv).rgb;
    if material[0].x==2.0 || material[0].x==3.0 {
        let normal=v.normal/max(length(v.normal),0.0001);
        let direction=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
        // Water keeps the lengthened GL normal; chrome doubles its normals.
        var strength=1.0;
        if fx.y==2u {strength=length(v.normal);}
        if fx.x==2u {strength*=2.0;}
        let facing=max(dot(normal,-direction),0.0)*strength;
        var sun=0.0;
        if facing>0.0 {sun=facing*sun_visibility(v.world_position,normal);}
        illumination=camera.ambient.rgb+camera.sun_color.rgb*sun
            +point_illumination(v.world_position,v.normal)*strength;
        if fx.x==3u {
            // Glow aims the normal at the sun, 1/min(1, sun rgb) long.
            var shortest=1.0;
            for(var c=0;c<3;c+=1) {if camera.sun_color[c]>0.0 {shortest=min(shortest,camera.sun_color[c]);}}
            illumination=camera.ambient.rgb+camera.sun_color.rgb/shortest;
        }
    } else {
        illumination=shadowed_lightmap(illumination,v.world_position,v.normal)
            +point_illumination(v.world_position,v.normal);
    }
    var display=pigment*illumination;
    // Fixed-function lighting clamps the vertex colour before texturing.
    if decal {display=mix(min(display,vec3<f32>(1.)),albedo.rgb,albedo.a);}
    return vec4<f32>(fogged(display,v.world_position),alpha);
}
