use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::cloud_sea::SEA_LEVEL;
use pigeon_post::controls::{ControlValue, ControlsPlugin, PartOf};
use pigeon_post::helm::{HelmPlugin, HelmWheel, RudderAngle};
use pigeon_post::net::NetMode;
use pigeon_post::ship::{ShipClass, ShipPlugin, ship_body};

/// Turns the wheel about its hinge axis while the torque is not zero.
#[derive(Resource, Default)]
struct WheelTorque(f32);

fn turn_wheel(torque: Res<WheelTorque>, mut wheels: Query<Forces, With<HelmWheel>>) {
    for mut forces in &mut wheels {
        let axis = forces.rotation().0 * Vec3::Z;
        forces.apply_torque(axis * torque.0);
    }
}

/// A headless app as the authority, with a class A ship at its rest draft far from the dock.
fn app_with_ship() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        bevy::state::app::StatesPlugin,
        bevy_replicon::RepliconPlugins,
        PhysicsPlugins::default(),
        ShipPlugin,
        ControlsPlugin,
        HelmPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )))
    .init_resource::<WheelTorque>()
    .add_systems(FixedUpdate, turn_wheel);
    app.init_resource::<NetMode>();
    app.finish();
    let class = ShipClass::A;
    let ship = app
        .world_mut()
        .spawn((
            ship_body(class),
            Transform::from_xyz(-40.0, SEA_LEVEL + class.hull_size().y * 0.1, 0.0),
        ))
        .id();
    // Settle on the waves before a test moves the ship.
    run_seconds(&mut app, 2.0);
    (app, ship)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn helm_of(app: &mut App, ship: Entity) -> Entity {
    let mut wheels = app
        .world_mut()
        .query_filtered::<(Entity, &PartOf), With<HelmWheel>>();
    wheels
        .iter(app.world())
        .find(|(_, part_of)| part_of.0 == ship)
        .map(|(wheel, _)| wheel)
        .expect("ship has a helm")
}

fn rotation(app: &App, ship: Entity) -> Quat {
    app.world()
        .get::<Rotation>(ship)
        .expect("ship has a rotation")
        .0
}

/// The heading of the bow in the horizontal plane. Turning to starboard makes it negative.
fn heading(app: &App, ship: Entity) -> f32 {
    let bow = rotation(app, ship) * Vec3::NEG_Z;
    (-bow.x).atan2(-bow.z)
}

fn set_velocity(app: &mut App, ship: Entity, velocity: Vec3) {
    app.world_mut()
        .get_mut::<LinearVelocity>(ship)
        .expect("ship has a velocity")
        .0 = velocity;
}

fn velocity(app: &App, ship: Entity) -> Vec3 {
    app.world()
        .get::<LinearVelocity>(ship)
        .expect("ship has a velocity")
        .0
}

/// Turns the wheel clockwise for a pigeon that looks forward, then lets go.
fn turn_wheel_to_starboard(app: &mut App, ship: Entity) -> f32 {
    app.insert_resource(WheelTorque(12.0));
    run_seconds(app, 0.6);
    app.insert_resource(WheelTorque(0.0));
    run_seconds(app, 1.0);
    let wheel = helm_of(app, ship);
    app.world()
        .get::<ControlValue>(wheel)
        .expect("wheel has a value")
        .0
}

/// Keeps the ship at `speed` along its bow for `seconds`, as an engine would, and returns
/// the change of the heading.
fn sail(app: &mut App, ship: Entity, speed: f32, seconds: f32) -> f32 {
    let start = heading(app, ship);
    for _ in 0..(seconds * 60.0) as usize {
        let bow = rotation(app, ship) * Vec3::NEG_Z;
        let flat = Vec3::new(bow.x, 0.0, bow.z).normalize_or_zero();
        let vertical = velocity(app, ship).y;
        set_velocity(app, ship, flat * speed + Vec3::Y * vertical);
        app.update();
    }
    heading(app, ship) - start
}

#[test]
fn turned_wheel_sets_rudder() {
    let (mut app, ship) = app_with_ship();
    let value = turn_wheel_to_starboard(&mut app, ship);
    assert!(value > 0.3, "wheel value {value}");
    let rudder = app
        .world()
        .get::<RudderAngle>(ship)
        .expect("ship has a rudder")
        .0;
    assert!(rudder > 0.1, "rudder angle {rudder}");

    run_seconds(&mut app, 2.0);
    let wheel = helm_of(&mut app, ship);
    let later = app
        .world()
        .get::<ControlValue>(wheel)
        .expect("wheel has a value")
        .0;
    assert!((later - value).abs() < 0.03, "wheel drifted to {later}");
}

#[test]
fn moving_ship_turns_with_the_wheel() {
    let (mut app, ship) = app_with_ship();
    turn_wheel_to_starboard(&mut app, ship);
    let turn = sail(&mut app, ship, 5.0, 3.0);
    assert!(turn < -0.2, "ship turned only {turn} rad");
}

#[test]
fn ship_at_rest_does_not_turn() {
    let (mut app, ship) = app_with_ship();
    turn_wheel_to_starboard(&mut app, ship);
    let start = heading(&app, ship);
    run_seconds(&mut app, 3.0);
    let turn = heading(&app, ship) - start;
    assert!(turn.abs() < 0.05, "ship at rest turned {turn} rad");
}

#[test]
fn straight_wheel_keeps_course() {
    let (mut app, ship) = app_with_ship();
    let turn = sail(&mut app, ship, 5.0, 3.0);
    assert!(turn.abs() < 0.05, "ship turned {turn} rad");
}

/// Sets the ship moving along `direction` in its own frame, and returns how far it drifts
/// along that direction in two seconds.
fn drift(direction: Vec3) -> f32 {
    let (mut app, ship) = app_with_ship();
    let along = rotation(&app, ship) * direction;
    let along = Vec3::new(along.x, 0.0, along.z).normalize_or_zero();
    set_velocity(&mut app, ship, along * 3.0);
    let start = app
        .world()
        .get::<Position>(ship)
        .expect("ship has a position")
        .0;
    run_seconds(&mut app, 2.0);
    let end = app
        .world()
        .get::<Position>(ship)
        .expect("ship has a position")
        .0;
    (end - start).dot(along)
}

#[test]
fn keel_stops_sideways_drift() {
    let forward = drift(Vec3::NEG_Z);
    let sideways = drift(Vec3::X);
    assert!(forward > 2.5, "forward drift {forward} m");
    assert!(sideways < forward * 0.25, "sideways drift {sideways} m");
}

#[test]
fn despawned_ship_takes_its_helm() {
    let (mut app, ship) = app_with_ship();
    helm_of(&mut app, ship);
    app.world_mut().despawn(ship);
    app.update();
    let mut parts = app.world_mut().query::<&PartOf>();
    let left = parts
        .iter(app.world())
        .filter(|part_of| part_of.0 == ship)
        .count();
    assert_eq!(left, 0);
    let mut wheels = app.world_mut().query_filtered::<&PartOf, With<HelmWheel>>();
    // The ship at the dock keeps its own helm.
    assert_eq!(wheels.iter(app.world()).count(), 1);
}
