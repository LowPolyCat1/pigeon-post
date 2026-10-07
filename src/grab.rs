//! Grabbing with the wings. Each mouse button holds one wing: the left button the left wing,
//! the right button the right wing. A held object hangs on a force-limited spring, so it
//! swings, bumps into things, and heavy cargo needs both wings or a second pigeon.
//!
//! Only the authority simulates physics, so only the authority runs these systems. A client
//! sends its buttons and its view direction in [`PigeonInput`].

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::first_person::{EYE_HEIGHT, look_rotation};
use crate::pigeon::{Pigeon, PigeonInput};

/// How far a wing reaches from the eye, in meters.
pub const REACH: f32 = 2.5;
/// The hold point of a wing in the frame of the view: in front, a bit low, and to its side,
/// so the held object does not cover the crosshair.
const HOLD_FORWARD: f32 = 1.1;
const HOLD_SIDE: f32 = 0.35;
const HOLD_DROP: f32 = 0.25;
/// The natural frequency of the grip spring, in rad/s. Fast enough to follow the view,
/// slow enough that a held object visibly swings.
const GRIP_FREQUENCY: f32 = 10.0;
/// Below 1, so a held object overshoots a little.
const GRIP_DAMPING_RATIO: f32 = 0.7;
/// The pull of one wing, in newtons. One wing lifts about 4 kg. A heavier object sags and
/// needs the other wing too.
pub const MAX_GRIP_FORCE: f32 = 40.0;
/// If something drags the held point this far from the wing, the wing lets go.
const BREAK_DISTANCE: f32 = 3.0;
/// Removes spin from a held object, so it does not twirl on the spring.
const HELD_SPIN_DAMPING: f32 = 4.0;

pub struct GrabPlugin;

impl Plugin for GrabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (update_grips, pull_grips).chain());
    }
}

/// An object that a wing can hold.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grabbable;

/// The wings of [`Grips`].
pub const LEFT_WING: usize = 0;
pub const RIGHT_WING: usize = 1;

/// What each wing of a pigeon holds, by [`LEFT_WING`] and [`RIGHT_WING`].
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct Grips(pub [Option<Grip>; 2]);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grip {
    pub body: Entity,
    /// The held point, in the frame of the body. The object hangs from this point.
    pub anchor: Vec3,
}

/// Where `wing` holds an object, for an eye at `eye` that looks along `look`. If both wings
/// hold the same object, both pull to the center. On two side points, most of their pull
/// would cancel out sideways.
pub fn hold_point(eye: Vec3, look: Quat, wing: usize, both_wings: bool) -> Vec3 {
    let side = match (both_wings, wing == LEFT_WING) {
        (true, _) => 0.0,
        (false, true) => -HOLD_SIDE,
        (false, false) => HOLD_SIDE,
    };
    eye + look * Vec3::new(side, -HOLD_DROP, -HOLD_FORWARD)
}

/// The pull on a body of `mass` whose held point is `error` away from the wing and moves at
/// `velocity` relative to the pigeon. The spring sets an acceleration, so a light and a heavy
/// object follow alike until the force limit.
pub fn grip_force(error: Vec3, velocity: Vec3, mass: f32) -> Vec3 {
    let acceleration =
        error * GRIP_FREQUENCY.powi(2) - velocity * 2.0 * GRIP_DAMPING_RATIO * GRIP_FREQUENCY;
    (acceleration * mass).clamp_length_max(MAX_GRIP_FORCE)
}

/// The grabbable bodies. A pigeon is never one, so the queries do not overlap.
type GrabbableTransforms<'w, 's> =
    Query<'w, 's, (&'static Position, &'static Rotation), (With<Grabbable>, Without<Pigeon>)>;
