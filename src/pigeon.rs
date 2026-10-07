//! The pigeon: a dynamic rigid body that walks, jumps, flaps and glides.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::stamina::Stamina;

pub const RADIUS: f32 = 0.3;
/// Length of the straight part of the capsule, without the two caps.
pub const CAPSULE_LENGTH: f32 = 0.4;
pub const WALK_SPEED: f32 = 5.0;
/// Horizontal acceleration toward the walk speed on the ground, in m/s².
const GROUND_ACCELERATION: f32 = 40.0;
/// Weaker than on the ground, so a jump or a shove keeps its momentum.
const AIR_ACCELERATION: f32 = 10.0;
pub const JUMP_SPEED: f32 = 5.5;
/// Vertical speed a flap sets. Lower than a jump, so flying is clumsier than walking.
pub const FLAP_SPEED: f32 = 4.5;
pub const FLAP_COST: f32 = 0.2;
/// Fastest fall while gliding, in m/s.
pub const GLIDE_FALL_SPEED: f32 = 1.5;
/// Stamina per second while gliding. A full bar glides for about 6 seconds.
const GLIDE_DRAIN: f32 = 0.15;
/// Stamina per second on the ground. An empty bar is full again after 2 seconds.
const GROUND_REFILL: f32 = 0.5;
/// Gap below the capsule that still counts as standing on the ground.
const GROUND_PROBE_DISTANCE: f32 = 0.1;
/// The probe sphere is a bit thinner than the capsule, so a wall does not count as ground.
const PROBE_RADIUS: f32 = RADIUS * 0.9;

/// Simulation of every pigeon. Keyboard reading is separate, see [`read_keyboard`].
pub struct PigeonPlugin;

impl Plugin for PigeonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (update_grounded, walk, jump_or_flap, glide, refill_on_ground).chain(),
        );
    }
}

#[derive(Component, Clone, Copy)]
#[require(PigeonInput, Grounded, GroundVelocity, Stamina)]
pub struct Pigeon;

/// The input for the next fixed step. The simulation reads this, never the keyboard,
/// so a test can drive a pigeon.
#[derive(Component, Default, Debug, Clone, Copy, PartialEq)]
pub struct PigeonInput {
    /// `x` is right, `y` is forward. The length is at most 1.
    pub movement: Vec2,
    /// Set on the frame of the press. A press can fall between two fixed steps,
    /// so it stays set until the next fixed step consumes it.
    pub jump: bool,
    /// Held down. Slows a fall while the pigeon is in the air.
    pub glide: bool,
}

impl PigeonInput {
    /// Takes the newest movement and glide. A jump stays set until a fixed step consumes it,
    /// because several frames can arrive between two fixed steps.
    pub fn merge(&mut self, frame: PigeonInput) {
        self.movement = frame.movement;
        self.glide = frame.glide;
        self.jump |= frame.jump;
    }
}

/// One frame of local input. The authority writes it into the pigeon of the sender,
/// so a host, a client and a single player use one input path.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct InputMessage(pub PigeonInput);

#[derive(Component, Default, Debug)]
pub struct Grounded(pub bool);

/// The velocity of the body under the pigeon, at the feet of the pigeon. A pigeon walks
/// relative to it, so it stays on a moving deck. In the air the last value stays, so a jump
/// from a moving deck lands on the deck again.
#[derive(Component, Default, Debug, Clone, Copy, PartialEq)]
pub struct GroundVelocity(pub Vec3);

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

pub fn read_keyboard(keys: Res<ButtonInput<KeyCode>>, mut messages: MessageWriter<InputMessage>) {
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
    messages.write(InputMessage(PigeonInput {
        movement: movement.normalize_or_zero(),
        jump: keys.just_pressed(KeyCode::Space),
        glide: keys.pressed(KeyCode::Space),
    }));
}

/// The motion of a body that a pigeon can stand on. Another pigeon is not ground that moves.
type CarrierQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Position,
        &'static Rotation,
        &'static LinearVelocity,
        &'static AngularVelocity,
        &'static ComputedCenterOfMass,
    ),
    Without<Pigeon>,
>;

