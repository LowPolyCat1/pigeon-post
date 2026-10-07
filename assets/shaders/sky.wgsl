// The sky dome: a gradient, a sun, and drifting toon clouds. Below the horizon the color is
// the fog color, so far islands and the cloud sea fade into the sky without a seam.

#import bevy_pbr::{forward_io::VertexOutput, mesh_view_bindings::view}
#import pigeon_post::noise::fbm

struct Sky {
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    // xyz: unit vector toward the sun.
    sun: vec4<f32>,
    time: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> sky: Sky;

const CLOUD_LIGHT: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
const CLOUD_SHADE: vec3<f32> = vec3<f32>(0.78, 0.80, 0.95);

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let direction = normalize(in.world_position.xyz - view.world_position);
    let up = clamp(direction.y, 0.0, 1.0);
    var color = mix(sky.horizon.rgb, sky.zenith.rgb, pow(up, 0.5));

    let toward_sun = max(dot(direction, sky.sun.xyz), 0.0);
    color += vec3<f32>(1.0, 0.95, 0.8) * pow(toward_sun, 48.0) * 0.35;
    color = mix(color, vec3<f32>(1.0, 0.98, 0.9), smoothstep(0.9965, 0.998, toward_sun));

    // The clouds lie on a flat layer above the camera, so they shrink toward the horizon.
    if direction.y > 0.0 {
        let layer = direction.xz / (direction.y + 0.12) * 1.4 + vec2<f32>(sky.time * 0.015, 0.0);
        let density = fbm(layer);
        // Hard steps give the flat, painted look instead of soft gradients.
        let cover = smoothstep(0.56, 0.6, density) * smoothstep(0.0, 0.2, direction.y);
        let cloud = mix(CLOUD_SHADE, CLOUD_LIGHT, step(0.66, density));
        color = mix(color, cloud, cover);
    }
    return vec4<f32>(color, 1.0);
}
