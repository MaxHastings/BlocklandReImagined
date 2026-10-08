// The one fog every pass shares; `bri_content::environment::Fog` is its CPU
// twin and says how it works. atmosphere: fog start, fog end, unused, 1 when
// fog is on; below: 1 when the sky goes on below the horizon (fog_color.w).
fn fog_density(a:vec4<f32>)->f32 {return FOG_DEPTH/max(a.y-a.x,0.001);}
// How fast a ray whose direction rises `up` leaves the fog, per unit.
fn fog_rising(up:f32,below:f32)->f32 {
    return select(max(up,0.0),abs(up),below>0.5)/FOG_HEIGHT;
}
// Fog over the sky along a ray whose direction rises `up`: the air at the
// world's edge in front of it.
fn sky_fog_at(up:f32,a:vec4<f32>,below:f32)->f32 {
    if a.y<=0.0 {return 0.0;}
    return (1.0-exp(-FOG_DEPTH*exp(-fog_rising(up,below)*a.y)))*a.w;
}
// Fog over a point at `offset` from the eye.
fn fog_along(offset:vec3<f32>,a:vec4<f32>,below:f32)->f32 {
    if a.y<=0.0 {return 0.0;}
    let distance=length(offset);
    let up=offset.y/max(distance,0.0001);
    let inside=max(distance-a.x,0.0);
    let rise=fog_rising(up,below);
    let x=rise*inside;
    let spread=select((1.0-exp(-x))/x,1.0-0.5*x,x<0.0001);
    let depth=fog_density(a)*exp(-rise*a.x)*inside*spread;
    return (1.0-exp(-depth))*a.w;
}
// How far a point at `offset` has faded out into what lies behind it: 0
// before the last quarter of the fog's range, 1 at the visible distance
// and past it.
fn fog_edge(offset:vec3<f32>,a:vec4<f32>)->f32 {
    if a.y<=0.0 {return 0.0;}
    let t=clamp((length(offset)-a.x-0.75*(a.y-a.x))/max(0.25*(a.y-a.x),0.001),0.0,1.0);
    return t*t*(3.0-2.0*t)*a.w;
}
// Opaque surfaces fade out by leaving that share of their pixels to what is
// behind them (the sky), in interleaved gradient noise (Jimenez 2014) over
// the window pixel `pixel`.
fn faded_out(pixel:vec2<f32>,offset:vec3<f32>,a:vec4<f32>)->bool {
    let edge=fog_edge(offset,a);
    return edge>0.0 && edge>=fract(52.9829189*fract(dot(floor(pixel),vec2<f32>(0.06711056,0.00583715))));
}
