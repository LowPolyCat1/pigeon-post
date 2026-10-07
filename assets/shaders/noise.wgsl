// Value noise for the stylised sky and cloud sea. Cheap and smooth enough for puffy shapes.
#define_import_path pigeon_post::noise

fn hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let blend = f * f * (3.0 - 2.0 * f);
    let a = hash(cell);
    let b = hash(cell + vec2<f32>(1.0, 0.0));
    let c = hash(cell + vec2<f32>(0.0, 1.0));
    let d = hash(cell + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, blend.x), mix(c, d, blend.x), blend.y);
}

// Three octaves. The result stays between 0 and 1.
fn fbm(p: vec2<f32>) -> f32 {
    return value_noise(p) * 0.57 + value_noise(p * 2.03 + 17.1) * 0.29
        + value_noise(p * 4.01 + 31.7) * 0.14;
}
