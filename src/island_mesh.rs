//! Procedural floating islands in the style of the Skylands: an irregular grass top with a
//! turf lip, cliff bands of soil and rock, and a jagged rock underside with hanging spikes.
//!
//! The generator uses only a seed and arithmetic, so the host and every client build the
//! same island without a network message.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

/// Depth of the turf lip below the grass edge, and of its underside.
const LIP_DROP: f32 = 0.12;
const LIP_UNDERSIDE: f32 = 0.42;
/// The lip overhangs the outline by this fraction of the radius.
const LIP_OVERHANG: f32 = 1.035;
/// The underside starts at this fraction of the outline radius.
const UNDERSIDE_START: f32 = 0.86;
/// Below 1, the underside stays wide and then narrows fast near the tip, like a carrot.
const UNDERSIDE_TAPER: f32 = 0.7;
/// Vertical extent of one rock color band.
const ROCK_BAND: f32 = 1.1;

/// The parameters of one island. The grass top is at y = 0 in the local space of the island.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IslandShape {
    pub seed: u32,
    /// The largest radius of the outline. The turf lip overhangs it a little.
    pub radius: f32,
    /// The outline radius varies between `radius * (1 - outline_variation)` and `radius`.
    pub outline_variation: f32,
    /// Inside this distance from the center, the grass is flat at y = 0.
    pub flat_radius: f32,
    /// Height amplitude of the grass outside the flat area.
    pub bump: f32,
    /// Distance from the grass top down to the lowest point of the underside.
    pub depth: f32,
    pub spikes: u32,
}

impl IslandShape {
    pub fn new(seed: u32, radius: f32) -> Self {
        Self {
            seed,
            radius,
            outline_variation: 0.16,
            flat_radius: radius * 0.4,
            bump: (radius * 0.06).min(0.45),
            depth: radius * 1.35,
            spikes: 4 + (radius * 0.8) as u32,
        }
    }

    /// The radius of the grass edge in the direction `angle`, in radians around +Y from +X.
    pub fn outline_radius(&self, angle: f32) -> f32 {
        let around = Vec2::new(angle.cos(), angle.sin()) * 1.7 + Vec2::splat(31.0);
        let noise = fbm(self.seed, around, 3);
        self.radius * (1.0 - self.outline_variation * noise)
    }

    /// The height of the grass at local `(x, z)`. Outside the outline, the edge height applies.
    pub fn top_height(&self, x: f32, z: f32) -> f32 {
        let distance = Vec2::new(x, z).length();
        let edge = self.outline_radius(z.atan2(x));
        let start = self.flat_radius.min(edge - 0.01);
        let blend = smoothstep(start, edge, distance);
        let noise = fbm(self.seed.wrapping_add(1), Vec2::new(x, z) * 0.35, 2);
        // The edge sags a little, so the top looks rounded like a hill.
        blend * (self.bump * (2.0 * noise - 1.0) - self.bump * 0.6)
    }

    fn segments(&self) -> usize {
        ((self.radius * 3.5) as usize).clamp(14, 40)
    }

    fn top_rings(&self) -> usize {
        ((self.radius / 1.4) as usize).clamp(2, 7)
    }

    fn cliff_bottom(&self) -> f32 {
        -(self.soil_bottom() + self.depth * 0.22)
    }

    /// A small island gets a thinner soil band, so its rock still shows.
    fn soil_bottom(&self) -> f32 {
        (0.6 + self.radius * 0.09).min(1.5)
    }

    /// The height of the smooth underside at `fraction` of the outline radius, without the
    /// jitter of the vertices.
    fn underside_height(&self, fraction: f32) -> f32 {
        if fraction >= UNDERSIDE_START {
            return self.cliff_bottom();
        }
        let along = 1.0 - (fraction.max(0.0) / UNDERSIDE_START).powf(1.0 / UNDERSIDE_TAPER);
        self.cliff_bottom() + (-self.depth - self.cliff_bottom()) * along
    }

