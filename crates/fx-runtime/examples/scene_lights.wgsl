// Merge into SceneRenderer's shader; group/binding numbers are host-selected.
// Upload bytemuck::cast_slice(frame.lights.map(GpuLight::from)) to STORAGE|COPY_DST.
// Bind only the initialized range (or pass a separate active count).
struct FxLight { position_radius:vec4<f32>, color:vec4<f32> }
@group(3) @binding(0) var<storage,read> fx_lights:array<FxLight>;

fn native_effect_lighting(world_position:vec3<f32>,normal:vec3<f32>)->vec3<f32> {
    var sum=vec3(0.);
    for(var i=0u;i<arrayLength(&fx_lights);i++) {
        let light=fx_lights[i];
        let delta=light.position_radius.xyz-world_position;
        let distance=length(delta);let radius=light.position_radius.w;
        if radius>0. && distance<radius {
            let direction=delta/max(distance,0.0001);
            let attenuation=max(0.,1.-distance/radius);
            sum+=light.color.rgb*max(dot(normal,direction),0.)*attenuation*attenuation;
        }
    }
    return sum;
}
// In main fragment: shaded_rgb += albedo_rgb*native_effect_lighting(world_position,normal).
// This unshadowed quadratic attenuation is a concrete native integration example;
// exact original light falloff and occlusion are still acceptance work.
