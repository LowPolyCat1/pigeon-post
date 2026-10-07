//! Loose crates on the island. The host simulates them, a pigeon pushes them, and each
//! client sees the replicated motion.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::grab::Grabbable;
use crate::net::NetMode;

pub const CRATE_SIZE: f32 = 0.6;
const CRATE_DENSITY: f32 = 15.0;
/// Left of the row where the pigeons spawn, so a new pigeon does not land on a crate.
const CRATE_POSITIONS: [Vec3; 3] = [
    Vec3::new(-3.0, CRATE_SIZE / 2.0, -2.0),
    Vec3::new(-2.0, CRATE_SIZE / 2.0, -2.0),
    Vec3::new(-2.5, CRATE_SIZE * 1.5, -2.0),
];

pub struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<Crate, ()>().add_systems(
            Startup,
            spawn_crates.run_if(|mode: Res<NetMode>| mode.is_authority()),
        );
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Crate;

// Replicon needs a serde type, and a marker carries no data.
impl From<Crate> for () {
    fn from(_: Crate) -> Self {}
}

impl From<()> for Crate {
    fn from(_: ()) -> Self {
        Crate
    }
}

/// The physics components of a crate, without a mesh, so tests can spawn one headless.
/// A client receives only [`Crate`] and the transform, so it does not simulate the crate.
pub fn crate_body() -> impl Bundle {
    (
        Crate,
        Replicated,
        RigidBody::Dynamic,
        Collider::cuboid(CRATE_SIZE, CRATE_SIZE, CRATE_SIZE),
        // 3.2 kg: one wing barely lifts a crate, two wings carry it.
        ColliderDensity(CRATE_DENSITY),
        Grabbable,
        TransformInterpolation,
    )
}

fn spawn_crates(mut commands: Commands) {
    for position in CRATE_POSITIONS {
        commands.spawn((crate_body(), Transform::from_translation(position)));
    }
}
