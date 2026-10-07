//! The cloud sea below the islands. A pigeon floats in it and refills its stamina.
//!
//! The surface moves with the Gerstner waves of [`crate::waves`]. Only the authority runs these
//! systems, so the gameplay uses one surface on every instance.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pigeon::Pigeon;
use crate::stamina::Stamina;
use crate::waves::surface_height;

/// Low enough to fall into from an island, close enough to flap back up with a full bar.
pub const SEA_LEVEL: f32 = -2.5;
/// Upward acceleration per meter of depth, in m/s² per m. Gravity balances it at about 0.25 m.
const BUOYANCY_STIFFNESS: f32 = 40.0;
/// Fraction of the velocity the cloud sea removes per second. Without it, a pigeon bobs forever.
const CLOUD_DRAG: f32 = 3.0;
/// Stamina per second while swimming.
const SWIM_REFILL: f32 = 0.5;

pub struct CloudSeaPlugin;

impl Plugin for CloudSeaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (float, refill_while_swimming));
    }
}

/// Positive means submerged. The value is the depth below the surface at `time`, in meters.
pub fn cloud_sea_depth(position: Vec3, time: f32) -> f32 {
    SEA_LEVEL + surface_height(position.xz(), time) - position.y
}

/// Upward acceleration at `depth`. Zero above the surface.
fn buoyancy(depth: f32) -> f32 {
    BUOYANCY_STIFFNESS * depth.max(0.0)
}

fn float(time: Res<Time>, mut pigeons: Query<(&Position, &mut LinearVelocity), With<Pigeon>>) {
    let delta = time.delta_secs();
    for (position, mut velocity) in &mut pigeons {
        let depth = cloud_sea_depth(position.0, time.elapsed_secs());
        if depth <= 0.0 {
            continue;
        }
        velocity.y += buoyancy(depth) * delta;
        velocity.0 *= (1.0 - CLOUD_DRAG * delta).max(0.0);
    }
}

fn refill_while_swimming(
    time: Res<Time>,
    mut pigeons: Query<(&Position, &mut Stamina), With<Pigeon>>,
) {
    for (position, mut stamina) in &mut pigeons {
        if cloud_sea_depth(position.0, time.elapsed_secs()) > 0.0 {
            stamina.refill(SWIM_REFILL * time.delta_secs());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_follows_the_waves() {
        let point = Vec3::new(3.0, SEA_LEVEL - 1.0, -2.0);
        let surface = surface_height(point.xz(), 1.5);
        assert!((cloud_sea_depth(point, 1.5) - (1.0 + surface)).abs() < 1e-5);
    }

    #[test]
    fn depth_is_negative_above_the_highest_crest() {
        assert!(cloud_sea_depth(Vec3::new(0.0, SEA_LEVEL + 2.0, 0.0), 0.0) < 0.0);
    }

    #[test]
    fn no_buoyancy_above_surface() {
        assert_eq!(buoyancy(-1.0), 0.0);
    }

    #[test]
    fn buoyancy_grows_with_depth() {
        assert!(buoyancy(1.0) > buoyancy(0.5));
    }
}
