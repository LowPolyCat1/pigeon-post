//! The helm: a ship's wheel that turns the rudder. The rudder turns a moving ship, and the
//! keel stops the hull from sliding sideways through the cloud sea.
//!
//! The authority spawns the wheel with each ship and simulates the steering. A client
//! receives the rudder angle and turns the rudder blade.

use std::f32::consts::PI;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::cloud_sea::cloud_sea_depth;
use crate::controls::{ControlSystems, ControlValue, PartOf, WheelSpec, spawn_wheel};
use crate::net::NetMode;
use crate::ship::{Ship, ShipClass};

/// The rudder turns this far each way, in radians (35°).
pub const MAX_RUDDER_ANGLE: f32 = 35.0 * PI / 180.0;
/// The yaw acceleration in rad/s² per radian of rudder and per m/s of forward speed. At
/// 5 m/s and full rudder, the keel damping holds the turn at about 0.3 rad/s.
const RUDDER_GAIN: f32 = 0.8;
/// The keel damps sideways motion by this much per second. The isotropic drag of the
/// cloud sea is about 0.6 per second at the rest draft, so the hull keeps most of its
/// forward speed and loses its sideways speed.
const KEEL_DRAG: f32 = 8.0;
/// The keel works fully once its bottom is this deep in the cloud sea.
const KEEL_FULL_DEPTH: f32 = 0.5;
/// The keel points spread over this fraction of the hull length. Points along the hull
/// also damp the yaw, so a turn does not speed up without end.
const KEEL_SPREAD: f32 = 0.4;
const KEEL_POINTS: usize = 4;
/// The center of the wheel sits this high above the deck.
const WHEEL_HEIGHT: f32 = 0.9;

pub struct HelmPlugin;

impl Plugin for HelmPlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<RudderAngle, f32>()
            .add_observer(spawn_helm)
            .add_systems(
                FixedUpdate,
                (set_rudder, steer_ship).chain().after(ControlSystems),
            );
    }
}

/// The wheel of a helm. Its [`ControlValue`] sets the rudder of the ship it is part of.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelmWheel;

/// The angle of the rudder of a ship, in radians. A positive angle turns the ship to
/// starboard (+X).
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct RudderAngle(pub f32);

impl From<RudderAngle> for f32 {
    fn from(angle: RudderAngle) -> Self {
        angle.0
    }
}

impl From<f32> for RudderAngle {
    fn from(angle: f32) -> Self {
        RudderAngle(angle)
    }
}

/// The rudder angle for a wheel value in [-1, 1].
pub fn rudder_angle(wheel: f32) -> f32 {
    wheel.clamp(-1.0, 1.0) * MAX_RUDDER_ANGLE
}

/// The yaw acceleration about the up axis of the ship, in rad/s². A positive rudder turns
/// a ship that moves forward to starboard, which is a negative yaw. Without speed the
/// rudder does nothing, and going astern turns the other way.
pub fn rudder_yaw_acceleration(rudder: f32, forward_speed: f32) -> f32 {
    -RUDDER_GAIN * rudder * forward_speed
}

/// The hinge of the wheel in the frame of the ship. The hinge axis points to the bow, so a
/// positive angle is clockwise for a pigeon at the wheel that looks forward.
pub fn wheel_hinge(class: ShipClass) -> Transform {
    class.layout().helm
        * Transform::from_xyz(0.0, WHEEL_HEIGHT, 0.0).with_rotation(Quat::from_rotation_y(PI))
}

fn spawn_helm(
    add: On<Add, Ship>,
    mode: Res<NetMode>,
    classes: Query<&ShipClass>,
    mut commands: Commands,
) {
    if !mode.is_authority() {
        return;
    }
    let Ok(&class) = classes.get(add.entity) else {
        return;
    };
    commands.entity(add.entity).insert(RudderAngle::default());
    let wheel = spawn_wheel(
        &mut commands,
        add.entity,
        wheel_hinge(class),
        WheelSpec::default(),
    );
    commands.entity(wheel).insert(HelmWheel);
}

