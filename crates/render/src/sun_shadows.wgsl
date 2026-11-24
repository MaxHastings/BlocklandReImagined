// Sun shadow filtering shared by the classic and modern scene pipelines.
struct CascadeCoord { uv:vec2<f32>, depth:f32, cascade:i32, slope:vec2<f32> };
fn cascade_coord(position:vec3<f32>,n:vec3<f32>,cascade:i32)->CascadeCoord {
    var out:CascadeCoord;
    // Offset along the normal by the cascade's texel size against acne.
    let clip=shadows.matrices[cascade]*vec4<f32>(position+n*shadows.texels[cascade]*1.5,1.0);
    out.uv=clip.xy*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5);
    out.depth=clip.z;
    out.cascade=cascade;
    out.slope=vec2<f32>(0.0);
    if shadows.params.z>0.0 {
        // The receiver's plane in the map: for an orthographic map, the clip
        // normal is the world normal's clip direction over each axis' squared
        // scale. Depth change per clip unit, clamped where the plane is edge
        // on to the sun (no direct light there anyway), then per uv unit.
        let m=shadows.matrices[cascade];
        let r0=vec3<f32>(m[0].x,m[1].x,m[2].x);
        let r1=vec3<f32>(m[0].y,m[1].y,m[2].y);
        let r2=vec3<f32>(m[0].z,m[1].z,m[2].z);
        let g=(m[0].xyz*n.x+m[1].xyz*n.y+m[2].xyz*n.z)/vec3<f32>(dot(r0,r0),dot(r1,r1),dot(r2,r2));
        let edge=select(1.0,-1.0,g.z<0.0)*max(abs(g.z),1e-9);
        // Ten units across per unit down: steeper is nearly edge on.
        let limit=10.0*shadows.depth_scale[cascade]*shadows.texels[cascade]*shadows.params.y*0.5;
        let slope=clamp(-g.xy/edge,vec2<f32>(-limit),vec2<f32>(limit));
        out.slope=slope*vec2<f32>(2.0,-2.0);
    }
    return out;
}
fn inside_map(c:CascadeCoord)->bool {
    let margin=3.0/shadows.params.y;
    return all(c.uv>vec2<f32>(margin)) && all(c.uv<vec2<f32>(1.0-margin)) && c.depth>0.0 && c.depth<1.0;
}
// Two fitted cascades and the handoff to the fixed distant map.
struct ShadowCoord { near:CascadeCoord, far:CascadeCoord, blend:f32, strength:f32, far_blend:f32 };
fn shadow_coord(position:vec3<f32>,normal:vec3<f32>)->ShadowCoord {
    var out:ShadowCoord;
    out.near.cascade=-1;
    out.blend=0.0;
    out.far_blend=0.0;
    let count=i32(shadows.forward_count.w);
    if count<=0 {return out;}
    let view_depth=dot(position-shadows.origin.xyz,shadows.forward_count.xyz);
    let last=shadows.splits[count-1];
    out.far_blend=smoothstep(0.75*last,last,view_depth);
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
        let blend_start=shadows.splits[cascade]-band;
        if band>0.0 && view_depth>blend_start {
            out.far=cascade_coord(position,n,cascade+1);
            if inside_map(out.far) {
                out.blend=clamp((view_depth-blend_start)/band,0.0,1.0);
            }
        }
    }
    // Classic and baked receivers fade live casters over the last tenth.
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
// PCSS sun shadows (the PCSS Sun Shadows option; shadows.params.z is 1).
// The caster layer is searched around the receiver for blockers; their mean
// distance back along the sun sets how wide the filter disc is, so a shadow
// is sharp where its caster touches the surface and soft far from it.
const PCSS_SEARCH_TAPS:i32=8;
const PCSS_FILTER_TAPS:i32=16;
// The disc never grows past this many texels, nor shrinks below the width of
// the ordinary 3x3 filter's softest edge.
const PCSS_MAX_TEXELS:f32=10.0;
// The far map's texels are deliberately coarse, so its penumbra search is
// capped lower: an unscaled radius would reach across whole shadow edges and
// smear false partial shade onto lit faces at range.
const PCSS_FAR_MAX_TEXELS:f32=4.0;
const PCSS_MIN_TEXELS:f32=1.5;
const PCSS_MAP_SEARCH_TAPS:i32=16;
const PCSS_MAP_FILTER_TAPS:i32=32;