    /// The triangles of the island in local space.
    pub fn geometry(&self) -> IslandGeometry {
        let mut geometry = IslandGeometry::default();
        let segments = self.segments();
        let step = TAU / segments as f32;
        let mut jitter = Jitter::new(self.seed.wrapping_add(7));

        let center = geometry.vertex(Vec3::new(0.0, self.top_height(0.0, 0.0), 0.0));
        let rings = self.top_rings();
        let mut previous: Vec<u32> = Vec::new();
        for ring in 1..=rings {
            let fraction = ring as f32 / rings as f32;
            let current: Vec<u32> = (0..segments)
                .map(|index| {
                    // Inner rings turn a little, so the facets of the grass are irregular.
                    let turn = if ring < rings {
                        jitter.signed() * step * 0.3
                    } else {
                        0.0
                    };
                    let angle = index as f32 * step + turn;
                    let radius = fraction * self.outline_radius(angle);
                    let (x, z) = (radius * angle.cos(), radius * angle.sin());
                    geometry.vertex(Vec3::new(x, self.top_height(x, z), z))
                })
                .collect();
            if ring == 1 {
                geometry.fan_from(center, &current, Layer::Grass);
            } else {
                geometry.strip(&previous, &current, Layer::Grass);
            }
            previous = current;
        }

        let outline: Vec<(f32, f32)> = (0..segments)
            .map(|index| {
                let angle = index as f32 * step;
                (angle, self.outline_radius(angle))
            })
            .collect();
        let edge: Vec<f32> = outline
            .iter()
            .map(|&(angle, radius)| self.top_height(radius * angle.cos(), radius * angle.sin()))
            .collect();

        let side_ring = |geometry: &mut IslandGeometry,
                         jitter: &mut Jitter,
                         factor: f32,
                         height: &dyn Fn(usize) -> f32,
                         spread: Vec2|
         -> Vec<u32> {
            outline
                .iter()
                .enumerate()
                .map(|(index, &(angle, radius))| {
                    let radius = radius * (factor + jitter.signed() * spread.x);
                    let y = height(index) + jitter.signed() * spread.y;
                    geometry.vertex(Vec3::new(radius * angle.cos(), y, radius * angle.sin()))
                })
                .collect()
        };

        let lip = side_ring(
            &mut geometry,
            &mut jitter,
            LIP_OVERHANG,
            &|index| edge[index] - LIP_DROP,
            Vec2::ZERO,
        );
        geometry.strip(&previous, &lip, Layer::Lip);
        let lip_under = side_ring(
            &mut geometry,
            &mut jitter,
            0.96,
            &|index| edge[index] - LIP_UNDERSIDE,
            Vec2::new(0.01, 0.04),
        );
        geometry.strip(&lip, &lip_under, Layer::LipUnderside);
        let soil = side_ring(
            &mut geometry,
            &mut jitter,
            0.975,
            &|_| -0.6 * self.soil_bottom(),
            Vec2::new(0.015, 0.1),
        );
        geometry.strip(&lip_under, &soil, Layer::Soil);
        let soil_bottom = side_ring(
            &mut geometry,
            &mut jitter,
            0.955,
            &|_| -self.soil_bottom(),
            Vec2::new(0.015, 0.12),
        );
        geometry.strip(&soil, &soil_bottom, Layer::Soil);

        let mut previous = soil_bottom;
        let cliff_rings = 3;
        for ring in 1..=cliff_rings {
            let along = ring as f32 / cliff_rings as f32;
            let factor = 0.955 + (UNDERSIDE_START - 0.955) * along;
            let y = -self.soil_bottom() + (self.cliff_bottom() + self.soil_bottom()) * along;
            let current = side_ring(
                &mut geometry,
                &mut jitter,
                factor,
                &|_| y,
                Vec2::new(0.035, 0.15),
            );
            geometry.strip(&previous, &current, Layer::Rock);
            previous = current;
        }

        let underside_rings = ((self.depth / 1.6) as usize).clamp(3, 8);
        let span = -self.depth - self.cliff_bottom();
        for ring in 1..=underside_rings {
            let along = ring as f32 / (underside_rings + 1) as f32;
            let factor = UNDERSIDE_START * (1.0 - along).powf(UNDERSIDE_TAPER);
            let y = self.cliff_bottom() + span * along;
            let current = side_ring(
                &mut geometry,
                &mut jitter,
                factor,
                &|_| y,
                Vec2::new(0.09 * factor, 0.2 * span.abs() / underside_rings as f32),
            );
            geometry.strip(&previous, &current, Layer::Rock);
            previous = current;
        }
        let tip = geometry.vertex(Vec3::new(
            jitter.signed() * self.radius * 0.08,
            -self.depth,
            jitter.signed() * self.radius * 0.08,
        ));
        geometry.fan_to(&previous, tip, Layer::Rock);

        for spike in 0..self.spikes {
            self.spike(&mut geometry, &mut jitter, spike);
        }
        geometry
    }

