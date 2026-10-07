// The cloud sea surface. The displacement must match `displacement` in src/waves.rs: the host
// computes buoyancy on the CPU, and this shader only draws the same surface.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#import pigeon_post::noise::fbm

struct Waves {
    // Per wave: direction x, direction z, wave number, angular speed.
    motion: array<vec4<f32>, 4>,
    // Per wave: amplitude, horizontal amplitude.
    size: array<vec4<f32>, 4>,
    time: f32,
    sea_level: f32,
}

struct Style {
    horizon: vec4<f32>,
    // xyz: unit vector toward the sun.
    sun: vec4<f32>,
    // x: fog start, y: fog end, in meters from the camera.
    fog: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> waves: Waves;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> style: Style;

const CLOUD_LIGHT: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
const CLOUD_MID: vec3<f32> = vec3<f32>(0.84, 0.87, 0.97);
const CLOUD_SHADE: vec3<f32> = vec3<f32>(0.64, 0.68, 0.90);
// Highest crest above the sea level, used to brighten the crests.
const CREST_HEIGHT: f32 = 0.7;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let rest = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    ).xz;

    var offset = vec3<f32>(0.0);
    // The normal from the partial derivatives of the Gerstner sum.
    var normal = vec3<f32>(0.0, 1.0, 0.0);
    for (var i = 0u; i < 4u; i++) {
        let motion = waves.motion[i];
        let size = waves.size[i];
        let direction = motion.xy;
        let phase = motion.z * dot(direction, rest) - motion.w * waves.time;
        let c = cos(phase);
        let s = sin(phase);
        offset += vec3<f32>(direction.x * size.y * c, size.x * s, direction.y * size.y * c);
        normal -= vec3<f32>(
            direction.x * motion.z * size.x * c,
            motion.z * size.y * s,
            direction.y * motion.z * size.x * c,
        );
    }

    let world = vec3<f32>(rest.x, waves.sea_level, rest.y) + offset;
    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
    out.world_normal = normalize(normal);
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = in.world_position.xyz;
    // Puffs drift slowly over the waves. They only change the shade, not the surface height,
    // so the picture still matches the gameplay.
    let puff = fbm(world.xz * 0.09 + vec2<f32>(waves.time * 0.02, waves.time * 0.01));
    let light = dot(normalize(in.world_normal), style.sun.xyz) + (puff - 0.5) * 0.8;

    // Three flat bands instead of a smooth gradient: the toon look of the sky clouds.
    var color = CLOUD_SHADE;
    color = mix(color, CLOUD_MID, step(0.62, light));
    color = mix(color, CLOUD_LIGHT, step(0.88, light));

    let height = world.y - waves.sea_level;
    color = mix(color, CLOUD_LIGHT, smoothstep(0.45, 0.65, height / CREST_HEIGHT) * 0.6);

    // A thin crease where two puffs meet.
    color *= 1.0 - (1.0 - smoothstep(0.0, 0.025, abs(puff - 0.55))) * 0.08;

    // The custom shader skips the fog of Bevy, so it fades to the horizon color itself.
    let distance = length(world - view.world_position);
    let fog = clamp((distance - style.fog.x) / (style.fog.y - style.fog.x), 0.0, 1.0);
    color = mix(color, style.horizon.rgb, fog);
    return vec4<f32>(color, 1.0);
}