fn set_rudder(
    wheels: Query<(&PartOf, &ControlValue), With<HelmWheel>>,
    mut ships: Query<&mut RudderAngle, With<Ship>>,
) {
    for (part_of, value) in &wheels {
        if let Ok(mut rudder) = ships.get_mut(part_of.0) {
            rudder.set_if_neq(RudderAngle(rudder_angle(value.0)));
        }
    }
}

/// The keel points of a hull, in the frame of the ship, along the center line.
fn keel_points(class: ShipClass) -> impl Iterator<Item = Vec3> {
    let length = class.hull_size().z;
    (0..KEEL_POINTS).map(move |index| {
        let along = 2.0 * index as f32 / (KEEL_POINTS - 1) as f32 - 1.0;
        Vec3::new(0.0, 0.0, along * KEEL_SPREAD * length)
    })
}

fn steer_ship(
    time: Res<Time>,
    mut ships: Query<(Forces, &ComputedMass, &ShipClass, Option<&RudderAngle>), With<Ship>>,
) {
    let now = time.elapsed_secs();
    for (mut forces, mass, &class, rudder) in &mut ships {
        let position = forces.position().0;
        let rotation = forces.rotation().0;
        let bottom = Vec3::Y * class.hull_size().y / 2.0;
        let side = rotation * Vec3::X;
        let share = mass.value() / KEEL_POINTS as f32;
        let mut wetted = 0.0;
        for point in keel_points(class) {
            // The force acts at the height of the center, so the keel does not heel the
            // ship. The depth comes from the hull bottom, where the keel is.
            let center = position + rotation * point;
            let depth = cloud_sea_depth(center - rotation * bottom, now);
            let immersion = (depth / KEEL_FULL_DEPTH).clamp(0.0, 1.0);
            wetted += immersion / KEEL_POINTS as f32;
            let sideways = forces.velocity_at_point(center).dot(side);
            forces.apply_force_at_point(-side * sideways * KEEL_DRAG * share * immersion, center);
        }

        let Some(rudder) = rudder else {
            continue;
        };
        let forward_speed = forces.linear_velocity().dot(rotation * Vec3::NEG_Z);
        let yaw = rudder_yaw_acceleration(rudder.0, forward_speed) * wetted;
        forces.apply_angular_acceleration(rotation * Vec3::Y * yaw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_value_maps_to_rudder_limits() {
        assert_eq!(rudder_angle(0.0), 0.0);
        assert_eq!(rudder_angle(1.0), MAX_RUDDER_ANGLE);
        assert_eq!(rudder_angle(-1.0), -MAX_RUDDER_ANGLE);
        assert_eq!(rudder_angle(3.0), MAX_RUDDER_ANGLE);
        assert!((rudder_angle(0.5) - MAX_RUDDER_ANGLE / 2.0).abs() < 1e-6);
    }

    #[test]
    fn starboard_rudder_yaws_to_starboard() {
        // A negative yaw about +Y turns the bow (-Z) toward +X.
        assert!(rudder_yaw_acceleration(0.3, 5.0) < 0.0);
        assert!(rudder_yaw_acceleration(-0.3, 5.0) > 0.0);
    }

    #[test]
    fn rudder_needs_speed() {
        assert_eq!(rudder_yaw_acceleration(MAX_RUDDER_ANGLE, 0.0), 0.0);
    }

    #[test]
    fn rudder_reverses_astern() {
        assert!(rudder_yaw_acceleration(0.3, -2.0) > 0.0);
    }

    #[test]
    fn keel_points_lie_inside_hull() {
        for class in ShipClass::ALL {
            let half = class.hull_size() / 2.0;
            let points: Vec<Vec3> = keel_points(class).collect();
            assert_eq!(points.len(), KEEL_POINTS);
            assert!(points.iter().all(|point| point.z.abs() < half.z));
        }
    }

    #[test]
    fn wheel_axis_points_to_bow() {
        let axis = wheel_hinge(ShipClass::A).rotation * Vec3::Z;
        assert!(axis.dot(Vec3::NEG_Z) > 0.99, "{axis}");
    }
}
