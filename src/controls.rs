//! Ship controls that a pigeon works with its wings: levers, wheels and cranks. Each control
//! is a small rigid body on a hinge. A pigeon grabs it and drags it, and joint friction
//! keeps it where the pigeon leaves it. The angle of the hinge gives a [`ControlValue`].
//!
//! Only the authority simulates the controls. A client receives the transform, the kind and
//! the value, and adds the mesh.

use std::f32::consts::{PI, TAU};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::grab::Grabbable;

/// The handle of a lever and the arm of a crank are this thick.
pub const HANDLE_THICKNESS: f32 = 0.08;
/// The spokes of a wheel reach past the rim by this much, so a wing can hold them.
pub const SPOKE_OVERHANG: f32 = 0.15;
pub const SPOKE_THICKNESS: f32 = 0.1;
/// The grip of a crank sticks out along the hinge axis, toward the pigeon.
pub const CRANK_GRIP_LENGTH: f32 = 0.3;
/// Light enough that one wing swings a control, heavy enough that the joint to a ship of
/// several hundred kilograms stays stable.
const CONTROL_DENSITY: f32 = 200.0;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<Control, (u8, f32)>()
            .replicate_as::<ControlValue, f32>()
            .add_systems(FixedUpdate, read_controls.in_set(ControlSystems));
    }
}

/// The system that updates each [`ControlValue`]. A system that reads a value runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControlSystems;

/// The kind and the size of a control. A client builds the mesh from it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum Control {
    /// The handle reaches this far along the local Y axis of the hinge.
    Lever { length: f32 },
    /// The rim has this radius. The spokes reach past it.
    Wheel { radius: f32 },
    /// The arm reaches this far along the local Y axis, and the grip sits at its end.
    Crank { radius: f32 },
}

impl From<Control> for (u8, f32) {
    fn from(control: Control) -> Self {
        match control {
            Control::Lever { length } => (0, length),
            Control::Wheel { radius } => (1, radius),
            Control::Crank { radius } => (2, radius),
        }
    }
}

// Only `From<Control>` writes this pair, so a kind above 2 does not occur.
impl From<(u8, f32)> for Control {
    fn from((kind, size): (u8, f32)) -> Self {
        match kind {
            0 => Control::Lever { length: size },
            1 => Control::Wheel { radius: size },
            _ => Control::Crank { radius: size },
        }
    }
}

/// The setting of a control. A lever gives a value between its two end values, a wheel a
/// value in [-1, 1], and a crank the number of turns.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct ControlValue(pub f32);

impl From<ControlValue> for f32 {
    fn from(value: ControlValue) -> Self {
        value.0
    }
}

impl From<f32> for ControlValue {
    fn from(value: f32) -> Self {
        ControlValue(value)
    }
}

/// The body that carries a part, for example the ship of a control. When the body is
/// despawned, its parts are despawned too.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = Parts)]
pub struct PartOf(pub Entity);

/// The parts on a body. See [`PartOf`].
#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
#[relationship_target(relationship = PartOf, linked_spawn)]
pub struct Parts(Vec<Entity>);

impl Parts {
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// A lever turns between two angles. The value moves linearly from `values.0` at
/// `angles.0` to `values.1` at `angles.1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeverSpec {
    pub length: f32,
    /// In radians, within (-π, π), the smaller first.
    pub angles: (f32, f32),
    pub values: (f32, f32),
    /// The torque in N·m that the lever resists with. A pigeon pulls with up to 40 N
    /// per wing, at the length of the handle.
    pub friction: f32,
}

impl Default for LeverSpec {
    fn default() -> Self {
        LeverSpec {
            length: 0.8,
            angles: (-0.7, 0.7),
            values: (0.0, 1.0),
            friction: 4.0,
        }
    }
}

/// A wheel turns up to `max_angle` each way. The value is the angle over `max_angle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelSpec {
    pub radius: f32,
    /// In radians, below π, because the joint measures the angle within (-π, π].
    pub max_angle: f32,
    pub friction: f32,
}

