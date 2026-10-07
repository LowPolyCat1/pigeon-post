//! The skyship: a hull that sails on the cloud sea. The host simulates the hull, and each
//! client sees the replicated motion.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::cloud_sea::{SEA_LEVEL, cloud_sea_depth};
use crate::net::NetMode;

/// Width, height and length of the hull. The deck is the top face.
pub const HULL_SIZE: Vec3 = Vec3::new(4.0, 2.0, 10.0);
/// Low rails on the deck edge. A pigeon can hop over them, a sliding crate stops at them.
pub const RAIL_HEIGHT: f32 = 0.5;
const RAIL_THICKNESS: f32 = 0.15;
/// 80 m³ of hull at this density gives a ship of 800 kg.
const HULL_DENSITY: f32 = 10.0;
/// The part of the hull height below the surface in calm water.
const REST_DRAFT: f32 = 0.4;
/// Probe positions on the hull bottom, as fractions of the hull size. The spread lets a
/// crest under the bow lift the bow first, so the waves pitch and roll the ship.
const PROBES_X: [f32; 2] = [-0.4, 0.4];
const PROBES_Z: [f32; 4] = [-0.4, -0.15, 0.15, 0.4];
const PROBE_COUNT: f32 = (PROBES_X.len() * PROBES_Z.len()) as f32;
/// Drag of the cloud sea on a submerged probe, per second. Below the critical value of the
/// heave, so the ship still bobs a few times before it settles.
const CLOUD_DRAG: f32 = 1.5;
const GRAVITY: f32 = 9.81;
/// Next to the test island, with a gap that a pigeon can jump.
pub const SHIP_SPAWN: Vec3 = Vec3::new(13.0, SEA_LEVEL, 0.0);

pub struct ShipPlugin;

impl Plugin for ShipPlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<Ship, ()>()
            .add_systems(
                Startup,
                spawn_ship.run_if(|mode: Res<NetMode>| mode.is_authority()),
            )
            .add_systems(FixedUpdate, float_ship);
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Ship;

// Replicon needs a serde type, and a marker carries no data.
impl From<Ship> for () {
    fn from(_: Ship) -> Self {}
}

impl From<()> for Ship {
    fn from(_: ()) -> Self {
        Ship
    }
}

/// The hull and the rails as one compound collider, so the ship is one rigid body.
pub fn ship_collider() -> Collider {
    let half = HULL_SIZE / 2.0;
    let rail_y = half.y + RAIL_HEIGHT / 2.0;
    let side = Collider::cuboid(RAIL_THICKNESS, RAIL_HEIGHT, HULL_SIZE.z);
    let end = Collider::cuboid(HULL_SIZE.x, RAIL_HEIGHT, RAIL_THICKNESS);
    Collider::compound(vec![
        (
            Vec3::ZERO,
            Quat::IDENTITY,
            Collider::cuboid(HULL_SIZE.x, HULL_SIZE.y, HULL_SIZE.z),
        ),
        (
            Vec3::new(-half.x, rail_y, 0.0),
            Quat::IDENTITY,
            side.clone(),
        ),
        (Vec3::new(half.x, rail_y, 0.0), Quat::IDENTITY, side),
        (Vec3::new(0.0, rail_y, -half.z), Quat::IDENTITY, end.clone()),
        (Vec3::new(0.0, rail_y, half.z), Quat::IDENTITY, end),
    ])
}

/// The physics components of the ship, without a mesh, so tests can spawn one headless.
/// A client receives only [`Ship`] and the transform, so it does not simulate the ship.
pub fn ship_body() -> impl Bundle {
    (
        Ship,
        Replicated,
        RigidBody::Dynamic,
        ship_collider(),
        ColliderDensity(HULL_DENSITY),
        TransformInterpolation,
    )
}

fn spawn_ship(mut commands: Commands) {
    // Start at the rest draft, so the ship does not drop onto the sea.
    let rest = SHIP_SPAWN + Vec3::Y * (HULL_SIZE.y / 2.0 - REST_DRAFT * HULL_SIZE.y);
    commands.spawn((ship_body(), Transform::from_translation(rest)));
}

/// The probe points on the hull bottom, in the frame of the ship.
fn probe_points() -> impl Iterator<Item = Vec3> {
    PROBES_X.into_iter().flat_map(|x| {
        PROBES_Z
            .into_iter()
            .map(move |z| Vec3::new(x, -0.5, z) * HULL_SIZE)
    })
}

/// The upward force on one probe. The probes together carry the whole weight at the rest
/// draft, and the force stops growing when the probe is a full hull height deep.
fn probe_lift(depth: f32, mass: f32) -> f32 {
    let stiffness = mass * GRAVITY / (PROBE_COUNT * REST_DRAFT * HULL_SIZE.y);
    stiffness * depth.clamp(0.0, HULL_SIZE.y)
}

fn float_ship(time: Res<Time>, mut ships: Query<(Forces, &ComputedMass), With<Ship>>) {
    let now = time.elapsed_secs();
    for (mut forces, mass) in &mut ships {
        let mass = mass.value();
        let position = forces.position().0;
        let rotation = forces.rotation().0;
        for probe in probe_points() {
            let point = position + rotation * probe;
            let depth = cloud_sea_depth(point, now);
            if depth <= 0.0 {
                continue;
            }
            let submerged = (depth / HULL_SIZE.y).min(1.0);
            let drag = -forces.velocity_at_point(point) * CLOUD_DRAG * mass / PROBE_COUNT;
            forces
                .apply_force_at_point(Vec3::Y * probe_lift(depth, mass) + drag * submerged, point);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_carry_weight_at_rest_draft() {
        let mass = 800.0;
        let total = probe_lift(REST_DRAFT * HULL_SIZE.y, mass) * PROBE_COUNT;
        assert!((total - mass * GRAVITY).abs() < 1e-2, "lift {total}");
    }

    #[test]
    fn probe_above_surface_has_no_lift() {
        assert_eq!(probe_lift(-0.3, 800.0), 0.0);
    }

    #[test]
    fn probe_lift_stops_at_hull_height() {
        assert_eq!(
            probe_lift(HULL_SIZE.y * 3.0, 800.0),
            probe_lift(HULL_SIZE.y, 800.0)
        );
    }

    #[test]
    fn probes_lie_on_hull_bottom() {
        let points: Vec<Vec3> = probe_points().collect();
        assert_eq!(points.len(), PROBE_COUNT as usize);
        for point in points {
            assert_eq!(point.y, -HULL_SIZE.y / 2.0);
            assert!(point.x.abs() < HULL_SIZE.x / 2.0 && point.z.abs() < HULL_SIZE.z / 2.0);
        }
    }
}
