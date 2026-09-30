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
// brickSIDE: the clamped filter, or only the nearest base-level texel under
// Use Sharp Filter (see TextureFiltering).
@group(0) @binding(12) var side_sampler:sampler;
// Cascaded sun shadows of bricks, players and models (see shadow.rs); one
// layer per cascade. forward_count.w==0 disables them.
struct Shadows {
    matrices:array<mat4x4<f32>,4>, splits:vec4<f32>, texels:vec4<f32>,
    forward_count:vec4<f32>, params:vec4<f32>, depth_scale:vec4<f32>,
    origin:vec4<f32>,
};
@group(0) @binding(6) var shadow_map:texture_depth_2d_array;
@group(0) @binding(7) var shadow_sampler:sampler_comparison;
@group(0) @binding(8) var<uniform> shadows:Shadows;
@group(0) @binding(9) var shadow_point:sampler;
// Baked interior light for vertex-lit surfaces (light_volume.rs): RGB
// premultiplied by the cell's share outside walls. placement: origin, cell
// size; dimensions, 1 when a volume is bound.
struct LightVolume { origin_cell:vec4<f32>, dims:vec4<f32> };
@group(0) @binding(10) var light_volume:texture_3d<f32>;
@group(0) @binding(11) var<uniform> volume:LightVolume;
// Mirrors LightVolume::light.
fn baked_surroundings(position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    if volume.dims.w==0.0 {return vec3<f32>(0.0);}
    let t=(position-volume.origin_cell.xyz)/(volume.origin_cell.w*volume.dims.xyz);
    if any(t<vec3<f32>(0.0)) || any(t>vec3<f32>(1.0)) {return vec3<f32>(0.0);}
    let s=textureSampleLevel(light_volume,clamped_exact,t,0.0);
    if s.a<0.01 {return vec3<f32>(0.0);}
    let n=normal/max(length(normal),0.0001);
    let form=0.7+0.3*max(dot(n,vec3<f32>(-0.57735,0.57735,0.57735)),0.0);
    return s.rgb*(min(s.a*2.0,1.0)/s.a)*form;
}
// A receiver in one cascade's map: map coordinates, depth along the sun,
// and whether it lies inside that map (with room for the filter footprint).
struct CascadeCoord { uv:vec2<f32>, depth:f32, cascade:i32 };
fn cascade_coord(position:vec3<f32>,n:vec3<f32>,cascade:i32)->CascadeCoord {
    var out:CascadeCoord;
    // Offset along the normal by the cascade's texel size against acne.
    let clip=shadows.matrices[cascade]*vec4<f32>(position+n*shadows.texels[cascade]*1.5,1.0);
    out.uv=clip.xy*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5);
    out.depth=clip.z;
    out.cascade=cascade;
    return out;
}
fn inside_map(c:CascadeCoord)->bool {
    let margin=3.0/shadows.params.y;
    return all(c.uv>vec2<f32>(margin)) && all(c.uv<vec2<f32>(1.0-margin)) && c.depth>0.0 && c.depth<1.0;
}
// The cascade a receiver reads, and the next one it fades into over the
// last part of this cascade's range, so detail changes gradually instead of
// along a hard line (a straight seam across a flat wall).
struct ShadowCoord { near:CascadeCoord, far:CascadeCoord, blend:f32, strength:f32 };
fn shadow_coord(position:vec3<f32>,normal:vec3<f32>)->ShadowCoord {
    var out:ShadowCoord;
    out.near.cascade=-1;
    out.blend=0.0;
    let count=i32(shadows.forward_count.w);
    let view_depth=dot(position-shadows.origin.xyz,shadows.forward_count.xyz);
    var cascade=-1;
    for(var i=0;i<count;i+=1) {
        if view_depth<shadows.splits[i] {cascade=i;break;}
    }
    if cascade<0 {return out;}
    let n=normal/max(length(normal),0.0001);
    out.near=cascade_coord(position,n,cascade);
    // Cascades are fitted to the player's view; a receiver outside its
    // depth's cascade (behind the camera, as a mirror shows it) reads the
    // finest wider one that holds it, or none.
    while !inside_map(out.near) {
        cascade+=1;
        if cascade>=count {out.near.cascade=-1;return out;}
        out.near=cascade_coord(position,n,cascade);
    }
    if cascade+1<count {
        let start=select(0.0,shadows.splits[max(cascade-1,0)],cascade>0);
        let band=CASCADE_BLEND*(shadows.splits[cascade]-start);
        out.far=cascade_coord(position,n,cascade+1);
        if inside_map(out.far) {
            out.blend=clamp((view_depth-(shadows.splits[cascade]-band))/band,0.0,1.0);
        }
    }
    // Fade out over the last tenth of the shadow distance.
    let last=shadows.splits[count-1];
    out.strength=clamp((last-view_depth)/(last*0.1),0.0,1.0);
    return out;
}
// Share of each cascade's depth range over which it fades into the next.
const CASCADE_BLEND:f32=0.2;
// Lit (1) or shadowed (0) for each of a gathered 2x2 texel block, ordered
// as textureGather returns them. A texel is shadowed only when no occluder
// (a brick that does not cast) lies between its caster and this
// surface, so a shadow lands only on the first surface it reaches.
fn shadow_block(uv:vec2<f32>,c:CascadeCoord,occluder_layer:i32,gap:f32)->vec4<f32> {
    let casters=textureGather(shadow_map,shadow_point,uv,c.cascade);
    let occluders=textureGather(shadow_map,shadow_point,uv,occluder_layer);
    let between=occluders>casters+vec4<f32>(gap) & occluders<vec4<f32>(c.depth-gap);
    return select(vec4<f32>(1.0),vec4<f32>(0.0),casters<vec4<f32>(c.depth) & !between);
}
// Smooth 3x3 percentage-closer filter from a 4x4 texel footprint, with
// bilinear edge weights; 1 is fully lit.
fn cascade_lit(c:CascadeCoord)->f32 {
    let occluder_layer=c.cascade+i32(shadows.forward_count.w);
    let size=shadows.params.y;
    // Occluders must be a tenth of a unit clear of caster and receiver.
    let gap=0.1*shadows.depth_scale[c.cascade];
    let f=c.uv*size-vec2<f32>(0.5);
    let base=floor(f);
    let t=f-base;
    // Blocks of texels (base-1, base) and (base+1, base+2) on each axis;
    // gather order is (0,1), (1,1), (1,0), (0,0).
    let a=shadow_block(base/size,c,occluder_layer,gap);
    let b=shadow_block((base+vec2<f32>(2.0,0.0))/size,c,occluder_layer,gap);
    let d=shadow_block((base+vec2<f32>(0.0,2.0))/size,c,occluder_layer,gap);
    let e=shadow_block((base+vec2<f32>(2.0))/size,c,occluder_layer,gap);
    let rows=array<vec4<f32>,4>(
        vec4<f32>(a.w,a.z,b.w,b.z),
        vec4<f32>(a.x,a.y,b.x,b.y),
        vec4<f32>(d.w,d.z,e.w,e.z),
        vec4<f32>(d.x,d.y,e.x,e.y),
    );
    let wx=vec4<f32>(1.0-t.x,1.0,1.0,t.x);
    let wy=vec4<f32>(1.0-t.y,1.0,1.0,t.y);
    return dot(wy,vec4<f32>(dot(rows[0],wx),dot(rows[1],wx),dot(rows[2],wx),dot(rows[3],wx)))/9.0;
}
fn shadow_lit(c:ShadowCoord)->f32 {
    if c.near.cascade<0 {return 1.0;}
    var lit=cascade_lit(c.near);
    // Only receivers in the blend band pay for the second cascade.
    if c.blend>0.0 {lit=mix(lit,cascade_lit(c.far),c.blend);}
    return mix(1.0,lit,c.strength);
}
// Sun light reaching a vertex-lit surface past bricks, players and models.
fn sun_visibility(position:vec3<f32>,normal:vec3<f32>)->f32 {
    return shadow_lit(shadow_coord(position,normal));
}
// Like v20's projected shape shadows, a caster darkens a baked (lightmapped)
// surface by a fixed share whatever its baked light, so players and vehicles
// shadow the dim Bedroom carpet too. The share is the mission's ambient to
// ambient+sun ratio, bounded so shadows stay visible but never black.
fn shadowed_lightmap(lightmap:vec3<f32>,position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    let c=shadow_coord(position,normal);
    if c.near.cascade<0 {return lightmap;}
    let weights=vec3<f32>(0.2126,0.7152,0.0722);
    let ambient=dot(camera.ambient.rgb,weights);
    let lit=ambient+dot(camera.sun_color.rgb,weights);
    let shade=clamp(ambient/max(lit,0.0001),0.4,0.7);
    return lightmap*mix(shade,1.0,shadow_lit(c));
}
@group(1) @binding(15) var<uniform> material:array<vec4<f32>,5>;
// v20 brick FX (blocklandv20.exe quad emitter 0x52ed70, docs/audits/bricks.md).
// fx.w packs 1 + color + 8*shape + 32*corner + 128*depthStuds; fx.xyz is the
// brick centre. Only brick materials read it.
struct BrickFx { color:u32, shape:u32, corner:u32, depth:f32 };
fn brick_fx(fx:vec4<f32>)->BrickFx {
    var out=BrickFx(0u,0u,0u,1.0);
    if (material[0].x==2.0 || material[0].x==3.0 || material[0].x==9.0) && fx.w>=1.0 {
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
    // The FX centre moves with the model, like the vertices (debris is built
    // at the origin and placed by its instance transform).
    let centre=(model*vec4<f32>(fx_data.xyz,1.0)).xyz;
    let ms=camera.atmosphere.z*1000.0;
    // Torque axes: x = native x, y = -native z, z = native y.
    var world=position;
    if fx.shape==1u {
        // Undulo: only within 100 units, amplitude 0.08 at full strength.
        let distance=length(centre-camera.eye.xyz);
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
    let toward=normalize(centre-camera.eye.xyz);
    switch fx.color {
        case 1u: {
            // Pearl: near side clamp(1.6 paint), far side 0.9 paint.
            let t=clamp(dot(world-centre,toward)/(fx.depth*0.125)+0.5,0.0,1.0);
            paint=vec4<f32>(min(color.rgb*1.6,vec3<f32>(1.0))*(1.0-t)+color.rgb*0.9*t,color.a*(1.0-0.1*t));
        }
        case 2u: {
            // Chrome: near half blends to white, far half to half paint.
            let t=clamp(dot(world-centre,toward)/(fx.depth*0.5)+0.5,0.0,1.0);
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
// A diffuse slot by index, for brick surfaces that pick theirs per vertex.
// Explicit gradients keep the sampling valid in non-uniform control flow.
fn slot_sample(slot:u32,s:sampler,uv:vec2<f32>,dx:vec2<f32>,dy:vec2<f32>)->vec4<f32> {
    switch slot {
        case 1u: {return textureSampleGrad(layer1,s,uv,dx,dy);}
        case 2u: {return textureSampleGrad(layer2,s,uv,dx,dy);}
        case 3u: {return textureSampleGrad(layer3,s,uv,dx,dy);}
        case 4u: {return textureSampleGrad(layer4,s,uv,dx,dy);}
        case 5u: {return textureSampleGrad(layer5,s,uv,dx,dy);}
        default: {return textureSampleGrad(layer0,s,uv,dx,dy);}
    }
}
fn slot_size(slot:u32)->vec2<f32> {
    switch slot {
        case 1u: {return vec2<f32>(textureDimensions(layer1));}
        case 2u: {return vec2<f32>(textureDimensions(layer2));}
        case 3u: {return vec2<f32>(textureDimensions(layer3));}
        case 4u: {return vec2<f32>(textureDimensions(layer4));}
        case 5u: {return vec2<f32>(textureDimensions(layer5));}
        default: {return vec2<f32>(textureDimensions(layer0));}
    }
}
@fragment fn fs_main(v:VertexOut)->@location(0) vec4<f32> {
    if material[0].x==6.0 {
        let time=camera.atmosphere.z;
        // Fluid space: Torque x/y plus the terrain's 1024 offset.
        let fluid=vec2<f32>(v.world_position.x+1024.0,1024.0-v.world_position.z);
        let phase=fluid*material[2].x+vec2<f32>(time/material[2].z);
        let depth_mapped=material[2].w>0.5;
        // Only the depth-mapped path distorts and flows its coordinates; the
        // plain two-pass path texgens fluid space at TessSurface/48 per unit
        // and drifts on the fixed 8 s cycle.
        var distortion=vec2<f32>(0.0);
        var drift=vec2<f32>(time*0.02,cos(time*0.785398163)*0.03);
        var base=fluid*material[3].x/48.0;
        if depth_mapped {
            distortion=vec2<f32>(cos(phase.x),sin(phase.y))*material[2].y;
            drift=material[1].xy*time;
            base=v.uv*material[3].x+distortion;
        }
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
        if depth_mapped {
            // fluid::CalcVertSpecular, added (SRC_ALPHA, ONE) under the depth
            // mask: colour.rgb*colour.a*pow(half.up,power)^2, sun as light 0.
            let light=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
            let half_vector=normalize(normalize(camera.eye.xyz-v.world_position)-light);
            let facing=max(half_vector.y,0.0);
            var shine=1.0;
            if material[4].w>0.0 {shine=select(0.0,pow(facing,material[4].w),facing>0.0);}
            rgb+=material[4].rgb*shine*shine*a/alpha;
        }
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
    // Brick surfaces in one material behave as the surface material their
    // vertex names: TOP..RAMP overlays (SIDE clamped), then the unprinted
    // painted face (see MaterialKind::BrickSurfaces).
    var slot=0u;
    var clamp_edge=material[0].z==1.0;
    var overlay=material[0].x==3.0;
    let surfaces=material[0].x==9.0;
    if surfaces {
        slot=min(u32(v.lightmap_uv.x+0.5),5u);
        clamp_edge=slot==1u;
        overlay=slot<5u;
    }
    // v20 brickSIDE: GL_CLAMP with nearest magnification. Snap to base-level
    // texel centres and keep the unsnapped derivatives for mip selection.
    let size=slot_size(slot);
    let dx=dpdx(v.uv);let dy=dpdy(v.uv);
    var albedo:vec4<f32>;
    if clamp_edge {
        let snapped=(floor(clamp(v.uv,vec2<f32>(0.),vec2<f32>(1.))*size-vec2<f32>(0.0001))+vec2<f32>(0.5))/size;
        albedo=slot_sample(slot,side_sampler,clamp(snapped,vec2<f32>(0.5)/size,vec2<f32>(1.)-vec2<f32>(0.5)/size),dx,dy);
    } else {
        albedo=slot_sample(slot,tiled,v.uv,dx,dy);
    }
    let base_color=v.color.rgb;
    var alpha=albedo.a*v.color.a;
    let flags=u32(material[0].w);
    // Opaque Torque model materials ignore texture alpha (no blend, no test).
    if (flags&2u)!=0u {alpha=v.color.a;}
    var pigment=display_color(albedo.rgb)*base_color;
    let decal=overlay;
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
    if (flags&1u)!=0u {
        // v20 temp brick (0x52e6b4): t = ms mod flashTime folded at half,
        // then alpha = offset + range * t/half, ignoring paint alpha.
        // material[1] holds (flashTime s, range, offset); v20's 800 ms,
        // 0.3 and 0.3 when unset.
        let f=material[1];
        let period=select(0.8,f.x,f.x>0.0);
        let range=select(0.3,f.y,f.x>0.0);
        let offset=select(0.3,f.z,f.x>0.0);
        let t=fract(time/period)*period;
        alpha=offset+range*min(t,period-t)/(period*0.5);
    }
    if alpha<=material[0].y {discard;}
    if material[0].x==7.0 || material[0].x==8.0 {
        return vec4<f32>(fogged(pigment,v.world_position),alpha);
    }
    var illumination=textureSample(lightmap,clamped_exact,v.lightmap_uv).rgb;
    if material[0].x==2.0 || material[0].x==3.0 || surfaces {
        let normal=v.normal/max(length(v.normal),0.0001);
        let direction=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
        // Water keeps the lengthened GL normal; chrome doubles its normals.
        var strength=1.0;
        if fx.y==2u {strength=length(v.normal);}
        if fx.x==2u {strength*=2.0;}
        let facing=max(dot(normal,-direction),0.0)*strength;
        var sun=0.0;
        if facing>0.0 {sun=facing*sun_visibility(v.world_position,normal);}
        // Interior lights exist only in lightmaps; the brighter of the sun
        // and that baked light, so dark maps' lamps light players and bricks.
        illumination=max(camera.ambient.rgb+camera.sun_color.rgb*sun,baked_surroundings(v.world_position,v.normal))
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