impl Default for WheelSpec {
    fn default() -> Self {
        WheelSpec {
            radius: 0.5,
            max_angle: 2.4,
            friction: 6.0,
        }
    }
}

/// A crank turns without a stop. The value counts the turns, positive along the hinge
/// axis, and stays between `turns.0` and `turns.1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrankSpec {
    pub radius: f32,
    pub turns: (f32, f32),
    pub friction: f32,
}

impl Default for CrankSpec {
    fn default() -> Self {
        CrankSpec {
            radius: 0.35,
            turns: (f32::NEG_INFINITY, f32::INFINITY),
            friction: 4.0,
        }
    }
}

/// How the authority turns the hinge angle into a value.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum ControlMapping {
    Lever {
        angles: (f32, f32),
        values: (f32, f32),
    },
    Wheel {
        max_angle: f32,
    },
    Crank {
        turns: (f32, f32),
        /// The hinge angle of the last step. The change since then adds to the turns.
        last_angle: f32,
    },
}

/// The hinge of a control in the frame of its body. The local Z axis is the hinge axis.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ControlHinge(pub Transform);

/// Wraps an angle into [-π, π).
pub fn wrap_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// The angle in [-π, π) of a rotation about the Z axis. A small tilt off the axis, from a
/// soft joint, does not change it much.
pub fn hinge_angle(relative: Quat) -> f32 {
    wrap_angle(2.0 * relative.z.atan2(relative.w))
}

/// The value of a lever at `angle`. Past an end, the value stays at the end value.
pub fn lever_value(angle: f32, angles: (f32, f32), values: (f32, f32)) -> f32 {
    let span = angles.1 - angles.0;
    if span.abs() <= f32::EPSILON {
        return values.0;
    }
    let along = ((angle - angles.0) / span).clamp(0.0, 1.0);
    values.0 + along * (values.1 - values.0)
}

/// The value of a wheel at `angle`, in [-1, 1].
pub fn wheel_value(angle: f32, max_angle: f32) -> f32 {
    if max_angle <= f32::EPSILON {
        return 0.0;
    }
    (angle / max_angle).clamp(-1.0, 1.0)
}

/// The turns of a crank after its hinge angle moved from `last_angle` to `angle`. A crank
/// turns less than half a turn per step, so the shorter way round is the real one.
pub fn crank_turns(turns: f32, last_angle: f32, angle: f32, limits: (f32, f32)) -> f32 {
    let delta = wrap_angle(angle - last_angle);
    (turns + delta / TAU).clamp(limits.0, limits.1)
}

/// Spawns a lever on `body` at `hinge`, a transform in the frame of the body. The handle
/// points along the local Y axis of the hinge and turns about its local Z axis.
pub fn spawn_lever(
    commands: &mut Commands,
    body: Entity,
    hinge: Transform,
    spec: LeverSpec,
) -> Entity {
    let collider = Collider::compound(vec![(
        Vec3::Y * spec.length / 2.0,
        Quat::IDENTITY,
        Collider::cuboid(HANDLE_THICKNESS, spec.length, HANDLE_THICKNESS),
    )]);
    spawn_control(
        commands,
        body,
        hinge,
        Control::Lever {
            length: spec.length,
        },
        ControlMapping::Lever {
            angles: spec.angles,
            values: spec.values,
        },
        Some(spec.angles),
        spec.friction,
        collider,
        lever_value(0.0, spec.angles, spec.values),
    )
}

/// Spawns a wheel on `body` at `hinge`. The wheel lies in the local XY plane of the hinge
/// and turns about its local Z axis. The spokes are the handles.
pub fn spawn_wheel(
    commands: &mut Commands,
    body: Entity,
    hinge: Transform,
    spec: WheelSpec,
) -> Entity {
    let spoke = Collider::cuboid(
        2.0 * (spec.radius + SPOKE_OVERHANG),
        SPOKE_THICKNESS,
        SPOKE_THICKNESS,
    );
    let spokes = (0..4)
        .map(|index| {
            (
                Vec3::ZERO,
                Quat::from_rotation_z(index as f32 * PI / 4.0),
                spoke.clone(),
            )
        })
        .collect();
    spawn_control(
        commands,
        body,
        hinge,
        Control::Wheel {
            radius: spec.radius,
        },
        ControlMapping::Wheel {
            max_angle: spec.max_angle,
        },
        Some((-spec.max_angle, spec.max_angle)),
        spec.friction,
        Collider::compound(spokes),
        0.0,
    )
}

