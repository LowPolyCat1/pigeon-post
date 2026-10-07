// The cloud sea surface. The displacement must match `displacement` in src/waves.rs: the host
// computes buoyancy on the CPU, and this shader only draws the same surface.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

struct Waves {
    // Per wave: direction x, direction z, wave number, angular speed.
    motion: array<vec4<f32>, 4>,
    // Per wave: amplitude, horizontal amplitude.
    size: array<vec4<f32>, 4>,
    time: f32,
    sea_level: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> waves: Waves;

// The direction toward the sun of the test level. A fixed value is enough for a stylised sea.
const LIGHT: vec3<f32> = vec3<f32>(0.37, 0.80, 0.48);
// Highest crest above the sea level, used to fade the crests to white.
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
    let light = 0.65 + 0.35 * max(dot(normalize(in.world_normal), LIGHT), 0.0);
    let height = in.world_position.y - waves.sea_level;
    let crest = clamp(height / CREST_HEIGHT * 0.5 + 0.5, 0.0, 1.0);
    let color = mix(vec3<f32>(0.70, 0.74, 0.88), vec3<f32>(1.0), crest) * light;
    return vec4<f32>(color, 0.92);
}