type GrabbableForces<'w, 's> =
    Query<'w, 's, (Forces, &'static ComputedMass), (With<Grabbable>, Without<Pigeon>)>;

/// A pressed wing takes the first grabbable object under the crosshair. A released wing lets go.
fn update_grips(
    spatial: SpatialQuery,
    mut pigeons: Query<(Entity, &Position, &PigeonInput, &mut Grips), With<Pigeon>>,
    colliders: Query<&ColliderOf>,
    grabbables: GrabbableTransforms,
) {
    for (pigeon, position, input, mut grips) in &mut pigeons {
        let eye = position.0 + Vec3::Y * EYE_HEIGHT;
        let Ok(direction) = Dir3::new(look_rotation(input.look) * Vec3::NEG_Z) else {
            continue;
        };
        let filter = SpatialQueryFilter::from_excluded_entities([pigeon]);
        for wing in [LEFT_WING, RIGHT_WING] {
            if !input.grab[wing] {
                grips.0[wing] = None;
                continue;
            }
            if grips.0[wing].is_some() {
                continue;
            }
            let Some(hit) = spatial.cast_ray(eye, direction, REACH, true, &filter) else {
                continue;
            };
            let body = colliders
                .get(hit.entity)
                .map_or(hit.entity, |collider| collider.body);
            let Ok((body_position, rotation)) = grabbables.get(body) else {
                continue;
            };
            let point = eye + direction * hit.distance;
            grips.0[wing] = Some(Grip {
                body,
                anchor: rotation.0.inverse() * (point - body_position.0),
            });
        }
    }
}

fn pull_grips(
    mut pigeons: Query<(&Position, &LinearVelocity, &PigeonInput, &mut Grips), With<Pigeon>>,
    mut bodies: GrabbableForces,
) {
    for (position, velocity, input, mut grips) in &mut pigeons {
        let eye = position.0 + Vec3::Y * EYE_HEIGHT;
        let look = look_rotation(input.look);
        let both_wings = matches!(grips.0, [Some(left), Some(right)] if left.body == right.body);
        for wing in [LEFT_WING, RIGHT_WING] {
            let Some(grip) = grips.0[wing] else {
                continue;
            };
            // The object is gone, for example despawned by the host.
            let Ok((mut forces, mass)) = bodies.get_mut(grip.body) else {
                grips.0[wing] = None;
                continue;
            };
            let anchor = forces.position().0 + forces.rotation().0 * grip.anchor;
            let error = hold_point(eye, look, wing, both_wings) - anchor;
            if error.length() > BREAK_DISTANCE {
                grips.0[wing] = None;
                continue;
            }
            let relative = forces.velocity_at_point(anchor) - velocity.0;
            forces.apply_force_at_point(grip_force(error, relative, mass.value()), anchor);
            let spin = forces.angular_velocity();
            forces.apply_angular_acceleration(-spin * HELD_SPIN_DAMPING);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    #[test]
    fn wings_hold_on_their_own_side() {
        let left = hold_point(Vec3::ZERO, Quat::IDENTITY, LEFT_WING, false);
        let right = hold_point(Vec3::ZERO, Quat::IDENTITY, RIGHT_WING, false);
        assert!(left.x < 0.0 && right.x > 0.0);
        assert!(left.z < 0.0 && right.z < 0.0, "both in front of the eye");
    }

    #[test]
    fn hold_point_turns_with_the_view() {
        let turned = hold_point(
            Vec3::ZERO,
            look_rotation(Vec2::new(FRAC_PI_2, 0.0)),
            LEFT_WING,
            false,
        );
        // A quarter turn left looks along -X.
        assert!(turned.x < -1.0, "{turned}");
    }

    #[test]
    fn both_wings_on_one_object_pull_to_the_center() {
        let left = hold_point(Vec3::ZERO, Quat::IDENTITY, LEFT_WING, true);
        let right = hold_point(Vec3::ZERO, Quat::IDENTITY, RIGHT_WING, true);
        assert_eq!(left, right);
        assert_eq!(left.x, 0.0);
    }

    #[test]
    fn grip_force_is_limited() {
        let force = grip_force(Vec3::new(0.0, 10.0, 0.0), Vec3::ZERO, 50.0);
        assert!((force.length() - MAX_GRIP_FORCE).abs() < 1e-3);
    }

    #[test]
    fn grip_force_pulls_toward_the_wing_and_brakes() {
        let pull = grip_force(Vec3::X * 0.1, Vec3::ZERO, 1.0);
        assert!(pull.x > 0.0);
        let brake = grip_force(Vec3::ZERO, Vec3::X, 1.0);
        assert!(brake.x < 0.0);
    }
}