fn update_grounded(
    mut pigeons: Query<(&ShapeHits, &Position, &mut Grounded, &mut GroundVelocity)>,
    colliders: Query<&ColliderOf>,
    carriers: CarrierQuery,
) {
    for (hits, position, mut grounded, mut ground_velocity) in &mut pigeons {
        let Some(hit) = hits.iter().next() else {
            grounded.0 = false;
            continue;
        };
        grounded.0 = true;
        let body = colliders
            .get(hit.entity)
            .map_or(hit.entity, |collider| collider.body);
        ground_velocity.0 = carriers.get(body).map_or(
            Vec3::ZERO,
            |(body_position, rotation, linear, angular, center)| {
                let center = body_position.0 + rotation.0 * center.0;
                point_velocity(linear.0, angular.0, center, position.0)
            },
        );
    }
}

/// The velocity of `point` on a rigid body that moves with `linear` and turns with `angular`
/// around `center`.
fn point_velocity(linear: Vec3, angular: Vec3, center: Vec3, point: Vec3) -> Vec3 {
    linear + angular.cross(point - center)
}

fn walk(
    time: Res<Time>,
    mut pigeons: Query<(
        &PigeonInput,
        &Grounded,
        &GroundVelocity,
        &mut LinearVelocity,
    )>,
) {
    for (input, grounded, ground_velocity, mut velocity) in &mut pigeons {
        let acceleration = if grounded.0 {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        };
        // Forward is -Z, the direction the camera looks.
        let target =
            Vec2::new(input.movement.x, -input.movement.y) * WALK_SPEED + ground_velocity.0.xz();
        let horizontal = steer(velocity.xz(), target, acceleration * time.delta_secs());
        velocity.x = horizontal.x;
        velocity.z = horizontal.y;
    }
}

/// One button: a jump on the ground, a flap in the air.
fn jump_or_flap(
    mut pigeons: Query<(
        &mut PigeonInput,
        &Grounded,
        &mut Stamina,
        &mut LinearVelocity,
    )>,
) {
    for (mut input, grounded, mut stamina, mut velocity) in &mut pigeons {
        if !std::mem::take(&mut input.jump) {
            continue;
        }
        if grounded.0 {
            velocity.y = JUMP_SPEED;
        } else if stamina.try_spend(FLAP_COST) {
            velocity.y = FLAP_SPEED;
        }
    }
}

fn glide(
    time: Res<Time>,
    mut pigeons: Query<(&PigeonInput, &Grounded, &mut Stamina, &mut LinearVelocity)>,
) {
    for (input, grounded, mut stamina, mut velocity) in &mut pigeons {
        if !input.glide || grounded.0 || stamina.is_empty() || velocity.y >= -GLIDE_FALL_SPEED {
            continue;
        }
        velocity.y = -GLIDE_FALL_SPEED;
        stamina.drain(GLIDE_DRAIN * time.delta_secs());
    }
}

fn refill_on_ground(time: Res<Time>, mut pigeons: Query<(&Grounded, &mut Stamina)>) {
    for (grounded, mut stamina) in &mut pigeons {
        if grounded.0 {
            stamina.refill(GROUND_REFILL * time.delta_secs());
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
    fn point_velocity_of_moving_body() {
        let velocity = point_velocity(Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO, Vec3::ZERO, Vec3::ONE);
        assert_eq!(velocity, Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn point_velocity_of_turning_body() {
        // A turn around the vertical axis moves a point on +X toward -Z.
        let velocity = point_velocity(Vec3::ZERO, Vec3::Y, Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
        assert!(
            (velocity - Vec3::new(0.0, 0.0, -3.0)).length() < 1e-6,
            "{velocity}"
        );
    }

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
    fn merge_takes_newest_movement_and_glide() {
        let mut input = PigeonInput {
            movement: Vec2::X,
            jump: false,
            glide: true,
        };
        input.merge(PigeonInput {
            movement: Vec2::Y,
            jump: false,
            glide: false,
        });
        assert_eq!(input.movement, Vec2::Y);
        assert!(!input.glide);
    }

    #[test]
    fn merge_keeps_jump_until_consumed() {
        let mut input = PigeonInput::default();
        input.merge(PigeonInput {
            jump: true,
            ..default()
        });
        input.merge(PigeonInput::default());
        assert!(input.jump);
    }

    #[test]
    fn steer_brakes_toward_zero() {
        let result = steer(Vec2::new(0.0, 3.0), Vec2::ZERO, 1.0);
        assert_eq!(result, Vec2::new(0.0, 2.0));
    }
}