    /// A rock that hangs from the underside: an open, kinked pyramid. Its base lies above
    /// the underside, so the base hides inside the island.
    fn spike(&self, geometry: &mut IslandGeometry, jitter: &mut Jitter, number: u32) {
        let angle = (number as f32 + 0.5 + jitter.signed() * 0.35) * TAU / self.spikes as f32;
        let outline = self.outline_radius(angle);
        let fraction = 0.3 + 0.45 * jitter.unit();
        let width = self.radius * (0.1 + 0.08 * jitter.unit());
        let foot = Vec2::new(angle.cos(), angle.sin()) * fraction * outline;
        let base_y = self.underside_height(fraction + width / outline) + 0.4;
        let surface = self.underside_height(fraction);
        // Spikes near the rim are shorter, so the outline of the underside stays a cone.
        let length = self.depth * (0.15 + 0.2 * jitter.unit()) * (1.1 - fraction);
        // No spike hangs much below the tip of the island.
        let length = length.min(surface + self.depth * 1.05);

        let sides = 5;
        let twist = jitter.unit() * TAU;
        let ring = |geometry: &mut IslandGeometry, center: Vec3, radius: f32| -> Vec<u32> {
            (0..sides)
                .map(|side| {
                    let around = twist + side as f32 * TAU / sides as f32;
                    geometry.vertex(center + Vec3::new(around.cos(), 0.0, around.sin()) * radius)
                })
                .collect()
        };
        let base = ring(geometry, Vec3::new(foot.x, base_y, foot.y), width);
        let kink = Vec2::new(jitter.signed(), jitter.signed()) * width * 0.4;
        let middle = ring(
            geometry,
            Vec3::new(foot.x + kink.x, surface - length * 0.4, foot.y + kink.y),
            width * 0.5,
        );
        geometry.strip(&base, &middle, Layer::Rock);
        let lean = Vec2::new(jitter.signed(), jitter.signed()) * width * 0.6;
        let tip = geometry.vertex(Vec3::new(
            foot.x + lean.x,
            surface - length,
            foot.y + lean.y,
        ));
        geometry.fan_to(&middle, tip, Layer::Rock);
    }