// A pixel's own noise, so taps rotate differently from pixel to pixel.
var<private> sun_pixel:vec2<f32>;
fn pcss_noise()->f32 {
    return fract(52.9829189*fract(dot(floor(sun_pixel),vec2<f32>(0.06711056,0.00583715))));
}
// Tap i of n on a unit disc, turned by angle; points spread evenly.
fn pcss_disc(i:i32,n:i32,angle:f32)->vec2<f32> {
    let a=f32(i)*2.3999632+angle;
    return sqrt((f32(i)+0.5)/f32(n))*vec2<f32>(cos(a),sin(a));
}
// One bilinear comparison at uv against receiver depth z with the same caster
// and occluder rules as cascade_lit; 1 is lit.
fn pcss_tap(uv:vec2<f32>,z:f32,cascade:i32,occluder_layer:i32,gap:f32)->f32 {
    let size=shadows.params.y;
    let f=uv*size-vec2<f32>(0.5);
    // The 2x2 texel block holding the point, kept on the map.
    let base=clamp(floor(f),vec2<f32>(0.0),vec2<f32>(size-2.0));
    let t=clamp(f-base,vec2<f32>(0.0),vec2<f32>(1.0));
    let corner=(base+vec2<f32>(1.0))/size;
    let casters=textureGather(shadow_map,shadow_point,corner,cascade);
    let occluders=textureGather(shadow_map,shadow_point,corner,occluder_layer);
    let between=occluders>casters+vec4<f32>(gap) & occluders<vec4<f32>(z-gap);
    let lit=select(vec4<f32>(1.0),vec4<f32>(0.0),casters<vec4<f32>(z) & !between);
    // Gather order is (0,1), (1,1), (1,0), (0,0).
    return dot(lit,vec4<f32>((1.0-t.x)*t.y,t.x*t.y,t.x*(1.0-t.y),(1.0-t.x)*(1.0-t.y)));
}
// The sum of a vector's four components, written out: DXC on Windows rejects
// dot() against a constant vector of ones here ("DXIL intrinsic overload").
fn pcss_sum4(v:vec4<f32>)->f32 {return v.x+v.y+v.z+v.w;}
fn pcss_lit(c:CascadeCoord)->f32 {
    let size=shadows.params.y;
    let texel=shadows.texels[c.cascade];
    let scale=shadows.depth_scale[c.cascade];
    let occluder_layer=c.cascade+i32(shadows.forward_count.w);
    let gap=0.1*scale;
    // Half a texel of depth, so a surface never shadows itself.
    let bias=0.5*texel*scale;
    let angle=6.2831853*pcss_noise();
    let last=i32(size)-1;
    let depth_minus_bias=c.depth-bias;
    // The four texels around the receiver always count, so a shadow even one
    // texel wide is never missed by the sparser ring below. Each is compared
    // at its own place on the receiver's plane.
    let core_base=clamp(floor(c.uv*size-vec2<f32>(0.5)),vec2<f32>(0.0),vec2<f32>(size-2.0));
    let core=textureGather(shadow_map,shadow_point,(core_base+vec2<f32>(1.0))/size,c.cascade);
    let core_to_texel=(core_base+vec2<f32>(0.5))/size-c.uv;
    let core_z=vec4<f32>(depth_minus_bias)+vec4<f32>(
        dot(c.slope,core_to_texel+vec2<f32>(0.0,1.0)/size),
        dot(c.slope,core_to_texel+vec2<f32>(1.0,1.0)/size),
        dot(c.slope,core_to_texel+vec2<f32>(1.0,0.0)/size),
        dot(c.slope,core_to_texel),
    );
    let core_blocks=core<core_z;
    var blocker_sum=pcss_sum4(select(vec4<f32>(0.0),core,core_blocks));
    var weights=pcss_sum4(select(vec4<f32>(0.0),vec4<f32>(1.0),core_blocks));
    // Then a ring out to the widest the filter can grow, so a pixel just
    // outside a shadow still finds it and the soft edge spreads to both sides
    // of the shadow's true edge. Nearer taps count more, which keeps the
    // distance estimate local to the edge in question.
    let max_texels_over_size=PCSS_MAX_TEXELS/size;
    let search_weight_scale=1.0/f32(PCSS_SEARCH_TAPS);
    for (var i=0;i<PCSS_SEARCH_TAPS;i+=1) {
        let offset=pcss_disc(i,PCSS_SEARCH_TAPS,angle)*max_texels_over_size;
        let uv=c.uv+offset;
        let z=depth_minus_bias+dot(c.slope,offset);
        let texel_at=clamp(vec2<i32>(floor(uv*size)),vec2<i32>(0),vec2<i32>(last));
        let d=textureLoad(shadow_map,texel_at,c.cascade,0);
        if d<z {
            let w=1.0-(f32(i)+0.5)*search_weight_scale;
            blocker_sum+=w*d;
            weights+=w;
        }
    }
    // Nothing in front of the receiver nearby: lit, whatever the occluders.
    if weights==0.0 {return 1.0;}
    let blocker_distance=(c.depth-blocker_sum/weights)/scale;
    let radius=clamp(blocker_distance*shadows.params.w/texel,PCSS_MIN_TEXELS,PCSS_MAX_TEXELS);
    var lit=0.0;
    let radius_over_size=radius/size;
    let filter_weight_scale=1.0/f32(PCSS_FILTER_TAPS);
    for (var i=0;i<PCSS_FILTER_TAPS;i+=1) {
        let offset=pcss_disc(i,PCSS_FILTER_TAPS,angle)*radius_over_size;
        let z=depth_minus_bias+dot(c.slope,offset);
        lit+=pcss_tap(c.uv+offset,z,c.cascade,occluder_layer,gap);
    }
    return lit*filter_weight_scale;
}
// Smooth 3x3 percentage-closer filter from a 4x4 texel footprint, with
// bilinear edge weights; 1 is fully lit.
fn cascade_lit(c:CascadeCoord)->f32 {
    if shadows.params.z>0.0 {return pcss_lit(c);}
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
    if c.near.cascade<0 || c.strength<=0.0 {return 1.0;}
    var lit=cascade_lit(c.near);
    // Only receivers in the blend band pay for the second cascade.
    if c.blend>0.0 {lit=mix(lit,cascade_lit(c.far),c.blend);}
    return mix(1.0,lit,c.strength);
}
// Sun light reaching a vertex-lit surface past bricks, players and models.
fn sun_visibility(position:vec3<f32>,normal:vec3<f32>)->f32 {
    return shadow_lit(shadow_coord(position,normal));
}

