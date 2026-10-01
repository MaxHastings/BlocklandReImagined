// The one fog every pass shares; `bri_content::environment::Fog` is its CPU
// twin and says how it works. atmosphere: fog start, fog end, unused, 1 when
// fog is on.
fn fog_density(a:vec4<f32>)->f32 {return FOG_DEPTH/max(a.y-a.x,0.001);}
fn sky_haze(up:f32,a:vec4<f32>)->f32 {
    if up<=0.0 {return 1.0;}
    let rise=up/FOG_HEIGHT;
    return 1.0-exp(-fog_density(a)*exp(-rise*a.x)/rise);
}
// Fog over the sky along a ray whose direction rises `up`.
fn sky_fog_at(up:f32,a:vec4<f32>)->f32 {
    if a.y<=0.0 {return 0.0;}
    return sky_haze(up,a)*a.w;
}
// Fog over a point at `offset` from the eye.
fn fog_along(offset:vec3<f32>,a:vec4<f32>)->f32 {
    if a.y<=0.0 {return 0.0;}
    let distance=length(offset);
    let up=offset.y/max(distance,0.0001);
    let inside=max(distance-a.x,0.0);
    let rise=max(up,0.0)/FOG_HEIGHT;
    let x=rise*inside;
    let spread=select((1.0-exp(-x))/x,1.0-0.5*x,x<0.0001);
    let depth=fog_density(a)*exp(-rise*a.x)*inside*spread;
    let t=clamp((distance-a.x-0.75*(a.y-a.x))/max(0.25*(a.y-a.x),0.001),0.0,1.0);
    let edge=t*t*(3.0-2.0*t);
    return max(1.0-exp(-depth),edge*sky_haze(up,a))*a.w;
}