    /// The flat-shaded render mesh with vertex colors.
    pub fn mesh(&self) -> Mesh {
        let geometry = self.geometry();
        let corners = geometry.triangles.len() * 3;
        let mut positions = Vec::with_capacity(corners);
        let mut normals = Vec::with_capacity(corners);
        let mut colors = Vec::with_capacity(corners);
        for (index, (triangle, layer)) in
            geometry.triangles.iter().zip(&geometry.layers).enumerate()
        {
            let [a, b, c] = triangle.map(|vertex| geometry.positions[vertex as usize]);
            let normal = (b - a).cross(c - a).normalize_or(Vec3::Y);
            let color = self.color(*layer, (a + b + c) / 3.0, index as u32);
            for corner in [a, b, c] {
                positions.push(corner.to_array());
                normals.push(normal.to_array());
                colors.push(color);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    }

    /// A static triangle mesh collider of the island.
    pub fn collider(&self) -> Result<Collider, String> {
        let geometry = self.geometry();
        // Without this fix, a walking pigeon bumps on the inner edges of the grass.
        Collider::try_trimesh_with_config(
            geometry.positions,
            geometry.triangles,
            TrimeshFlags::FIX_INTERNAL_EDGES,
        )
        .map_err(|error| {
            format!(
                "the collider of the island with seed {} and radius {} failed: {error:?}",
                self.seed, self.radius
            )
        })
    }

    fn color(&self, layer: Layer, center: Vec3, triangle: u32) -> [f32; 4] {
        let shade = 0.92 + 0.16 * unit(hash(self.seed, triangle as i32, 3));
        let srgb = match layer {
            Layer::Grass | Layer::Lip => {
                let patch = value_noise(
                    self.seed.wrapping_add(5),
                    Vec2::new(center.x, center.z) * 0.4,
                );
                Vec3::new(0.36, 0.70, 0.27).lerp(Vec3::new(0.46, 0.80, 0.31), patch)
            }
            Layer::LipUnderside => Vec3::new(0.28, 0.50, 0.21),
            Layer::Soil => Vec3::new(0.55, 0.38, 0.22),
            Layer::Rock => {
                const BANDS: [Vec3; 4] = [
                    Vec3::new(0.62, 0.50, 0.40),
                    Vec3::new(0.52, 0.43, 0.37),
                    Vec3::new(0.58, 0.48, 0.42),
                    Vec3::new(0.46, 0.39, 0.36),
                ];
                let wobble = value_noise(
                    self.seed.wrapping_add(9),
                    Vec2::new(center.x, center.z) * 0.3,
                ) - 0.5;
                let band = ((-center.y + wobble) / ROCK_BAND).floor() as i32;
                let deep = (-center.y / self.depth).clamp(0.0, 1.0);
                BANDS[band.rem_euclid(4) as usize].lerp(Vec3::new(0.30, 0.27, 0.29), deep * 0.6)
            }
        };
        let srgb = srgb * shade;
        Color::srgb(srgb.x, srgb.y, srgb.z)
            .to_linear()
            .to_f32_array()
    }
}

/// The part of the island a triangle belongs to. It selects the color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Grass,
    Lip,
    LipUnderside,
    Soil,
    Rock,
}

/// An indexed triangle list with one layer per triangle. The front faces point outward.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct IslandGeometry {
    pub positions: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    pub layers: Vec<Layer>,
}

impl IslandGeometry {
    fn vertex(&mut self, position: Vec3) -> u32 {
        self.positions.push(position);
        (self.positions.len() - 1) as u32
    }

    fn triangle(&mut self, corners: [u32; 3], layer: Layer) {
        self.triangles.push(corners);
        self.layers.push(layer);
    }

    /// Connects two closed rings with the same vertex count. `inner` is the ring nearer to
    /// the top center along the surface.
    fn strip(&mut self, inner: &[u32], outer: &[u32], layer: Layer) {
        let count = inner.len();
        for index in 0..count {
            let next = (index + 1) % count;
            self.triangle([inner[index], outer[next], outer[index]], layer);
            self.triangle([inner[index], inner[next], outer[next]], layer);
        }
    }

    fn fan_from(&mut self, center: u32, ring: &[u32], layer: Layer) {
        for index in 0..ring.len() {
            let next = (index + 1) % ring.len();
            self.triangle([center, ring[next], ring[index]], layer);
        }
    }

    fn fan_to(&mut self, ring: &[u32], tip: u32, layer: Layer) {
        for index in 0..ring.len() {
            let next = (index + 1) % ring.len();
            self.triangle([ring[index], ring[next], tip], layer);
        }
    }
}

/// A sequence of deterministic random numbers from a seed.
struct Jitter {
    seed: u32,
    count: i32,
}

impl Jitter {
    fn new(seed: u32) -> Self {
        Self { seed, count: 0 }
    }

    fn unit(&mut self) -> f32 {
        self.count += 1;
        unit(hash(self.seed, self.count, 0))
    }

    fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// An integer hash with good avalanche, so neighboring cells get unrelated values.
pub fn hash(seed: u32, x: i32, y: i32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B1)
        ^ (x as u32).wrapping_mul(0x85EB_CA77)
        ^ (y as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

/// Maps a hash to [0, 1).
fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth value noise in [0, 1) with one random value per integer grid point.
pub fn value_noise(seed: u32, point: Vec2) -> f32 {
    let cell = point.floor();
    let (x, y) = (cell.x as i32, cell.y as i32);
    let local = point - cell;
    let fade = local * local * (Vec2::splat(3.0) - 2.0 * local);
    let corner = |dx: i32, dy: i32| unit(hash(seed, x + dx, y + dy));
    let bottom = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * fade.x;
    let top = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * fade.x;
    bottom + (top - bottom) * fade.y
}

/// Value noise in [0, 1) with `octaves` layers of rising detail.
pub fn fbm(seed: u32, point: Vec2, octaves: u32) -> f32 {
    let (mut sum, mut weight, mut total) = (0.0, 1.0, 0.0);
    for octave in 0..octaves {
        let scale = (1u32 << octave) as f32;
        sum += value_noise(seed.wrapping_add(octave * 101), point * scale) * weight;
        total += weight;
        weight *= 0.5;
    }
    sum / total
}

fn smoothstep(start: f32, end: f32, value: f32) -> f32 {
    let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes() -> Vec<IslandShape> {
        let mut main = IslandShape::new(1, 10.0);
        main.flat_radius = 7.5;
        vec![
            main,
            IslandShape::new(2, 4.5),
            IslandShape::new(3, 2.5),
            IslandShape::new(4, 9.0),
        ]
    }

    #[test]
    fn noise_is_deterministic_and_in_range() {
        for step in 0..200 {
            let point = Vec2::new(step as f32 * 0.37 - 30.0, step as f32 * -0.21 + 5.0);
            let value = fbm(5, point, 3);
            assert_eq!(value, fbm(5, point, 3));
            assert!((0.0..1.0).contains(&value), "{value} at {point}");
        }
        let differs = (0..50).any(|step| {
            let point = Vec2::splat(step as f32 * 0.7);
            value_noise(1, point) != value_noise(2, point)
        });
        assert!(differs);
    }

    #[test]
    fn outline_stays_in_bounds() {
        for shape in shapes() {
            for step in 0..720 {
                let radius = shape.outline_radius(step as f32 * TAU / 720.0);
                let low = shape.radius * (1.0 - shape.outline_variation);
                assert!(radius >= low && radius <= shape.radius, "{radius}");
            }
        }
    }

    #[test]
    fn top_is_flat_near_center() {
        let shape = shapes()[0];
        for step in 0..100 {
            let angle = step as f32 * 0.31;
            let distance = (step % 10) as f32 * 0.75;
            let height = shape.top_height(distance * angle.cos(), distance * angle.sin());
            assert!(height.abs() < 1e-6, "{height} at {distance}");
        }
    }

    #[test]
    fn island_stays_inside_the_lip() {
        for shape in shapes() {
            for position in shape.geometry().positions {
                let reach = Vec2::new(position.x, position.z).length();
                assert!(reach <= shape.radius * LIP_OVERHANG + 1e-4, "{position}");
                assert!(
                    position.y >= -shape.depth * 1.1 && position.y <= shape.bump,
                    "{position} in {shape:?}"
                );
            }
        }
    }

    #[test]
    fn grass_faces_up() {
        for shape in shapes() {
            let geometry = shape.geometry();
            for (triangle, layer) in geometry.triangles.iter().zip(&geometry.layers) {
                let [a, b, c] = triangle.map(|vertex| geometry.positions[vertex as usize]);
                if *layer == Layer::Grass {
                    assert!((b - a).cross(c - a).y > 0.0, "{a} {b} {c}");
                }
            }
        }
    }

    #[test]
    fn mesh_has_finite_normals_and_colors() {
        for shape in shapes() {
            let mesh = shape.mesh();
            let count = mesh.count_vertices();
            assert!(count > 0 && count % 3 == 0);
            for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
                let values = mesh
                    .attribute(attribute)
                    .and_then(|values| values.as_float3())
                    .expect("float3 attribute");
                assert_eq!(values.len(), count);
                assert!(values.iter().flatten().all(|value| value.is_finite()));
            }
            assert_eq!(
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                    .map(|colors| colors.len()),
                Some(count)
            );
        }
    }

    #[test]
    fn geometry_is_deterministic() {
        let shape = IslandShape::new(42, 6.0);
        assert_eq!(shape.geometry(), shape.geometry());
        assert_ne!(
            shape.geometry().positions,
            IslandShape::new(43, 6.0).geometry().positions
        );
    }

    #[test]
    fn collider_builds() {
        for shape in shapes() {
            assert!(shape.collider().is_ok(), "{shape:?}");
        }
    }
}
