//! The pigeon: a dynamic rigid body that walks and jumps.

use avian3d::prelude::*;
use bevy::prelude::*;

pub const RADIUS: f32 = 0.3;
/// Length of the straight part of the capsule, without the two caps.
pub const CAPSULE_LENGTH: f32 = 0.4;
pub const WALK_SPEED: f32 = 5.0;
/// Horizontal acceleration toward the walk speed on the ground, in m/s².
const GROUND_ACCELERATION: f32 = 40.0;
/// Weaker than on the ground, so a jump or a shove keeps its momentum.
const AIR_ACCELERATION: f32 = 10.0;
pub const JUMP_SPEED: f32 = 5.5;
/// Gap below the capsule that still counts as standing on the ground.
const GROUND_PROBE_DISTANCE: f32 = 0.1;
/// The probe sphere is a bit thinner than the capsule, so a wall does not count as ground.
const PROBE_RADIUS: f32 = RADIUS * 0.9;

/// Simulation of every pigeon. Keyboard reading is separate, see [`read_keyboard`].
pub struct PigeonPlugin;

impl Plugin for PigeonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (update_grounded, walk, jump).chain());
    }
}

#[derive(Component)]
#[require(PigeonInput, Grounded)]
pub struct Pigeon;

/// The input for the next fixed step. The simulation reads this, never the keyboard,
/// so a test can drive a pigeon.
#[derive(Component, Default, Debug)]
pub struct PigeonInput {
    /// `x` is right, `y` is forward. The length is at most 1.
    pub movement: Vec2,
    /// Set on the frame of the press. A press can fall between two fixed steps,
    /// so it stays set until the next fixed step consumes it.
    pub jump: bool,
}

#[derive(Component, Default, Debug)]
pub struct Grounded(pub bool);

/// The physics components of a pigeon, without a mesh, so tests can spawn one headless.
pub fn pigeon_body() -> impl Bundle {
    (
        Pigeon,
        RigidBody::Dynamic,
        Collider::capsule(RADIUS, CAPSULE_LENGTH),
        // A capsule that tips over cannot walk.
        LockedAxes::ROTATION_LOCKED,
        // With friction, a pigeon pushed into a wall sticks to it. walk() brakes the pigeon instead.
        Friction::ZERO.with_combine_rule(CoefficientCombine::Min),
        ShapeCaster::new(
            Collider::sphere(PROBE_RADIUS),
            Vec3::ZERO,
            Quat::IDENTITY,
            Dir3::NEG_Y,
        )
        .with_max_distance(CAPSULE_LENGTH / 2.0 + RADIUS - PROBE_RADIUS + GROUND_PROBE_DISTANCE)
        .with_max_hits(1),
        TransformInterpolation,
    )
}

pub fn read_keyboard(keys: Res<ButtonInput<KeyCode>>, mut input: Single<&mut PigeonInput>) {
    let mut movement = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        movement.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        movement.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        movement.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        movement.x -= 1.0;
    }
    input.movement = movement.normalize_or_zero();
    if keys.just_pressed(KeyCode::Space) {
        input.jump = true;
    }
}

fn update_grounded(mut pigeons: Query<(&ShapeHits, &mut Grounded)>) {
    for (hits, mut grounded) in &mut pigeons {
        grounded.0 = !hits.is_empty();
    }
}

fn walk(time: Res<Time>, mut pigeons: Query<(&PigeonInput, &Grounded, &mut LinearVelocity)>) {
    for (input, grounded, mut velocity) in &mut pigeons {
        let acceleration = if grounded.0 {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        };
        // Forward is -Z, the direction the camera looks.
        let target = Vec2::new(input.movement.x, -input.movement.y) * WALK_SPEED;
        let horizontal = steer(velocity.xz(), target, acceleration * time.delta_secs());
        velocity.x = horizontal.x;
        velocity.z = horizontal.y;
    }
}

fn jump(mut pigeons: Query<(&mut PigeonInput, &Grounded, &mut LinearVelocity)>) {
    for (mut input, grounded, mut velocity) in &mut pigeons {
        if std::mem::take(&mut input.jump) && grounded.0 {
            velocity.y = JUMP_SPEED;
        }
    }
}

/// Moves `current` toward `target` by at most `max_change`.
fn steer(current: Vec2, target: Vec2, max_change: f32) -> Vec2 {
    current + (target - current).clamp_length_max(max_change)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steer_stops_at_target() {
        let result = steer(Vec2::ZERO, Vec2::new(1.0, 0.0), 5.0);
        assert_eq!(result, Vec2::new(1.0, 0.0));
    }

    #[test]
    fn steer_limits_change() {
        let result = steer(Vec2::ZERO, Vec2::new(10.0, 0.0), 2.0);
        assert_eq!(result, Vec2::new(2.0, 0.0));
    }

    #[test]
    fn steer_brakes_toward_zero() {
        let result = steer(Vec2::new(0.0, 3.0), Vec2::ZERO, 1.0);
        assert_eq!(result, Vec2::new(0.0, 2.0));
    }
}
