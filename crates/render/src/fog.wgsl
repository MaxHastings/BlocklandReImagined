// The one fog every pass shares (`bri_content::environment::Fog`).
// atmosphere: fog start, fog end (complete), unused, 1 when fog is on.
fn fog_at(distance:f32,atmosphere:vec4<f32>)->f32 {
    let t=clamp((distance-atmosphere.x)/max(atmosphere.y-atmosphere.x,0.001),0.0,1.0);
    return (1.0-(1.0-t)*(1.0-t))*atmosphere.w;
}
// The sky along a ray rising `up` (its direction's y): fogged as far as the
// ray travels before it climbs SKY_FOG_CEILING above the eye, so at and below
// the horizon the sky is the fog colour, exactly where far geometry is.
fn sky_fog_at(up:f32,atmosphere:vec4<f32>)->f32 {
    return fog_at(SKY_FOG_CEILING/max(up,SKY_FOG_CEILING/1000000.0),atmosphere);
}