// One bilinear comparison at uv against the receiver's plane in both layers
// at once; 1 is lit. The caster layer keeps its occluder rule (a brick that
// does not cast stops a shadow from passing); every map surface casts, so the
// map factor is a plain comparison at its own rescaled depth.
fn combined_pcss_tap(uv:vec2<f32>,z:f32,z_map:f32,cascade:i32,occluder_layer:i32,map_layer:i32,gap:f32)->f32 {
    let size=shadows.params.y;
    let f=uv*size-vec2<f32>(0.5);
    // The 2x2 texel block holding the point, kept on the map.
    let base=clamp(floor(f),vec2<f32>(0.0),vec2<f32>(size-2.0));
    let t=clamp(f-base,vec2<f32>(0.0),vec2<f32>(1.0));
    let corner=(base+vec2<f32>(1.0))/size;
    let casters=textureGather(shadow_map,shadow_point,corner,cascade);
    let occluders=textureGather(shadow_map,shadow_point,corner,occluder_layer);
    let between=occluders>casters+vec4<f32>(gap) & occluders<vec4<f32>(z-gap);
    let blocked=casters<vec4<f32>(z) & !between;
    if all(blocked) {return 0.0;}
    let lit_caster=select(vec4<f32>(1.0),vec4<f32>(0.0),blocked);
    // The map layer over the same 2x2 block, compared at its own rescaled
    // receiver depth.
    let maps=textureGather(shadow_map,shadow_point,corner,map_layer);
    let lit_map=select(vec4<f32>(1.0),vec4<f32>(0.0),maps<vec4<f32>(z_map));
    // A texel is lit only where neither layer blocks it; bilinear over the
    // per-texel product (for 0/1 values that is the per-texel min), not over
    // the two separate interpolations.
    return dot(lit_caster*lit_map,vec4<f32>((1.0-t.x)*t.y,t.x*t.y,t.x*(1.0-t.y),(1.0-t.x)*(1.0-t.y)));
}
// Caster and map blocker distances are accumulated in world units to
// choose one penumbra radius. Every filter tap uses both layers.
fn combined_pcss_lit(c:CascadeCoord)->f32 {
    let size=shadows.params.y;
    let texel=shadows.texels[c.cascade];
    let scale=shadows.depth_scale[c.cascade];
    let occluder_layer=c.cascade+i32(shadows.forward_count.w);
    let map_layer=c.cascade+2*i32(shadows.forward_count.w);
    let gap=0.1*scale;
    // Half a texel of depth, in each layer's own units, so a surface never
    // shadows itself.
    let bias=0.5*texel*scale;
    let map_scale=shadows.map_scale[c.cascade];
    let z_map=c.depth*map_scale+shadows.map_offset[c.cascade];
    let bias_map=0.5*texel*scale*map_scale;
    // World units per unit of each layer's stored depth, so blocker distances
    // from both layers accumulate in the same units.
    let inv_scale=1.0/scale;
    let inv_scale_map=inv_scale/map_scale;
    let angle=6.2831853*pcss_noise();
    let last=i32(size)-1;
    let depth_minus_bias=c.depth-bias;
    let z_map_minus_bias=z_map-bias_map;
    // The four texels around the receiver always count, so a shadow even one
    // texel wide is never missed by the sparser ring below. Each layer is
    // compared at its own place on the receiver's plane.
    let core_base=clamp(floor(c.uv*size-vec2<f32>(0.5)),vec2<f32>(0.0),vec2<f32>(size-2.0));
    let corner=(core_base+vec2<f32>(1.0))/size;
    let core_to_texel=(core_base+vec2<f32>(0.5))/size-c.uv;
    // Gather order is (0,1), (1,1), (1,0), (0,0); the map layer's plane
    // offsets are the same dots rescaled by its depth scale.
    let s=vec4<f32>(
        dot(c.slope,core_to_texel+vec2<f32>(0.0,1.0)/size),
        dot(c.slope,core_to_texel+vec2<f32>(1.0,1.0)/size),
        dot(c.slope,core_to_texel+vec2<f32>(1.0,0.0)/size),
        dot(c.slope,core_to_texel),
    );
    var blocker_sum=0.0;
    var weights=0.0;
    let core_caster=textureGather(shadow_map,shadow_point,corner,c.cascade);
    let caster_blocks=core_caster<vec4<f32>(depth_minus_bias)+s;
    blocker_sum+=pcss_sum4(select(vec4<f32>(0.0),c.depth-core_caster,caster_blocks))*inv_scale;
    weights+=pcss_sum4(select(vec4<f32>(0.0),vec4<f32>(1.0),caster_blocks));
    let core_map=textureGather(shadow_map,shadow_point,corner,map_layer);
    let map_blocks=core_map<vec4<f32>(z_map_minus_bias)+s*map_scale;
    blocker_sum+=pcss_sum4(select(vec4<f32>(0.0),z_map-core_map,map_blocks))*inv_scale_map;
    weights+=pcss_sum4(select(vec4<f32>(0.0),vec4<f32>(1.0),map_blocks));
    // Then a ring out to the widest the filter can grow, in both layers at
    // once, so a pixel just outside either shadow still finds it and the soft
    // edge spreads to both sides of its true edge. Nearer taps count more,
    // which keeps the distance estimate local to the edge in question.
    let max_texels_over_size=PCSS_MAX_TEXELS/size;
    let search_weight_scale=1.0/f32(PCSS_MAP_SEARCH_TAPS);
    for (var i=0;i<PCSS_MAP_SEARCH_TAPS;i+=1) {
        let offset=pcss_disc(i,PCSS_MAP_SEARCH_TAPS,angle)*max_texels_over_size;
        let uv=c.uv+offset;
        let texel_at=clamp(vec2<i32>(floor(uv*size)),vec2<i32>(0),vec2<i32>(last));
        // Each layer is compared at its own slope-adjusted plane but measured
        // from the receiver's unadjusted depth, as each filter did alone.
        let w=1.0-(f32(i)+0.5)*search_weight_scale;
        let d_caster=textureLoad(shadow_map,texel_at,c.cascade,0);
        let caster_here=d_caster<depth_minus_bias+dot(c.slope,offset);
        if caster_here {
            blocker_sum+=w*(c.depth-d_caster)*inv_scale;
            weights+=w;
        }
        let d_map=textureLoad(shadow_map,texel_at,map_layer,0);
        if d_map<z_map_minus_bias+dot(c.slope,offset)*map_scale {
            blocker_sum+=w*(z_map-d_map)*inv_scale_map;
            weights+=w;
        }
    }
    // Nothing in front of the receiver nearby, in either layer: lit.
    if weights==0.0 {return 1.0;}
    let radius=clamp(blocker_sum/weights*shadows.params.w/texel,PCSS_MIN_TEXELS,PCSS_MAX_TEXELS);
    var lit=0.0;
    let radius_over_size=radius/size;
    let filter_weight_scale=1.0/f32(PCSS_FILTER_TAPS);
    // The filter taps keep one fixed layout across all pixels (the search
    // ring above keeps its per-pixel angle for the radius estimate): every
    // pixel averages the same directions, so neighbouring pixels differ only
    // by position. A per-pixel rotation here would make each pixel's average
    // jitter against its neighbours and read as grain at the edge.
    for (var i=0;i<PCSS_FILTER_TAPS;i+=1) {
        let offset=pcss_disc(i,PCSS_FILTER_TAPS,0.0)*radius_over_size;
        lit+=combined_pcss_tap(
            c.uv+offset,
            depth_minus_bias+dot(c.slope,offset),
            z_map_minus_bias+dot(c.slope,offset)*map_scale,
            c.cascade,occluder_layer,map_layer,gap);
    }
    return lit*filter_weight_scale;
}