/// Spawns a crank on `body` at `hinge`. The arm points along the local Y axis, the grip at
/// its end sticks out along the local Z axis, and the crank turns about that axis.
pub fn spawn_crank(
    commands: &mut Commands,
    body: Entity,
    hinge: Transform,
    spec: CrankSpec,
) -> Entity {
    let collider = Collider::compound(vec![
        (
            Vec3::Y * spec.radius / 2.0,
            Quat::IDENTITY,
            Collider::cuboid(HANDLE_THICKNESS, spec.radius, HANDLE_THICKNESS),
        ),
        (
            Vec3::new(0.0, spec.radius, CRANK_GRIP_LENGTH / 2.0),
            Quat::IDENTITY,
            Collider::cuboid(SPOKE_THICKNESS, SPOKE_THICKNESS, CRANK_GRIP_LENGTH),
        ),
    ]);
    let start = 0.0_f32.clamp(spec.turns.0, spec.turns.1);
    spawn_control(
        commands,
        body,
        hinge,
        Control::Crank {
            radius: spec.radius,
        },
        ControlMapping::Crank {
            turns: spec.turns,
            last_angle: 0.0,
        },
        None,
        spec.friction,
        collider,
        start,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_control(
    commands: &mut Commands,
    body: Entity,
    hinge: Transform,
    control_kind: Control,
    mapping: ControlMapping,
    limits: Option<(f32, f32)>,
    friction: f32,
    collider: Collider,
    value: f32,
) -> Entity {
    // The body may be spawned in the same command batch, so the control reads its
    // transform when the commands apply. Then the physics starts from the right pose.
    let entity = commands.spawn_empty().id();
    commands.queue(move |world: &mut World| {
        let Some(&pose) = world.get::<Transform>(body) else {
            warn!("The control {entity} has no body: entity {body} has no transform.");
            if let Ok(control) = world.get_entity_mut(entity) {
                control.despawn();
            }
            return;
        };
        let Ok(mut control) = world.get_entity_mut(entity) else {
            return;
        };
        control.insert((
            (
                control_kind,
                ControlValue(value),
                mapping,
                ControlHinge(hinge),
                PartOf(body),
                Replicated,
                pose * hinge,
            ),
            (
                RigidBody::Dynamic,
                collider,
                ColliderDensity(CONTROL_DENSITY),
                // The hinge holds the control. Gravity would swing a lever down.
                GravityScale(0.0),
                // A control on a rocking ship must follow the ship at once.
                SleepingDisabled,
                Grabbable,
                TransformInterpolation,
            ),
        ));

        // A motor that aims for no relative spin, with a torque limit, acts as dry
        // friction: it holds the control still against small torques and gives way to a
        // pigeon.
        let friction_motor = AngularMotor::new(MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: 1.0,
        })
        .with_max_torque(friction);
        let mut joint = RevoluteJoint::new(body, entity)
            .with_local_frame1(Isometry3d::new(hinge.translation, hinge.rotation))
            .with_local_frame2(Isometry3d::IDENTITY)
            .with_hinge_axis(Vec3::Z)
            .with_motor(friction_motor);
        if let Some((min, max)) = limits {
            joint = joint.with_angle_limits(min, max);
        }
        world.spawn((joint, JointCollisionDisabled, PartOf(body)));
    });
    entity
}

/// Updates the value of each control from the angle of its hinge.
fn read_controls(
    mut controls: Query<(
        &PartOf,
        &Rotation,
        &ControlHinge,
        &mut ControlMapping,
        &mut ControlValue,
    )>,
    bodies: Query<&Rotation, Without<ControlHinge>>,
) {
    for (part_of, rotation, hinge, mut mapping, mut value) in &mut controls {
        let Ok(body) = bodies.get(part_of.0) else {
            continue;
        };
        let relative = (body.0 * hinge.0.rotation).inverse() * rotation.0;
        let angle = hinge_angle(relative);
        let new_value = match *mapping {
            ControlMapping::Lever { angles, values } => lever_value(angle, angles, values),
            ControlMapping::Wheel { max_angle } => wheel_value(angle, max_angle),
            ControlMapping::Crank {
                turns,
                ref mut last_angle,
            } => {
                let counted = crank_turns(value.0, *last_angle, angle, turns);
                *last_angle = angle;
                counted
            }
        };
        // Only a real change marks the value as changed, so replication stays quiet.
        value.set_if_neq(ControlValue(new_value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: (f32, f32) = (f32::NEG_INFINITY, f32::INFINITY);

    #[test]
    fn hinge_angle_reads_rotation_about_z() {
        for angle in [-3.0, -1.2, 0.0, 0.4, 3.0] {
            let read = hinge_angle(Quat::from_rotation_z(angle));
            assert!((read - angle).abs() < 1e-5, "{angle} read as {read}");
        }
    }

    #[test]
    fn hinge_angle_ignores_quaternion_sign() {
        let read = hinge_angle(-Quat::from_rotation_z(0.5));
        assert!((read - 0.5).abs() < 1e-5, "{read}");
    }

    #[test]
    fn lever_maps_angles_to_values() {
        let angles = (-0.5, 0.5);
        assert_eq!(lever_value(-0.5, angles, (0.0, 1.0)), 0.0);
        assert_eq!(lever_value(0.0, angles, (0.0, 1.0)), 0.5);
        assert_eq!(lever_value(0.5, angles, (-1.0, 1.0)), 1.0);
        assert_eq!(lever_value(2.0, angles, (0.0, 1.0)), 1.0);
        assert_eq!(lever_value(-2.0, angles, (0.0, 1.0)), 0.0);
    }

    #[test]
    fn wheel_value_is_angle_over_max() {
        assert_eq!(wheel_value(1.2, 2.4), 0.5);
        assert_eq!(wheel_value(-2.4, 2.4), -1.0);
        assert_eq!(wheel_value(3.0, 2.4), 1.0);
        assert_eq!(wheel_value(1.0, 0.0), 0.0);
    }

    #[test]
    fn crank_counts_turns_over_steps() {
        let mut turns = 0.0;
        let mut last = 0.0;
        for step in 1..=40 {
            let angle = wrap_angle(step as f32 * TAU / 20.0);
            turns = crank_turns(turns, last, angle, LIMITS);
            last = angle;
        }
        assert!((turns - 2.0).abs() < 1e-4, "{turns}");
    }

    #[test]
    fn crank_wraps_at_half_turn() {
        // From just below π to just above -π is a small step forward, not a turn back.
        let turns = crank_turns(0.0, PI - 0.05, -PI + 0.05, LIMITS);
        assert!((turns - 0.1 / TAU).abs() < 1e-4, "{turns}");
        let back = crank_turns(0.0, -PI + 0.05, PI - 0.05, LIMITS);
        assert!((back + 0.1 / TAU).abs() < 1e-4, "{back}");
    }

    #[test]
    fn crank_stays_inside_limits() {
        assert_eq!(crank_turns(2.0, 0.0, 1.0, (0.0, 2.0)), 2.0);
        assert_eq!(crank_turns(0.0, 0.0, -1.0, (0.0, 2.0)), 0.0);
    }

    #[test]
    fn control_survives_conversion() {
        for control in [
            Control::Lever { length: 0.8 },
            Control::Wheel { radius: 0.5 },
            Control::Crank { radius: 0.3 },
        ] {
            assert_eq!(Control::from(<(u8, f32)>::from(control)), control);
        }
    }
}
