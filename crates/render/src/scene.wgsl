// The live environment (bri_content::atmosphere) is in sun_direction,
// sun_color, ambient, fog_color and atmosphere; sky is the sky's tint with
// the sun flare's size in w, flare its colour and strength, shadow_color the
// light where the sun does not reach (w 1 when set). baked_* are the map's
// own sun and ambient, as its lightmaps were baked; baked_sun_direction.w
// is 1 while the live light differs from them, and lightmaps are relit.
// sky_bands is the authored sky's colour by direction (scene.rs `SkyBands`).
// sky_sun is the direction toward the real sun (below the horizon too), w 1
// while the Enhanced sky replaces the authored one.
struct Camera {
    view_projection:mat4x4<f32>, eye:vec4<f32>, sun_direction:vec4<f32>,
    sun_color:vec4<f32>, ambient:vec4<f32>, fog_color:vec4<f32>, atmosphere:vec4<f32>,
    sky:vec4<f32>, flare:vec4<f32>, shadow_color:vec4<f32>,
    baked_sun_direction:vec4<f32>, baked_sun_color:vec4<f32>, baked_ambient:vec4<f32>,
    shading:vec4<f32>,
    sky_bands:array<vec4<f32>,128>,
    sky_sun:vec4<f32>, sky_color:vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera:Camera;
struct PointLight { position_radius:vec4<f32>, color:vec4<f32> };
// count.x lights; the grid (crate::light_grid) has its origin and inverse
// cell size in grid_origin, its dimensions and 1 when built in grid_dims.
struct PointLights { count:vec4<u32>, grid_origin:vec4<f32>, grid_dims:vec4<u32>, values:array<PointLight,256> };
@group(0) @binding(1) var<uniform> lights:PointLights;
// Cell table (list offset << 9 | light count), then each cell's lights.
@group(0) @binding(15) var<storage,read> light_grid:array<u32>;
// The grid cell's light list at `position`: its first word and length. Only
// the lights listed there can reach the point; outside the grid none can.
fn light_cell(position:vec3<f32>)->vec2<u32> {
    if(lights.grid_dims.w==0u) { return vec2<u32>(0u); }
    let cell=floor((position-lights.grid_origin.xyz)*lights.grid_origin.w);
    let dims=lights.grid_dims.xyz;
    if(any(cell<vec3<f32>(0.0))||any(cell>=vec3<f32>(dims))) { return vec2<u32>(0u); }
    let c=vec3<u32>(cell);
    let entry=light_grid[c.x+dims.x*(c.y+dims.y*c.z)];
    return vec2<u32>(entry>>9u,min(entry&511u,256u));
}
fn listed_light(list:vec2<u32>,i:u32)->PointLight {
    return lights.values[min(light_grid[list.x+i],255u)];
}
// Map surfaces and terrain: smooth finite-radius native falloff per pixel.
// Vertex-lit objects use vertex_point_illumination below.
fn point_illumination(position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    var result=vec3<f32>(0.0);
    let list=light_cell(position);
    let n=normal/max(length(normal),0.0001);
    for(var i=0u;i<list.y;i+=1u) {
        let light=listed_light(list,i);
        let delta=light.position_radius.xyz-position;
        let distance=length(delta);
        let falloff=max(1.0-distance/max(light.position_radius.w,0.0001),0.0);
        result+=light.color.rgb*falloff*falloff*max(dot(n,delta/max(distance,0.0001)),0.0);
    }
    return result;
}
// Vertex-lit objects (bricks, players, items, vehicles) take point lights
// as v20's fixed-function GL did (brick batcher 0x531860, see
// docs/audits/bricks.md): per vertex, colour x N.L x 1/(1 + 0.1 d^2) from
// each light whose radius reaches the vertex, interpolated across the face.
// So a lamp lights the corners it is near: a post under it, not the far
// middle of a baseplate, and a Brightness 5 light does not flood a street.
const GL_QUADRATIC_ATTENUATION:f32=0.1;
fn vertex_point_illumination(position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    var result=vec3<f32>(0.0);
    let list=light_cell(position);
    let n=normal/max(length(normal),0.0001);
    for(var i=0u;i<list.y;i+=1u) {
        let light=listed_light(list,i);
        let delta=light.position_radius.xyz-position;
        let d2=dot(delta,delta);
        let radius=light.position_radius.w;
        if d2>=radius*radius {continue;}
        let facing=max(dot(n,delta*inverseSqrt(max(d2,0.00000001))),0.0);
        result+=light.color.rgb*facing/(1.0+GL_QUADRATIC_ATTENUATION*d2);
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
    // Lamp shadows (shadow.rs): six faces per slot; per shaded map light
    // its slot (-1 none); (slots used, face resolution,
    // world texel per unit of distance, fade distance); position and reach.
    // Faces are tiles of shadow_map layers: (first layer, tiles per row,
    // tile share of a layer).
    lamp_faces:array<mat4x4<f32>,24>, light_slots:array<vec4<f32>,6>, lamp_params:vec4<f32>,
    // Moving casters' faces the same way, then their resolution.
    lamp_atlas:vec4<f32>, lamp_dynamic:vec4<f32>, lamp_centers:array<vec4<f32>,4>,
    // The map layer (layer cascade + 2 * count): per cascade, caster depth
    // to map-layer depth as depth * scale + offset; x 1 when drawn.
    map_scale:vec4<f32>, map_offset:vec4<f32>, map_params:vec4<f32>,
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
// Sun reaching a receiver past the map's own surfaces (the map layer): the
// same smooth 3x3 texel filter as lamp shadows, four bilinear comparisons.
fn map_cascade_lit(c:CascadeCoord)->f32 {
    let layer=c.cascade+2*i32(shadows.forward_count.w);
    let depth=c.depth*shadows.map_scale[c.cascade]+shadows.map_offset[c.cascade];
    let step=0.5/shadows.params.y;
    var lit=0.0;
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,c.uv+vec2<f32>(-step,-step),layer,depth);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,c.uv+vec2<f32>(step,-step),layer,depth);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,c.uv+vec2<f32>(-step,step),layer,depth);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,c.uv+vec2<f32>(step,step),layer,depth);
    return lit*0.25;
}
// Unified: the share of the sun reaching a brick, player, item or vehicle.
// Inside the shadow distance the map's own surfaces shade it from the map
// layer, so sun through a window lands with a filtered edge, exactly where
// it lights the walls and floor beside it; past it (and while the map layer
// is not drawn) the visibility volume's coarse sun stands in. Live casters
// shade it as everywhere.
fn object_sun(position:vec3<f32>,normal:vec3<f32>,vis:MapVisibility)->f32 {
    let c=shadow_coord(position,normal);
    if lighting_mode()==3 {return dynamic_sun(position,normal);}
    var map=vis.low.x;
    if c.near.cascade>=0 && shadows.map_params.x>0.0 {
        var lit=map_cascade_lit(c.near);
        if c.blend>0.0 {lit=mix(lit,map_cascade_lit(c.far),c.blend);}
        map=mix(map,lit,c.strength);
    }
    if map<=0.0 {return 0.0;}
    return min(map,shadow_lit(c));
}
// Lamp slot `slot`'s light reaching a surface past the same casters: the
// cube face the surface lies in, a 2x2 bilinear comparison per tap (so
// 3x3 texels), fading out with the eye distance like the sun's.
fn lamp_lit(slot:u32,position:vec3<f32>,normal:vec3<f32>)->f32 {
    let center=shadows.lamp_centers[slot];
    let n=normal/max(length(normal),0.0001);
    let distance=length(position-center.xyz);
    if distance>=center.w || distance<0.1 {return 1.0;}
    let fade=lamp_fade(position);
    if fade<=0.0 {return 1.0;}
    // Off the surface by a texel and a half at this distance, against acne.
    let f=lamp_face(slot,position+n*distance*shadows.lamp_params.z*1.5);
    // Kept brick faces, then this frame's moving casters.
    let lit=lamp_taps(shadows.lamp_atlas,shadows.lamp_params.y,f.index,f.uv,f.depth)
        *lamp_taps(shadows.lamp_dynamic,shadows.lamp_dynamic.w,f.index,f.uv,f.depth);
    return mix(1.0,lit,fade);
}
// Lamp shadows fade out with the eye distance like the sun's.
fn lamp_fade(position:vec3<f32>)->f32 {
    return clamp((shadows.lamp_params.w-length(position-shadows.origin.xyz))/(shadows.lamp_params.w*0.1),0.0,1.0);
}
// The cube face of lamp slot `slot` holding `p`: its index, where `p`
// falls in it, and its depth there.
struct LampFace { index:u32, uv:vec2<f32>, depth:f32 };
fn lamp_face(slot:u32,p:vec3<f32>)->LampFace {
    let index=slot*6u+cube_face(p-shadows.lamp_centers[slot].xyz);
    return face_point(index,shadows.lamp_faces[index],p);
}
// The cube face (+X, -X, +Y, -Y, +Z, -Z) holding direction `q`.
fn cube_face(q:vec3<f32>)->u32 {
    let a=abs(q);
    var face=select(4u,5u,q.z<0.0);
    if a.x>=a.y && a.x>=a.z {face=select(0u,1u,q.x<0.0);}
    else if a.y>=a.z {face=select(2u,3u,q.y<0.0);}
    return face;
}
fn face_point(index:u32,matrix:mat4x4<f32>,p:vec3<f32>)->LampFace {
    let clip=matrix*vec4<f32>(p,1.0);
    let ndc=clip.xyz/clip.w;
    let uv=clamp(ndc.xy*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5),vec2<f32>(0.0),vec2<f32>(1.0));
    return LampFace(index,uv,ndc.z);
}
// Dynamic mode: how much of recovered light `i` reaches past current map
// geometry, at any eye distance. -1 until a runtime cube cohort is available.
fn cube_seen(i:u32,position:vec3<f32>,n:vec3<f32>)->f32 {
    if map_lights.cube_atlas.x<0.5 {return -1.0;}
    let center=map_lights.values[i].position_inner.xyz;
    let delta=center-position;
    let distance=length(delta);
    if distance<0.1 {return 1.0;}
    // Off the surface as `lamp_reach` does, so a wall never shades itself.
    let toward=delta/distance;
    let texel=distance*map_lights.cube_params.y;
    let p=position+n*texel*(2.0+2.0*(1.0-max(dot(n,toward),0.0)))+toward*texel;
    let index=i*6u+cube_face(p-center);
    let f=face_point(index,map_lights.cube_faces[index],p);
    let atlas=vec4<f32>(map_lights.cube_atlas.yzw,0.0);
    // Lights past the Shadow Quality's soft count (cube_params.z, one bit
    // per light) take a single 2x2 comparison: a harder edge, far away.
    if ((bitcast<u32>(map_lights.cube_params.z)>>i)&1u)==0u {
        return lamp_tap(atlas,f.index,f.uv,f.depth);
    }
    return lamp_taps(atlas,map_lights.cube_params.x,f.index,f.uv,f.depth);
}
// Where map light `i` (shadow slot `slot`, or -1) reaches past the map's
// walls: its slot's map faces near the eye, else its visibility channel,
// or (Dynamic, a light without one) its cube.
fn light_seen(i:u32,slot:i32,position:vec3<f32>,n:vec3<f32>,v:ptr<function,MapVisibility>)->f32 {
    let channel=map_lights.values[i].channel.x;
    var seen=1.0;
    if channel>=0.0 {seen=channel_visibility(v,channel);}
    else {
        let cube=cube_seen(i,position,n);
        if cube>=0.0 {seen=cube;}
    }
    if slot>=0 {seen=lamp_reach(u32(slot),position,n,seen);}
    return seen;
}
// How much of lamp slot `slot`'s light reaches a surface past the map's
// own walls, from the slot's map faces (drawn with the map layer). The
// visibility volume (`volume`, the light's channel there) is only a few
// units coarse: beside furniture its cells can sit inside the geometry and
// hide a lamp from everything on the dresser beside it. Past the lamp
// shadow distance, and while the map faces are not drawn, the volume
// stands in.
fn lamp_reach(slot:u32,position:vec3<f32>,n:vec3<f32>,volume:f32)->f32 {
    if shadows.map_params.x<=0.0 {return volume;}
    let center=shadows.lamp_centers[slot];
    let delta=center.xyz-position;
    let distance=length(delta);
    if distance>=center.w {return volume;}
    if distance<0.1 {return 1.0;}
    let fade=lamp_fade(position);
    if fade<=0.0 {return volume;}
    // Off the surface by two texels, up to four as the light grazes it,
    // and one toward the lamp, so a wall never shades itself.
    let toward=delta/distance;
    let texel=distance*shadows.lamp_params.z;
    let p=position+n*texel*(2.0+2.0*(1.0-max(dot(n,toward),0.0)))+toward*texel;
    let f=lamp_face(slot,p);
    let atlas=vec4<f32>(shadows.map_params.y,shadows.lamp_dynamic.y,shadows.lamp_dynamic.z,0.0);
    return mix(volume,lamp_taps(atlas,shadows.lamp_dynamic.w,f.index,f.uv,f.depth),fade);
}
// One 2x2 bilinear comparison at `face_uv` in face `index`'s tile of an
// atlas, as `lamp_taps` places its four.
fn lamp_tap(atlas:vec4<f32>,index:u32,face_uv:vec2<f32>,depth:f32)->f32 {
    let tiles=u32(atlas.y);
    let tile=index%(tiles*tiles);
    let layer=i32(atlas.x)+i32(index/(tiles*tiles));
    let uv=(vec2<f32>(f32(tile%tiles),f32(tile/tiles))+face_uv)*atlas.z;
    return textureSampleCompareLevel(shadow_map,shadow_sampler,uv,layer,depth);
}
// A 2x2 bilinear comparison per tap (so 3x3 texels) in face `index`'s tile
// of an atlas (first layer, tiles per row, tile share of a layer).
fn lamp_taps(atlas:vec4<f32>,size:f32,index:u32,face_uv:vec2<f32>,depth:f32)->f32 {
    let tiles=u32(atlas.y);
    let tile=index%(tiles*tiles);
    let layer=i32(atlas.x)+i32(index/(tiles*tiles));
    let uv=(vec2<f32>(f32(tile%tiles),f32(tile/tiles))+face_uv)*atlas.z;
    let step=0.5*atlas.z/size;
    var lit=0.0;
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(-step,-step),layer,depth);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(step,-step),layer,depth);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(-step,step),layer,depth);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(step,step),layer,depth);
    return lit*0.25;

}
// The shadow slot of shaded map light `light` (under 24), or -1: one read
// of the table shadow.rs fills each frame, not a search of the slots for
// every light at every pixel.
fn lamp_slot(light:u32)->i32 {
    return i32(shadows.light_slots[light/4u][light%4u]);
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
// Lighting model (camera.ambient.w): 0 Classic, the v20 look (baked maps,
// sun-lit bricks, live shadows darken lightmaps by a fixed share); 2 Unified
// (map_lighting.rs, with specular highlights on objects); 3 Dynamic uses
// independent live illumination and current geometry shadows on all surfaces.
// 1 was Unified without highlights and draws as 2.
fn lighting_mode()->i32 {return i32(camera.ambient.w+0.5);}
// The live environment differs from the map's baked sun or ambient.
fn relit()->bool {return camera.baked_sun_direction.w>0.5;}
// The map's own sun and ambient, as its lightmaps were baked: the live
// values unless the environment was changed (callers may leave baked_*
// unset then).
fn baked_ambient()->vec3<f32> {return select(camera.ambient.rgb,camera.baked_ambient.rgb,relit());}
fn baked_sun_color()->vec3<f32> {return select(camera.sun_color.rgb,camera.baked_sun_color.rgb,relit());}
fn baked_sun_direction()->vec3<f32> {
    let d=select(camera.sun_direction.xyz,camera.baked_sun_direction.xyz,relit());
    return d/max(length(d),0.0001);
}
// The live sun comes from where the map's lightmaps were baked from.
fn baked_sun_direction_kept()->bool {
    return !relit() || distance(camera.sun_direction.xyz,camera.baked_sun_direction.xyz)<0.0001;
}
// Light where the sun does not reach, blending to the ambient light as
// `reach` (the share of the sun a surface facing it receives) rises.
fn ambient_at(reach:f32)->vec3<f32> {
    if camera.shadow_color.w<0.5 {return camera.ambient.rgb;}
    return mix(camera.shadow_color.rgb,camera.ambient.rgb,clamp(reach,0.0,1.0));
}
// Sky-tinted ambient (camera.shading, Unified and Dynamic only): faces turned
// up take the ambient light in the sky's colour at the same brightness, faces
// turned down a darker one, level faces the flat ambient unchanged. Nothing
// gets brighter, so enclosed rooms stay as dark as before. `normal` is unit
// length.
override MODERN_SKY_SHADING:bool=false;
const HEMISPHERE_DOWN:f32=0.75;
fn hemisphere(normal:vec3<f32>)->vec3<f32> {
    if !MODERN_SKY_SHADING || camera.shading.x<=0.0 || lighting_mode()==0 {return vec3<f32>(1.0);}
    let tinted=select(mix(vec3<f32>(1.0),vec3<f32>(HEMISPHERE_DOWN),-normal.y),
        mix(vec3<f32>(1.0),camera.shading.yzw,normal.y),normal.y>0.0);
    return mix(vec3<f32>(1.0),tinted,camera.shading.x);
}
// The live sun reaching a lightmapped texel whose bake let `baked` of the
// map's own sun through. From the map's baked direction: that share, past
// live casters. From a new direction: the map's own surfaces in the shadow
// cascades near the eye; past them the baked share stands in, as the
// texel's openness to the sky.
fn relit_sun(baked:f32,position:vec3<f32>,n:vec3<f32>)->f32 {
    if baked_sun_direction_kept() {
        if baked<=0.0 {return 0.0;}
        return min(baked,sun_visibility(position,n));
    }
    let c=shadow_coord(position,n);
    var map=baked;
    if c.near.cascade>=0 && shadows.map_params.x>0.0 {
        var lit=map_cascade_lit(c.near);
        if c.blend>0.0 {lit=mix(lit,map_cascade_lit(c.far),c.blend);}
        map=mix(map,lit,c.strength);
    }
    if map<=0.0 {return 0.0;}
    return min(map,shadow_lit(c));
}
// An interior lightmap with its decomposition (map_lighting::decompose_sheet
// in material slot 9): RGB the static light, A the share of the sun the bake
// let through. In Unified mode a live shadow takes away only the sun the
// texel actually had, so baked shade is never darkened twice; unshadowed, the
// mission lightmap shows exactly as baked.
fn decomposed_lightmap(mission:vec3<f32>,parts:vec4<f32>,uv:vec2<f32>,position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    if lighting_mode()==0 && !relit() {return shadowed_lightmap(mission,position,normal);}
    let n=normal/max(length(normal),0.0001);
    let direction=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    let facing=max(dot(n,-direction),0.0);
    var sun=parts.a;
    if facing>0.0 && sun>0.0 {sun=min(sun,sun_visibility(position,n));}
    if relit() && facing>0.0 {sun=relit_sun(parts.a,position,n);}
    // The bake's own sun, which the live sun replaces.
    let baked_direction=baked_sun_direction();
    let baked_facing=max(dot(n,-baked_direction),0.0);
    // A lamp's live shadow takes away that lamp's share of the texel's
    // static light (as the map compiler lit it: no cosine), and a light
    // dimmed, recoloured or switched off at run time (`light_tint`) takes
    // away the part of its share it no longer gives. Beside the lights it
    // placed the fit is least exact and can claim more light than the
    // texel holds; there a light takes only its proportion of the fitted
    // light (with the mission ambient, which always stays), so a shadow or
    // a switched-off light is never darker than the light it really gave.
    var shaded=vec3<f32>(0.0);
    let tinted=map_lights.count.y!=0u;
    let channels=min(u32(material[1].y),24u);
    if channels>0u && (shadows.lamp_params.x>0.0 || tinted) {
        // The bake's per-texel shares (map_lighting::DynamicSheet), when the
        // material carries them: exactly the light each gave this texel, as
        // the Dynamic mode switches it.
        let seen=channel_shares(uv,channels);
        for(var c=0u;c<channels;c+=1u) {
            let i=channel_light(c);
            if i<min(map_lights.count.z,24u) {
                let light=map_lights.values[i];
                let given=light_given(light,position)*seen[c/4u][c%4u];
                var lit=1.0;
                let slot=lamp_slot(i);
                if slot>=0 {lit=lamp_lit(u32(slot),position,n);}
                shaded+=given*max(vec3<f32>(1.0)-light_tint(light)*lit,vec3<f32>(0.0));
            }
        }
        shaded=min(shaded,parts.rgb);
    } else if shadows.lamp_params.x>0.0 || tinted {
        let vis=map_visibility(position,n);
        var v=vis;
        for(var i=0u;i<min(map_lights.count.x,24u);i+=1u) {
            let slot=lamp_slot(i);
            if slot<0 && !tinted {continue;}
            let light=map_lights.values[i];
            let delta=light.position_inner.xyz-position;
            let distance=length(delta);
            let outer=light.color_outer.w;
            if vis.state==0u || distance>=outer || dot(n,delta)<=0.0 {continue;}
            let seen=light_seen(i,slot,position,n,&v);
            var lit=1.0;
            if slot>=0 {lit=lamp_lit(u32(slot),position,n);}
            if seen<=0.0 {continue;}
            let inner=light.position_inner.w;
            let share=light.color_outer.rgb*clamp((outer-distance)/max(outer-inner,0.001),0.0,1.0)*seen;
            shaded+=share*max(vec3<f32>(1.0)-light_tint(light)*lit,vec3<f32>(0.0));
        }
        if any(shaded>vec3<f32>(0.0)) {
            let total=map_light_total(position,n,vis)+baked_ambient();
            shaded=min(shaded*parts.rgb/max(total,max(parts.rgb,vec3<f32>(0.001))),parts.rgb);
        }
    }
    let baked=min(parts.rgb+baked_sun_color()*baked_facing*parts.a,vec3<f32>(1.0));
    let ambient=camera.ambient.rgb-baked_ambient();
    let live=clamp(parts.rgb-shaded+ambient+camera.sun_color.rgb*facing*sun,vec3<f32>(0.0),vec3<f32>(1.0));
    return max(mission-(baked-live),vec3<f32>(0.0));
}
// Every recovered light reaching a map surface as the map compiler lit it
// (no cosine), through the visibility volume (or a shadowed lamp's map
// faces).
fn map_light_total(position:vec3<f32>,n:vec3<f32>,visibility:MapVisibility)->vec3<f32> {
    var total=vec3<f32>(0.0);
    if visibility.state==0u {return total;}
    var vis=visibility;
    for(var i=0u;i<min(map_lights.count.x,24u);i+=1u) {
        let light=map_lights.values[i];
        let delta=light.position_inner.xyz-position;
        let distance=length(delta);
        let outer=light.color_outer.w;
        if distance>=outer || dot(n,delta)<=0.0 {continue;}
        let seen=light_seen(i,lamp_slot(i),position,n,&vis);
        if seen<=0.0 {continue;}
        let inner=light.position_inner.w;
        total+=light.color_outer.rgb*clamp((outer-distance)/max(outer-inner,0.001),0.0,1.0)*seen;
    }
    return total;
}
// Dynamic's authoritative sun visibility comes only from current geometry.
// Beyond the bounded cascade range it fades to unshadowed, never old masks.
fn dynamic_sun(position:vec3<f32>,normal:vec3<f32>)->f32 {
    let c=shadow_coord(position,normal);
    var map=1.0;
    if c.near.cascade>=0 && shadows.map_params.x>0.0 {
        var lit=map_cascade_lit(c.near);
        if c.blend>0.0 {lit=mix(lit,map_cascade_lit(c.far),c.blend);}
        map=mix(1.0,lit,c.strength);
    }
    return min(map,shadow_lit(c));
}
// Descriptors plus current geometry cubes: no legacy visibility input exists
// in this function. Uniform ambient approximates unreconstructed indirect light.
fn dynamic_light_sum(position:vec3<f32>,normal:vec3<f32>,power:f32)->LocalLight {
    var out=LocalLight(vec3<f32>(0.0),vec3<f32>(0.0));
    let n=normal/max(length(normal),0.0001);
    let eye=normalize(camera.eye.xyz-position);
    for(var i=0u;i<min(map_lights.count.x,24u);i+=1u) {
        let light=map_lights.values[i];
        let delta=light.position_inner.xyz-position;
        let distance=length(delta);
        let facing=max(dot(n,delta)/max(distance,0.0001),0.0);
        if distance>=light.color_outer.w || facing<=0.0 {continue;}
        var seen=1.0;
        if shadows.params.x>0.0 {
            let cube=cube_seen(i,position,n);
            // A first-use cube has no geometry result yet. Keep the current
            // lamp explicitly unshadowed instead of converting -1 to darkness.
            // Geometry refreshes reuse only previously rendered runtime cubes.
            if cube>=0.0 {seen=cube;}
        }
        let slot=lamp_slot(i);
        if slot>=0 {seen*=lamp_lit(u32(slot),position,n);}
        let rgb=light_given(light,position)*light_tint(light)*seen;
        out.diffuse+=rgb*facing;
        out.specular+=rgb*glint(n,delta/max(distance,0.0001),eye,power);
    }
    return out;
}
fn dynamic_illumination(position:vec3<f32>,normal:vec3<f32>)->LocalLight {
    let n=normal/max(length(normal),0.0001);
    let toward=-camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    let facing=max(dot(n,toward),0.0);
    let sun=dynamic_sun(position,n);
    let local=dynamic_light_sum(position,n,SPECULAR_POWER);
    let eye=normalize(camera.eye.xyz-position);
    // Preserve main's expression when off. Multiplying ambient by a uniform
    // one lets the compiler fuse a different multiply-add at half-byte edges.
    if !MODERN_SKY_SHADING || camera.shading.x<=0.0 || lighting_mode()==0 {
        return LocalLight(ambient_at(sun)+camera.sun_color.rgb*facing*sun+local.diffuse,
            (camera.sun_color.rgb*sun*select(0.0,highlight(n,toward,eye),facing>0.0)+local.specular)*SPECULAR_STRENGTH);
    }
    return LocalLight(ambient_at(sun)*hemisphere(n)+camera.sun_color.rgb*facing*sun+local.diffuse,
        (camera.sun_color.rgb*sun*select(0.0,highlight(n,toward,eye),facing>0.0)+local.specular)*SPECULAR_STRENGTH);
}
// A stored share of 1 (map_lighting::SHARE_ONE levels of 255), so a
// lamp's bright spot can hold more than its fitted light.
const SHARE_SCALE:f32=255.0/128.0;
// The shares of its lights a lightmap texel holds, four channels to a
// material slot 1..=6 (map_lighting::DynamicSheet).
fn channel_shares(uv:vec2<f32>,count:u32)->array<vec4<f32>,6> {
    var seen=array<vec4<f32>,6>();
    if count>0u {seen[0]=textureSampleLevel(layer1,clamped_exact,uv,0.0)*SHARE_SCALE;}
    if count>4u {seen[1]=textureSampleLevel(layer2,clamped_exact,uv,0.0)*SHARE_SCALE;}
    if count>8u {seen[2]=textureSampleLevel(layer3,clamped_exact,uv,0.0)*SHARE_SCALE;}
    if count>12u {seen[3]=textureSampleLevel(layer4,clamped_exact,uv,0.0)*SHARE_SCALE;}
    if count>16u {seen[4]=textureSampleLevel(layer5,clamped_exact,uv,0.0)*SHARE_SCALE;}
    if count>20u {seen[5]=textureSampleLevel(layer6,clamped_exact,uv,0.0)*SHARE_SCALE;}
    return seen;
}
// A map light as the map compiler lit a surface at `position` facing it:
// its colour by its linear falloff, no cosine.
fn light_given(l:MapLight,position:vec3<f32>)->vec3<f32> {
    let distance=length(l.position_inner.xyz-position);
    let outer=l.color_outer.w;
    let inner=l.position_inner.w;
    return l.color_outer.rgb*clamp((outer-distance)/max(outer-inner,0.001),0.0,1.0);
}
// Light channel `c` of a decomposed material in the Dynamic mode: two light
// indices to a parameter float (map_lighting::pack_channels).
fn channel_light(c:u32)->u32 {
    let f=c/2u;
    let word=u32(material[2u+f/4u][f%4u]+0.5);
    let index=select(word%32u,word/32u,c%2u==1u);
    // Its place in the uniform (24, past every light, when not there).
    let slot=map_lights.slots[index/4u][index%4u];
    return select(24u,u32(slot+0.5),slot>=0.0 && index<24u);
}
// The mission terrain lightmap is ambient plus sun times its baked
// visibility; the visibility is recovered from the lightmap itself, and a
// live shadow removes only the sun share that is there.
fn terrain_light(lightmap:vec3<f32>,position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    if relit() {return relit_terrain(lightmap,position,normal);}
    if lighting_mode()==0 {return shadowed_lightmap(lightmap,position,normal);}
    let n=normal/max(length(normal),0.0001);
    let direction=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    let facing=max(dot(n,-direction),0.0);
    let direct=max(lightmap-camera.ambient.rgb,vec3<f32>(0.0));
    if facing<=0.0 || all(direct<=vec3<f32>(0.004)) {return lightmap;}
    let weights=vec3<f32>(0.2126,0.7152,0.0722);
    let baked=clamp(dot(direct,weights)/max(dot(camera.sun_color.rgb,weights)*facing,0.02),0.0,1.0);
    let live=sun_visibility(position,n);
    return lightmap-direct*(1.0-min(1.0,live/max(baked,0.001)));
}
// A terrain lightmap under the live environment: its baked sun visibility
// recovered against the map's own sun, then lit again by the live ambient
// and sun (the terrain lightmap holds nothing else).
fn relit_terrain(lightmap:vec3<f32>,position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    let n=normal/max(length(normal),0.0001);
    let weights=vec3<f32>(0.2126,0.7152,0.0722);
    let baked_direction=baked_sun_direction();
    let baked_facing=max(dot(n,-baked_direction),0.0);
    let direct=max(lightmap-baked_ambient(),vec3<f32>(0.0));
    // Faces the bake's sun never reached say nothing: open sky.
    var baked=1.0;
    if baked_facing>0.02 {
        baked=clamp(dot(direct,weights)/max(dot(baked_sun_color(),weights)*baked_facing,0.02),0.0,1.0);
    }
    let direction=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    let facing=max(dot(n,-direction),0.0);
    var sun=0.0;
    if facing>0.0 {sun=relit_sun(baked,position,n);}
    return min(ambient_at(sun)+camera.sun_color.rgb*facing*sun,vec3<f32>(1.0));
}
// A map's recovered lights (map_lighting.rs) with a visibility volume from
// its geometry: two RGBA blocks stacked along z, (sun, channels 0-2) then
// (channels 3-6), each padded by one copied layer. dims.w==0: no volume, so
// the sun reaches everywhere and there are no map lights.
// channel: visibility channel, then the run-time tint (`light_tint`).
struct MapLight { position_inner:vec4<f32>, color_outer:vec4<f32>, channel:vec4<f32> };
// A light's colour and brightness now against the fitted light: 1 as
// fitted, 0 switched off (a broken bulb, an Add-On). count.y is 1 while any
// light differs, which is the only time map surfaces pay for it.
fn light_tint(light:MapLight)->vec3<f32> {
    return light.channel.yzw;
}
// cube_atlas and cube_params: the Dynamic mode's light cubes (1 when drawn,
// first layer, faces per row, face share of a layer; face resolution, world
// texel per unit of distance, the soft lights' bits); cube_faces each light's
// six face matrices.
struct MapLights {
    origin_cell:vec4<f32>, dims:vec4<f32>, count:vec4<u32>, values:array<MapLight,24>,
    slots:array<vec4<f32>,6>, cube_atlas:vec4<f32>, cube_params:vec4<f32>, cube_faces:array<mat4x4<f32>,144>,
};
@group(0) @binding(13) var visibility_volume:texture_3d<f32>;
@group(0) @binding(14) var<uniform> map_lights:MapLights;
// low: (sun, channels 0-2), read for every surface; high: channels 3-6,
// read only once a light on them is in reach (state 1 until then, 2 after;
// 0 outside the volume).
struct MapVisibility { low:vec4<f32>, high:vec4<f32>, coord:vec3<f32>, state:u32 };
fn map_visibility(position:vec3<f32>,normal:vec3<f32>)->MapVisibility {
    var out=MapVisibility(vec4<f32>(1.0,0.0,0.0,0.0),vec4<f32>(0.0),vec3<f32>(0.0),0u);
    if map_lights.dims.w==0.0 {return out;}
    let n=normal/max(length(normal),0.0001);
    // Half a cell off the surface, so a wall's own cells do not shade it.
    let cell=map_lights.origin_cell.w;
    let t=(position+n*cell*0.5-map_lights.origin_cell.xyz)/cell;
    let dims=map_lights.dims.xyz;
    if any(t<vec3<f32>(0.0)) || any(t>dims) {return out;}
    let layers=dims.z*2.0+2.0;
    let z=clamp(t.z,0.5,dims.z-0.5);
    let xy=t.xy/dims.xy;
    out.low=textureSampleLevel(visibility_volume,clamped_exact,vec3<f32>(xy,z/layers),0.0);
    out.coord=vec3<f32>(xy,(z+dims.z+2.0)/layers);
    out.state=1u;
    return out;
}
fn channel_visibility(v:ptr<function,MapVisibility>,channel_word:f32)->f32 {
    // A light the Dynamic mode shades without a channel: its cube (or
    // nothing) says where it reaches.
    if channel_word<0.0 {return 1.0;}
    let k=u32(channel_word)+1u;
    if k<4u {return (*v).low[k];}
    if (*v).state==1u {
        (*v).high=textureSampleLevel(visibility_volume,clamped_exact,(*v).coord,0.0);
        (*v).state=2u;
    }
    return (*v).high[k-4u];
}
// Blinn-Phong highlights (Unified and Dynamic) on bricks, players, items and
// vehicles, from the same lights, falloff and visibility as their diffuse
// light. Map surfaces keep their original baked look: no highlights.
const SPECULAR_POWER:f32=40.0;
const SPECULAR_STRENGTH:f32=0.3;
fn highlight(n:vec3<f32>,toward_light:vec3<f32>,toward_eye:vec3<f32>)->f32 {
    return glint(n,toward_light,toward_eye,SPECULAR_POWER);
}
fn glint(n:vec3<f32>,toward_light:vec3<f32>,toward_eye:vec3<f32>,power:f32)->f32 {
    let h=normalize(toward_light+toward_eye);
    return pow(max(dot(n,h),0.0),power);
}
struct LocalLight { diffuse:vec3<f32>, specular:vec3<f32> };
// The map compiler's light: full colour to the inner radius, then linear to
// nothing at the outer, on every surface facing it. The map's own surfaces
// were lit without a cosine (that is how the lights were fitted); objects
// take the engine's usual N.L (`lambert`), which gives bricks their form.
// Objects keep this share of a light on every face turned toward it (the
// rest follows N.L): the map's own surfaces took the light in full (no
// cosine), so a brick beside a wall matches its brightness while its faces
// still read apart.
const LAMBERT_FLOOR:f32=0.5;
// `power` sharpens the highlights (SPECULAR_POWER for painted surfaces).
fn map_light_sum(position:vec3<f32>,normal:vec3<f32>,visibility:MapVisibility,lambert:bool,power:f32)->LocalLight {
    var out=LocalLight(vec3<f32>(0.0),vec3<f32>(0.0));
    if lighting_mode()==3 {return dynamic_light_sum(position,normal,power);}
    if visibility.state==0u {return out;}
    var vis=visibility;
    let n=normal/max(length(normal),0.0001);
    let toward_eye=normalize(camera.eye.xyz-position);
    for(var i=0u;i<min(map_lights.count.x,24u);i+=1u) {
        let light=map_lights.values[i];
        let delta=light.position_inner.xyz-position;
        let distance=length(delta);
        let outer=light.color_outer.w;
        if distance>=outer || dot(n,delta)<=0.0 {continue;}
        // A shadowed lamp's reach comes from its own faces: the map's walls,
        // then the casters.
        let slot=lamp_slot(i);
        var seen=light_seen(i,slot,position,n,&vis);
        if slot>=0 {seen*=lamp_lit(u32(slot),position,n);}
        if seen<=0.0 {continue;}
        let inner=light.position_inner.w;
        let light_rgb=light.color_outer.rgb*light_tint(light)*clamp((outer-distance)/max(outer-inner,0.001),0.0,1.0)*seen;
        out.diffuse+=light_rgb*select(1.0,LAMBERT_FLOOR+(1.0-LAMBERT_FLOOR)*dot(n,delta)/max(distance,0.0001),lambert);
        out.specular+=light_rgb*glint(n,delta/max(distance,0.0001),toward_eye,power);
    }
    return out;
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
    // Point light at the vertex (vertex-lit materials only).
    @location(6) point_light:vec3<f32>,
    // Signed distance past the instance's clip plane; cut below zero.
    @location(7) clip:f32,
};
@vertex fn vs_main(@location(0) local_position:vec3<f32>,@location(1) local_normal:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) lightmap_uv:vec2<f32>,@location(4) local_color:vec4<f32>,
    @location(5) m0:vec4<f32>,@location(6) m1:vec4<f32>,@location(7) m2:vec4<f32>,@location(8) m3:vec4<f32>,@location(9) tint:vec4<f32>,
    @location(10) fx_data:vec4<f32>,@location(11) clip:vec4<f32>)->VertexOut {
    let model=mat4x4<f32>(m0,m1,m2,m3);
    let position=(model*vec4<f32>(local_position,1.0)).xyz;
    // Cofactor matrix is det(M)*inverse-transpose(M). Positive affine
    // determinants are validated on the CPU; preserve nonuniform scale normals.
    let cofactor=mat3x3<f32>(cross(m1.xyz,m2.xyz),cross(m2.xyz,m0.xyz),cross(m0.xyz,m1.xyz));
    let normal=(cofactor*local_normal)/dot(m0.xyz,cross(m1.xyz,m2.xyz));
    let color=local_color*tint;
    var out:VertexOut;out.position=camera.view_projection*vec4<f32>(position,1.0);
    out.uv=uv;out.lightmap_uv=lightmap_uv;out.color=color;out.normal=normal;out.world_position=position;
    out.clip=dot(clip.xyz,position)+clip.w;
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
    out.point_light=vec3<f32>(0.0);
    if material[0].x==2.0 || material[0].x==3.0 || material[0].x==9.0 {
        out.point_light=vertex_point_illumination(out.world_position,out.normal);
    }
    if material[0].x==6.0 {
        let phase=vec2<f32>(position.x+1024.0,1024.0-position.z)*0.05+vec2<f32>(camera.atmosphere.z);
        out.world_position.y+=(sin(phase.x)+sin(phase.y))*material[1].z*0.25;
        out.normal=normalize(vec3<f32>(-cos(phase.x)*material[1].z*0.0125,1.0,cos(phase.y)*material[1].z*0.0125));
        out.position=camera.view_projection*vec4<f32>(out.world_position,1.0);
    }
    if (material[0].x==4.0 || material[0].x==5.0) {
        out.world_position=camera.eye.xyz+position;
        out.position=camera.view_projection*vec4<f32>(camera.eye.xyz+position,1.0);
        // At the far plane: depth 0, reversed (scene.rs `DEPTH_CLEAR`).
        out.position.z=0.0;
        if material[0].x==5.0 {out.uv=uv+fract(normal.xy*camera.atmosphere.z);}
    }
    return out;
}
// The sun's disc and glow along `along` (display colour to add), from the
// live sun: camera.flare's colour times its strength, camera.sky.w its size.
fn sun_flare(along:vec3<f32>)->vec3<f32> {
    if camera.flare.a<=0.0 {return vec3<f32>(0.0);}
    let toward=-camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    let angle=acos(clamp(dot(along,toward),-1.0,1.0));
    let size=max(camera.sky.w,0.01);
    let disc=1.0-smoothstep(0.009*size,0.012*size,angle);
    let glow=pow(max(1.0-angle/(0.2*size),0.0),3.0);
    return camera.flare.rgb*camera.flare.a*(disc+0.55*glow);
}
// Enhanced sky ("Sky: Enhanced", off in Classic's v20 look): single scattering
// through a Rayleigh and Mie atmosphere, solved in closed form per ray for the
// real sun (camera.sky_sun), so a blue zenith, a pale horizon, a warm sunrise
// and sunset and a blue-purple twilight follow the server's time of day.
fn enhanced_sky_on()->bool {return MODERN_SKY_SHADING && camera.sky_sun.w>0.5;}
// Relative air mass along a ray `c` from the zenith (Kasten and Young).
fn air_mass(c:f32)->f32 {
    let cc=clamp(c,0.0,1.0);
    return 1.0/(cc+0.50572*pow(max(96.07995-degrees(acos(cc)),0.001),-1.6364));
}
const SKY_RAYLEIGH:vec3<f32>=vec3<f32>(0.05,0.12,0.30);
const SKY_OZONE:vec3<f32>=vec3<f32>(0.010,0.016,0.003);
const SKY_MIE:f32=0.012;
const SKY_MIE_G:f32=0.72;
fn smooth_between(a:f32,b:f32,x:f32)->f32 {
    let t=clamp((x-a)/(b-a),0.0,1.0);
    return t*t*(3.0-2.0*t);
}
// Sky radiance along `along` in display colour. Below the horizon it is the
// horizon's, unless the sky goes on below it (a sky_below map such as
// Skylands), where it mirrors the sky above.
fn enhanced_sky(along:vec3<f32>)->vec3<f32> {
    let sun=camera.sky_sun.xyz;
    let h=select(max(along.y,0.0),abs(along.y),camera.fog_color.w>0.5);
    let mu=dot(along,sun);
    let sun_air=air_mass(max(sun.y,0.0));
    let ext=SKY_RAYLEIGH+vec3<f32>(SKY_MIE);
    // Low views look through the thick air the sun's light has crossed; high
    // ones through a thin, still-blue layer.
    let reach=clamp(0.12+0.88*(1.0-h),0.0,1.0);
    let light=exp(-(ext+SKY_OZONE)*sun_air*reach);
    // Direct atmospheric scattering fades below the horizon; retaining it
    // until -0.30 made the whole twilight sky red instead of its low edge.
    let day=smooth_between(-0.16,0.02,sun.y);
    let rayleigh=0.75*(1.0+mu*mu);
    let g=SKY_MIE_G;
    let mie=(1.0-g*g)/pow(1.0+g*g-2.0*g*mu,1.5);
    let scatter=SKY_RAYLEIGH*rayleigh+vec3<f32>(SKY_MIE*mie);
    let path=(vec3<f32>(1.0)-exp(-ext*air_mass(h)))/ext;
    var color=3.2*light*scatter*path*day;
    let warmth=1.0-smooth_between(0.02,0.25,sun.y);
    let neutral=dot(color,vec3<f32>(0.2126,0.7152,0.0722));
    color=mix(color,vec3<f32>(neutral),0.45*warmth);
    // Twilight: the high air still lit after the sun has set, then the night.
    let twilight=smooth_between(-0.32,-0.04,sun.y)*(1.0-smooth_between(0.05,0.3,sun.y));
    color+=vec3<f32>(0.018,0.04,0.12)*twilight*(0.3+0.7*h);
    color+=vec3<f32>(0.004,0.008,0.022)*(1.0-day);
    // Desaturate sunlight alone, retaining the blue twilight and night floor.
    return display_color(vec3<f32>(1.0)-exp(-color))*camera.sky_color.rgb;
}
// The sun's and moon's discs along `along`, display colour to add after fog.
fn sky_bodies(along:vec3<f32>)->vec3<f32> {
    let sun=camera.sky_sun.xyz;
    if along.y<=-0.02 {return vec3<f32>(0.0);}
    let ext=SKY_RAYLEIGH+vec3<f32>(SKY_MIE);
    let seen=smooth_between(-0.02,0.01,along.y);
    let day=smooth_between(-0.30,0.02,sun.y);
    let mu=dot(along,sun);
    let sun_angle=acos(clamp(mu,-1.0,1.0));
    var color=exp(-ext*air_mass(max(sun.y,0.0)))*40.0*(1.0-smooth_between(0.014,0.018,sun_angle))*seen*smooth_between(-0.06,0.0,sun.y);
    let moon_angle=acos(clamp(-mu,-1.0,1.0));
    let moon=(1.0-smooth_between(0.010,0.0125,moon_angle))+0.08*pow(max(1.0-moon_angle/0.12,0.0),3.0);
    color+=vec3<f32>(0.9,0.95,1.1)*moon*seen*(1.0-day);
    // Stars: sparse, steady points on a cell grid, fading in with the night.
    if day<0.9 {
        let cell=floor(along*70.0);
        let h=fract(sin(dot(cell,vec3<f32>(12.9898,78.233,37.719)))*43758.5453);
        if h>0.97 {
            let spot=cell+vec3<f32>(0.5)+0.3*vec3<f32>(fract(h*91.7)-0.5,fract(h*57.3)-0.5,fract(h*33.1)-0.5);
            let near=length(along*70.0-spot);
            let twinkle=0.5+0.5*fract(h*13.7);
            color+=vec3<f32>(0.85,0.9,1.0)*twinkle*(1.0-smooth_between(0.05,0.2,near))*seen*(1.0-smooth_between(0.0,0.9,day))*0.8;
        }
    }
    return min(color,vec3<f32>(1.0))*camera.sky_color.rgb;
}
// The Enhanced sky at the horizon toward `along`: what the world's edge fogs
// to, in place of the authored fog colour.
fn enhanced_horizon(along:vec3<f32>)->vec3<f32> {
    return enhanced_sky(normalize(vec3<f32>(along.x,0.0,along.z)+vec3<f32>(0.0,0.0,0.00001)));
}
fn fog_amount(position:vec3<f32>)->f32 {
    return fog_along(position-camera.eye.xyz,camera.atmosphere,camera.fog_color.w);
}
// One band of the sky's faces (8 around, 16 up), tinted; the fog backdrop
// where they leave it uncovered.
fn sky_band(column:i32,row:i32)->vec3<f32> {
    let band=camera.sky_bands[u32(row)*8u+u32((column+8)%8)];
    return mix(camera.fog_color.rgb,band.rgb*camera.sky.rgb,band.a);
}
// What geometry at `position` fogs toward: the sky behind it, as the sky
// faces draw it under the fog of the world's edge (sun flare and clouds
// aside). Low and level it is the fog colour; high up, more of the sky's
// own, so tall far terrain meets the sky it fades into instead of standing
// pale against it.
fn fog_target(position:vec3<f32>)->vec3<f32> {
    let offset=position-camera.eye.xyz;
    let along=offset/max(length(offset),0.0001);
    if enhanced_sky_on() {
        return mix(enhanced_sky(along),enhanced_horizon(along),sky_fog_at(along.y,camera.atmosphere,camera.fog_color.w));
    }
    let around=(atan2(along.x,along.z)/(2.0*PI)+0.5)*8.0-0.5;
    let up=clamp((sign(along.y)*sqrt(abs(along.y))*0.5+0.5)*16.0-0.5,0.0,15.0);
    let column=i32(floor(around));
    let row=i32(floor(up));
    let above=min(row+1,15);
    let wa=around-floor(around);
    let face=mix(
        mix(sky_band(column,row),sky_band(column+1,row),wa),
        mix(sky_band(column,above),sky_band(column+1,above),wa),
        up-floor(up));
    return mix(face,camera.fog_color.rgb,sky_fog_at(along.y,camera.atmosphere,camera.fog_color.w));
}
fn fogged(display:vec3<f32>,position:vec3<f32>)->vec3<f32> {
    let fog=fog_amount(position);
    if fog<=0.0 {return output_color(display);}
    return output_color(mix(display,fog_target(position),fog));
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
    var color=clamp(mix(lit,fog_target(position),fog),vec3<f32>(0.0),vec3<f32>(1.0));
    let zero_bump=material[1].y;
    if lighting_mode()!=3 && (flags&2u)!=0u && distance<zero_bump {
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
fn shade_surface(v:VertexOut)->vec4<f32> {
    if v.clip<0.0 {discard;}
    // The sky (faces, clouds, fog backdrop) is what lies behind the world:
    // everything else fades out into it toward the visible distance.
    if material[0].x!=4.0 && material[0].x!=5.0
        && faded_out(v.position.xy,v.world_position-camera.eye.xyz,camera.atmosphere) {discard;}
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
        // Authored surface/reflection textures are daylight colours. Apply
        // the live environment before fog, including distant water and shore.
        var water_light:vec3<f32>;
        if lighting_mode()==3 {
            water_light=dynamic_illumination(v.world_position,v.normal).diffuse
                +point_illumination(v.world_position,v.normal);
        } else {
        let live_up=max(-normalize(camera.sun_direction.xyz).y,0.0);
        let baked_up=max(-baked_sun_direction().y,0.0);
        let daylight=baked_ambient()+baked_sun_color()*baked_up;
        water_light=select(vec3<f32>(1.0),clamp((camera.ambient.rgb+camera.sun_color.rgb*live_up)/max(daylight,vec3<f32>(0.001)),vec3<f32>(0.0),vec3<f32>(4.0)),relit());
        }
        rgb*=water_light;
        if depth_mapped {
            // fluid::CalcVertSpecular, added (SRC_ALPHA, ONE) under the depth
            // mask: colour.rgb*colour.a*pow(half.up,power)^2, sun as light 0.
            let light=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
            let half_vector=normalize(normalize(camera.eye.xyz-v.world_position)-light);
            let facing=max(half_vector.y,0.0);
            var shine=1.0;
            if material[4].w>0.0 {shine=select(0.0,pow(facing,material[4].w),facing>0.0);}
            rgb+=material[4].rgb*shine*shine*a/alpha*water_light;
        }
        return vec4<f32>(fogged(rgb,v.world_position),alpha);
    }
    if material[0].x==10.0 {return metal_surface(v);}
    if (material[0].x==4.0 || material[0].x==5.0) {
        // material[1].x: 0 a sky face or cloud (tinted by the sky colour),
        // 1 the fog backdrop (the live fog colour). Faces and clouds take the
        // fog of the world's edge in front of them; far geometry fades into them.
        let along=normalize(v.world_position-camera.eye.xyz);
        if enhanced_sky_on() && material[0].x==4.0 {
            // The faces and the backdrop under them draw the one procedural
            // sky, under the same fog far geometry fades into.
            let fog=sky_fog_at(along.y,camera.atmosphere,camera.fog_color.w);
            let rgb=mix(enhanced_sky(along),enhanced_horizon(along),fog)+sky_bodies(along);
            return vec4<f32>(output_color(min(rgb,vec3<f32>(1.0))),v.color.a);
        }
        if material[1].x==1.0 {
            let fog=min(camera.fog_color.rgb+sun_flare(along),vec3<f32>(1.0));
            return vec4<f32>(output_color(fog),v.color.a);
        }
        let fog=sky_fog_at(along.y,camera.atmosphere,camera.fog_color.w);
        var sky=textureSample(layer0,clamped,v.uv);
        if material[0].x==5.0 {
            sky=textureSample(layer0,tiled,v.uv);
            let edge=select(camera.fog_color.rgb,enhanced_horizon(along),enhanced_sky_on());
            let cloud=mix(display_color(sky.rgb)*v.color.rgb*camera.sky.rgb,edge,fog);
            return vec4<f32>(output_color(cloud),sky.a*v.color.a);
        }
        let tinted=mix(display_color(sky.rgb)*v.color.rgb*camera.sky.rgb,camera.fog_color.rgb,fog);
        let rgb=min(tinted+sun_flare(along),vec3<f32>(1.0));
        return vec4<f32>(output_color(rgb),sky.a*v.color.a);
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
        var terrain_lit:vec3<f32>;
        if lighting_mode()==3 {terrain_lit=dynamic_illumination(v.world_position,v.normal).diffuse;}
        else {terrain_lit=terrain_light(textureSampleLevel(lightmap,tiled_exact,light_uv,0.0).rgb,v.world_position,v.normal);}
        return vec4<f32>(terrain_passes(diffuse*v.color.rgb*(terrain_lit+point_illumination(v.world_position,v.normal)),v.world_position),v.color.a);
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
    // A model texture with no translucent texel: texture times light alone.
    if (flags&4u)!=0u {pigment=display_color(albedo.rgb);}
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
    var baked_light=vec4<f32>(1.0);
    if lighting_mode()!=3 {baked_light=textureSampleLevel(lightmap,clamped_exact,v.lightmap_uv,0.0);}
    var illumination=baked_light.rgb;
    var specular=vec3<f32>(0.0);
    let sun_toward=-camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    if material[0].x==2.0 || material[0].x==3.0 || surfaces {
        let normal=v.normal/max(length(v.normal),0.0001);
        let direction=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
        // Water keeps the lengthened GL normal; chrome doubles its normals.
        var strength=1.0;
        if fx.y==2u {strength=length(v.normal);}
        if fx.x==2u {strength*=2.0;}
        let facing=max(dot(normal,-direction),0.0)*strength;
        var sun=0.0;
        if lighting_mode()==0 {
            var reach=0.0;
            if facing>0.0 {reach=sun_visibility(v.world_position,normal);sun=facing*reach;}
            // Classic: interior lights exist only in lightmaps; the brighter of
            // the sun and that baked light, so dark maps' lamps light players
            // and bricks.
            illumination=max(ambient_at(reach)+camera.sun_color.rgb*sun,baked_surroundings(v.world_position,v.normal))
                +v.point_light*strength;
        } else if lighting_mode()==3 {
            let live=dynamic_illumination(v.world_position,v.normal);
            illumination=live.diffuse+point_illumination(v.world_position,v.normal)*strength;
            specular=live.specular;
        } else {
            // Unified: the map's own model. Sun where the map's geometry and
            // live casters let it through, the recovered lights through their
            // visibility, and the light the fit leaves over (the residual
            // volume bound in these modes).
            let vis=map_visibility(v.world_position,normal);
            var sun_share=0.0;
            if facing>0.0 {sun_share=object_sun(v.world_position,normal,vis);}
            sun=facing*sun_share;
            let local=map_light_sum(v.world_position,normal,vis,true,SPECULAR_POWER);
            illumination=ambient_at(sun_share)+camera.sun_color.rgb*sun+local.diffuse*strength
                +baked_surroundings(v.world_position,v.normal)
                +v.point_light*strength;
            if MODERN_SKY_SHADING && camera.shading.x>0.0 && lighting_mode()!=0 {
                illumination=ambient_at(sun_share)*hemisphere(normal)+camera.sun_color.rgb*sun+local.diffuse*strength
                +baked_surroundings(v.world_position,v.normal)
                +v.point_light*strength;
            }
            let toward_eye=normalize(camera.eye.xyz-v.world_position);
            specular=(camera.sun_color.rgb*sun_share*select(0.0,highlight(normal,sun_toward,toward_eye),facing>0.0)
                +local.specular)*SPECULAR_STRENGTH;
        }
        if fx.x==3u {
            // Glow aims the normal at the sun, 1/min(1, sun rgb) long.
            var shortest=1.0;
            for(var c=0;c<3;c+=1) {if camera.sun_color[c]>0.0 {shortest=min(shortest,camera.sun_color[c]);}}
            illumination=camera.ambient.rgb+camera.sun_color.rgb/shortest;
        }
    } else if lighting_mode()==3 {
        let live=dynamic_illumination(v.world_position,v.normal);
        illumination=live.diffuse+point_illumination(v.world_position,v.normal);
        specular=live.specular;
    } else if material[1].x==1.0 {
        illumination=decomposed_lightmap(baked_light.rgb,textureSample(weights0,clamped_exact,v.lightmap_uv),v.lightmap_uv,v.world_position,v.normal)
            +point_illumination(v.world_position,v.normal);
    } else {
        illumination=shadowed_lightmap(illumination,v.world_position,v.normal)
            +point_illumination(v.world_position,v.normal);
        // A lightmap without its decomposition: only the ambient light
        // changes with the live environment.
        if relit() {illumination=max(illumination+camera.ambient.rgb-baked_ambient(),vec3<f32>(0.0));}
    }
    var display=pigment*illumination;
    // Fixed-function lighting clamps the vertex colour before texturing.
    if decal {display=mix(min(display,vec3<f32>(1.)),albedo.rgb,albedo.a);}
    display+=specular;
    return vec4<f32>(fogged(display,v.world_position),alpha);
}

@fragment fn fs_main(v:VertexOut)->@location(0) vec4<f32> {
    return shade_surface(v);
}
// Reuse the surface's cut-out, clip and far-fade rules. Only visible opaque
// emissive faces write into the AO exclusion mask, under an equal-depth test.
@fragment fn fs_occlusion_mask(v:VertexOut)->@location(0) vec4<f32> {
    let glow=v.fx.x==3u && (material[0].x==2.0 || material[0].x==3.0 || material[0].x==9.0);
    let unlit=material[0].x==7.0 || material[0].x==8.0;
    if !glow && !unlit {discard;}
    let surface=shade_surface(v);
    if surface.a<=0.0 {discard;}
    return vec4<f32>(1.0);
}

// ---- Bare metal (MaterialKind::Metal) ----
// The metallic workflow real-time engines use: no diffuse colour, only
// reflection. Its surroundings come from the environment probe, a small
// cube of the world drawn around the nearest metal object
// (crate::environment_probe); past the probe's reach, and with no live
// probe, from a sky made of the map's own colours. The sun and the map's
// lights add GGX highlights. Lit in linear light, then display encoded
// like every other surface. material[1]: roughness, detail repeats, detail
// strength; material[2].rgb: reflectance at normal incidence (linear).
struct Probe { centre_reach:vec4<f32>, state:vec4<f32> };
@group(0) @binding(17) var probe_sampler:sampler;
@group(0) @binding(18) var<uniform> probe:Probe;
const PI:f32=3.14159265;
// A sky built from the map's fog (its horizon) and ambient light: brighter
// toward the horizon, deeper overhead, the ground's shade below.
fn sky_along(r:vec3<f32>)->vec3<f32> {
    let horizon=linear_color(clamp(camera.fog_color.rgb,vec3<f32>(0.0),vec3<f32>(1.0)));
    let ambient=linear_color(clamp(camera.ambient.rgb,vec3<f32>(0.0),vec3<f32>(1.0)));
    let zenith=mix(horizon,horizon*vec3<f32>(0.55,0.68,0.95),0.6);
    let ground=mix(ambient*0.45,horizon*0.25,0.4);
    if r.y>=0.0 {return mix(horizon,zenith,pow(r.y,0.6));}
    return mix(horizon*0.6,ground,pow(min(-r.y*3.0,1.0),0.5));
}
// Where direction `d` lies in the probe's octahedral map, y up.
fn probe_uv(d:vec3<f32>)->vec2<f32> {
    let n=d/max(abs(d.x)+abs(d.y)+abs(d.z),0.0001);
    var p=n.xz;
    if n.y<0.0 {
        let s=select(vec2<f32>(-1.0),vec2<f32>(1.0),p>=vec2<f32>(0.0));
        p=(vec2<f32>(1.0)-abs(p.yx))*s;
    }
    return p*0.5+vec2<f32>(0.5);
}
// What a mirror at `position` sees along `r`, blurred for `roughness`.
fn environment_along(position:vec3<f32>,r:vec3<f32>,roughness:f32)->vec3<f32> {
    let sky=sky_along(r);
    if probe.state.x<0.5 {return sky;}
    // Blurrier mips for rougher metal (Unity's perceptual mapping).
    let lod=probe.state.y*roughness*(1.7-0.7*roughness);
    // The probe's octahedral map is in slot 2 (FOLD in environment_probe.rs).
    var seen=textureSampleLevel(layer2,probe_sampler,probe_uv(r),lod).rgb;
    if OUTPUT_ENCODED==1u {seen=linear_color(seen);}
    // Far from the probe its picture is of somewhere else.
    let away=distance(position,probe.centre_reach.xyz);
    return mix(seen,sky,smoothstep(probe.state.z,probe.state.w,away));
}
// Karis' fit to the split-sum environment BRDF.
fn environment_brdf(f0:vec3<f32>,roughness:f32,nv:f32)->vec3<f32> {
    let r=roughness*vec4<f32>(-1.0,-0.0275,-0.572,0.022)+vec4<f32>(1.0,0.0425,1.04,-0.04);
    let a=min(r.x*r.x,exp2(-9.28*nv))*r.x+r.y;
    let ab=vec2<f32>(-1.04,1.04)*a+r.zw;
    return f0*ab.x+vec3<f32>(ab.y);
}
// GGX distribution, height-correlated Smith visibility and Schlick's
// Fresnel, times N.L: one light's share.
fn ggx_light(n:vec3<f32>,l:vec3<f32>,e:vec3<f32>,alpha:f32,f0:vec3<f32>)->vec3<f32> {
    let nl=dot(n,l);
    if nl<=0.0 {return vec3<f32>(0.0);}
    let h=normalize(l+e);
    let nh=max(dot(n,h),0.0);
    let nv=max(dot(n,e),0.0001);
    let a2=alpha*alpha;
    let d=nh*nh*(a2-1.0)+1.0;
    let distribution=a2/(PI*d*d);
    let vis=0.5/(nl*sqrt(nv*nv*(1.0-a2)+a2)+nv*sqrt(nl*nl*(1.0-a2)+a2));
    let fresnel=f0+(vec3<f32>(1.0)-f0)*pow(1.0-max(dot(l,h),0.0),5.0);
    return fresnel*distribution*vis*nl;
}
// A normal perturbed by the detail texture's tilt, in a frame built from
// screen derivatives (no tangents needed; Schueler's cotangent frame).
fn detailed_normal(n:vec3<f32>,p:vec3<f32>,uv:vec2<f32>,tilt:vec2<f32>)->vec3<f32> {
    let dp1=dpdx(p);let dp2=dpdy(p);let duv1=dpdx(uv);let duv2=dpdy(uv);
    let dp2perp=cross(dp2,n);let dp1perp=cross(n,dp1);
    let t=dp2perp*duv1.x+dp1perp*duv2.x;
    let b=dp2perp*duv1.y+dp1perp*duv2.y;
    let scale=inverseSqrt(max(max(dot(t,t),dot(b,b)),1e-20));
    return normalize(n+(t*tilt.x+b*tilt.y)*scale);
}
fn metal_surface(v:VertexOut)->vec4<f32> {
    let base=textureSample(layer0,tiled,v.uv).rgb*linear_color(clamp(v.color.rgb,vec3<f32>(0.0),vec3<f32>(1.0)));
    let detail=textureSample(layer1,tiled,v.uv*material[1].y);
    var n=v.normal/max(length(v.normal),0.0001);
    let tilt=(detail.ba-vec2<f32>(0.5))*2.0*material[1].z;
    n=detailed_normal(n,v.world_position,v.uv*material[1].y,tilt);
    let e=normalize(camera.eye.xyz-v.world_position);
    if dot(n,e)<0.0 {n=normalize(n+e*(0.01-dot(n,e)));}
    let roughness=clamp(material[1].x*detail.r*2.0,0.03,1.0);
    let cavity=detail.g;
    let f0=clamp(material[2].rgb*base,vec3<f32>(0.0),vec3<f32>(1.0));
    let nv=max(dot(n,e),0.0001);
    let r=reflect(-e,n);
    var radiance=environment_along(v.world_position,r,roughness)*environment_brdf(f0,roughness,nv);
    let alpha=max(roughness*roughness,0.002);
    // The sun where it reaches the surface, as for other objects.
    let travels=camera.sun_direction.xyz/max(length(camera.sun_direction.xyz),0.0001);
    let toward_sun=-travels;
    let sun_rgb=linear_color(clamp(camera.sun_color.rgb,vec3<f32>(0.0),vec3<f32>(1.0)));
    if lighting_mode()==0 {
        radiance+=sun_rgb*ggx_light(n,toward_sun,e,alpha,f0)*sun_visibility(v.world_position,n);
    } else {
        var vis=MapVisibility(vec4<f32>(1.0,0.0,0.0,0.0),vec4<f32>(0.0),vec3<f32>(0.0),0u);
        if lighting_mode()!=3 {vis=map_visibility(v.world_position,n);}
        var sun_share=0.0;
        if dot(n,toward_sun)>0.0 {sun_share=object_sun(v.world_position,n,vis);}
        radiance+=sun_rgb*ggx_light(n,toward_sun,e,alpha,f0)*sun_share;
        // The map's lights as Blinn-Phong lobes of matching width, normalised.
        let power=clamp(2.0/(alpha*alpha)-2.0,4.0,4096.0);
        let local=map_light_sum(v.world_position,n,vis,false,power);
        radiance+=linear_color(min(local.specular,vec3<f32>(1.0)))*f0*(power+8.0)/(8.0*PI);
    }
    // Brick and player lights, with the falloff other objects take
    // (vertex_point_illumination), as highlights.
    let list=light_cell(v.world_position);
    for(var i=0u;i<list.y;i+=1u) {
        let light=listed_light(list,i);
        let delta=light.position_radius.xyz-v.world_position;
        let d2=dot(delta,delta);
        let radius=light.position_radius.w;
        if d2>=radius*radius {continue;}
        let rgb=linear_color(clamp(light.color.rgb,vec3<f32>(0.0),vec3<f32>(1.0)));
        radiance+=rgb*ggx_light(n,delta*inverseSqrt(max(d2,0.00000001)),e,alpha,f0)/(1.0+GL_QUADRATIC_ATTENUATION*d2);
    }
    radiance*=cavity;
    return vec4<f32>(fogged(display_color(radiance),v.world_position),1.0);
}
