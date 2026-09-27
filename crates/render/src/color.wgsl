// Baked modulation was authored against display-encoded diffuse colors. Keep
// that response, then return linear output to the sRGB render attachment.
fn display_color(linear:vec3<f32>)->vec3<f32> {
    return select(1.055*pow(max(linear,vec3<f32>(0.0)),vec3<f32>(1.0/2.4))-0.055,
                  linear*12.92,linear<=vec3<f32>(0.0031308));
}
fn linear_color(display:vec3<f32>)->vec3<f32> {
    return select(pow(max((display+0.055)/1.055,vec3<f32>(0.0)),vec3<f32>(2.4)),
                  display/12.92,display<=vec3<f32>(0.04045));
}