// Sun reaching a receiver past the map's own surfaces (the map layer): the
// same smooth 3x3 texel filter as lamp shadows, four bilinear comparisons.
// Used when Soft Sun Shadows is off; with it on the combined pass covers
// both layers at once.
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
// Sun reaching a receiver past the map's own surfaces far from the eye,
// from the far map layer: coarse texels over the whole map, filtered as
// the cascades' map layer is. 1 (unshadowed) outside it or while it is not
// drawn. `fallback` stands in then, as it did before the layer existed.
fn far_map_pcss_lit(uv:vec2<f32>,depth:f32,n:vec3<f32>,layer:i32)->f32 {
    let size=shadows.params.y;
    let texel=shadows.far_params.z;
    let m=shadows.far_matrix;
    let r0=vec3<f32>(m[0].x,m[1].x,m[2].x);
    let r1=vec3<f32>(m[0].y,m[1].y,m[2].y);
    let r2=vec3<f32>(m[0].z,m[1].z,m[2].z);
    let g=(m[0].xyz*n.x+m[1].xyz*n.y+m[2].xyz*n.z)/vec3<f32>(dot(r0,r0),dot(r1,r1),dot(r2,r2));
    let depth_scale=length(r2);
    let edge=select(1.0,-1.0,g.z<0.0)*max(abs(g.z),1e-9);
    let limit=10.0*depth_scale*texel*size*0.5;
    let slope=clamp(-g.xy/edge,vec2<f32>(-limit),vec2<f32>(limit))*vec2<f32>(2.0,-2.0);
    let bias=0.5*texel*depth_scale;
    let angle=6.2831853*pcss_noise();
    let core_base=clamp(floor(uv*size-vec2<f32>(0.5)),vec2<f32>(0.0),vec2<f32>(size-2.0));
    let core=textureGather(shadow_map,shadow_point,(core_base+vec2<f32>(1.0))/size,layer);
    let core_to_texel=(core_base+vec2<f32>(0.5))/size-uv;
    let core_z=vec4<f32>(depth-bias)+vec4<f32>(
        dot(slope,core_to_texel+vec2<f32>(0.0,1.0)/size),
        dot(slope,core_to_texel+vec2<f32>(1.0,1.0)/size),
        dot(slope,core_to_texel+vec2<f32>(1.0,0.0)/size),
        dot(slope,core_to_texel),
    );
    let core_blocks=core<core_z;
    var blocker_sum=pcss_sum4(select(vec4<f32>(0.0),core,core_blocks));
    var blockers=pcss_sum4(select(vec4<f32>(0.0),vec4<f32>(1.0),core_blocks));
    let search_weight_scale=1.0/f32(PCSS_MAP_SEARCH_TAPS);
    for (var i=0;i<PCSS_MAP_SEARCH_TAPS;i+=1) {
        let offset=pcss_disc(i,PCSS_MAP_SEARCH_TAPS,angle)*(PCSS_MAX_TEXELS/size);
        let tap_z=depth-bias+dot(slope,offset);
        let texel_at=clamp(vec2<i32>(floor((uv+offset)*size)),vec2<i32>(0),vec2<i32>(i32(size)-1));
        let d=textureLoad(shadow_map,texel_at,layer,0);
        if d<tap_z {
            let weight=1.0-(f32(i)+0.5)*search_weight_scale;
            blocker_sum+=weight*d;
            blockers+=weight;
        }
    }
    if blockers==0.0 {return 1.0;}
    let blocker_distance=(depth-blocker_sum/blockers)/depth_scale;
    let radius=clamp(blocker_distance*shadows.params.w/texel,PCSS_MIN_TEXELS,PCSS_FAR_MAX_TEXELS);
    var lit=0.0;
    for (var i=0;i<PCSS_MAP_FILTER_TAPS;i+=1) {
        let offset=pcss_disc(i,PCSS_MAP_FILTER_TAPS,angle)*(radius/size);
        lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+offset,layer,depth-bias+dot(slope,offset));
    }
    return lit/f32(PCSS_MAP_FILTER_TAPS);
}
fn far_map_lit(position:vec3<f32>,n:vec3<f32>,fallback:f32)->f32 {
    if shadows.far_params.x<=0.0 {return fallback;}
    let clip=shadows.far_matrix*vec4<f32>(position+n*shadows.far_params.z*1.5,1.0);
    let uv=clip.xy*vec2<f32>(0.5,-0.5)+vec2<f32>(0.5);
    if any(uv<vec2<f32>(0.0)) || any(uv>vec2<f32>(1.0)) || clip.z<=0.0 || clip.z>=1.0 {return fallback;}
    let layer=i32(shadows.far_params.y);
    if shadows.params.z>0.0 {return far_map_pcss_lit(uv,clip.z,n,layer);}
    let step=0.5*shadows.far_params.w;
    var lit=0.0;
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(-step,-step),layer,clip.z);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(step,-step),layer,clip.z);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(-step,step),layer,clip.z);
    lit+=textureSampleCompareLevel(shadow_map,shadow_sampler,uv+vec2<f32>(step,step),layer,clip.z);
    return lit*0.25;
}
// Resolve map and caster visibility within each cascade before blending.
fn cascade_sun(c:CascadeCoord)->f32 {
    if shadows.params.z>0.0 {return combined_pcss_lit(c);}
    return min(map_cascade_lit(c),cascade_lit(c));
}

// Dynamic and Unified receivers share the same shadow maps. Their callers
// supply the sun visibility to use outside the maps' coverage.
fn scene_sun(position:vec3<f32>,normal:vec3<f32>,fallback:f32)->f32 {
    let c=shadow_coord(position,normal);
    let n=normal/max(length(normal),0.0001);
    if c.near.cascade<0 {return far_map_lit(position,n,fallback);}
    if shadows.map_params.x<=0.0 {
        return min(far_map_lit(position,n,fallback),shadow_lit(c));
    }
    var lit=cascade_sun(c.near);
    if c.blend>0.0 {lit=mix(lit,cascade_sun(c.far),c.blend);}
    if c.far_blend>0.0 {
        lit=mix(lit,far_map_lit(position,n,fallback),c.far_blend);
    }
    return lit;
}
