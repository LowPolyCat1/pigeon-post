use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::cloud_sea::SEA_LEVEL;
use pigeon_post::controls::{ControlValue, ControlsPlugin, PartOf};
use pigeon_post::engine::{
    Coal, Engine, EnginePart, EnginePlugin, FUEL_PER_LUMP, FURNACE_SIZE, IGNITION_FIRE,
    MOUTH_BOTTOM, MOUTH_TOP, bunker_inner_size, coal_body, starting_coal,
};
use pigeon_post::helm::HelmPlugin;
use pigeon_post::net::NetMode;
use pigeon_post::ship::{ShipClass, ShipPlugin, ship_body};

/// The torques that the test applies about the hinge axis of each engine lever.
#[derive(Resource, Default)]
struct LeverTorques {
    hatch: f32,
    throttle: f32,
    ignition: f32,
}

fn push_levers(torques: Res<LeverTorques>, mut levers: Query<(Forces, &EnginePart)>) {
    for (mut forces, part) in &mut levers {
        let torque = match part {
            EnginePart::Hatch => torques.hatch,
            EnginePart::Throttle => torques.throttle,
            EnginePart::Ignition => torques.ignition,
        };
        let axis = forces.rotation().0 * Vec3::Z;
        forces.apply_torque(axis * torque);
    }
}

/// A headless app as the authority, with a ship of `class` at its rest draft far from the
/// dock. The helm keeps the keel in the cloud sea, as in the game.
fn app_with_ship(class: ShipClass) -> (App, Entity) {
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
        EnginePlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )))
    .init_resource::<LeverTorques>()
    .add_systems(FixedUpdate, push_levers);
    app.init_resource::<NetMode>();
    app.finish();
    let ship = app
        .world_mut()
        .spawn((
            ship_body(class),
            Transform::from_xyz(-40.0, SEA_LEVEL + class.hull_size().y * 0.1, 0.0),
        ))
        .id();
    // Settle on the waves, and let the coal settle in the bunker.
    run_seconds(&mut app, 2.0);
    (app, ship)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn lever(app: &mut App, ship: Entity, wanted: EnginePart) -> Entity {
    let mut levers = app.world_mut().query::<(Entity, &EnginePart, &PartOf)>();
    levers
        .iter(app.world())
        .find(|(_, part, part_of)| **part == wanted && part_of.0 == ship)
        .map(|(entity, _, _)| entity)
        .expect("ship has the lever")
}

fn lever_value(app: &mut App, ship: Entity, part: EnginePart) -> f32 {
    let entity = lever(app, ship, part);
    app.world()
        .get::<ControlValue>(entity)
        .expect("lever has a value")
        .0
}

/// Pushes one lever with `torque` for a moment, then lets go of it.
fn work_lever(app: &mut App, part: EnginePart, torque: f32) {
    let set = |app: &mut App, torque: f32| {
        let mut torques = app.world_mut().resource_mut::<LeverTorques>();
        match part {
            EnginePart::Hatch => torques.hatch = torque,
            EnginePart::Throttle => torques.throttle = torque,
            EnginePart::Ignition => torques.ignition = torque,
        }
    };
    set(app, torque);
    run_seconds(app, 0.8);
    set(app, 0.0);
    run_seconds(app, 0.3);
}

fn engine(app: &App, ship: Entity) -> Engine {
    *app.world().get::<Engine>(ship).expect("ship has an engine")
}

fn set_fuel(app: &mut App, ship: Entity, fuel: f32) {
    app.world_mut()
        .get_mut::<Engine>(ship)
        .expect("ship has an engine")
        .fuel = fuel;
}

/// A point in the frame of the furnace mount, in world space.
fn furnace_point(app: &App, ship: Entity, local: Vec3) -> Vec3 {
    let position = app.world().get::<Position>(ship).expect("ship position").0;
    let rotation = app.world().get::<Rotation>(ship).expect("ship rotation").0;
    let mount = ShipClass::A.layout().furnace;
    position + rotation * mount.transform_point(local)
}

fn forward_speed(app: &App, ship: Entity) -> f32 {
    let velocity = app
        .world()
        .get::<LinearVelocity>(ship)
        .expect("ship velocity")
        .0;
    let rotation = app.world().get::<Rotation>(ship).expect("ship rotation").0;
    velocity.dot(rotation * Vec3::NEG_Z)
}

fn spawn_coal(app: &mut App, ship: Entity, local: Vec3, local_velocity: Vec3) -> Entity {
    let position = furnace_point(app, ship, local);
    let rotation = app.world().get::<Rotation>(ship).expect("ship rotation").0;
    let ship_velocity = app
        .world()
        .get::<LinearVelocity>(ship)
        .expect("ship velocity")
        .0;
    app.world_mut()
        .spawn((
            coal_body(),
            Transform::from_translation(position),
            LinearVelocity(ship_velocity + rotation * local_velocity),
        ))
        .id()
}

const MOUTH_MIDDLE: f32 = (MOUTH_BOTTOM + MOUTH_TOP) / 2.0;

/// Lights the engine with the ignition lever and sets the throttle to full ahead.
fn start_engine(app: &mut App, ship: Entity) {
    work_lever(app, EnginePart::Ignition, 6.0);
    assert!(engine(app, ship).lit, "engine did not light");
    work_lever(app, EnginePart::Throttle, 8.0);
    let throttle = engine(app, ship).throttle;
    assert!(throttle > 0.95, "throttle {throttle}");
}

#[test]
fn bunker_holds_starting_coal() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    let bunker = ShipClass::A.layout().coal_bunker.translation;
    let position = app.world().get::<Position>(ship).expect("ship position").0;
    let rotation = app.world().get::<Rotation>(ship).expect("ship rotation").0;
    let mut lumps = app
        .world_mut()
        .query_filtered::<(&Position, &PartOf), With<Coal>>();
    let inside: Vec<Vec3> = lumps
        .iter(app.world())
        .filter(|(_, part_of)| part_of.0 == ship)
        .map(|(lump, _)| rotation.inverse() * (lump.0 - position) - bunker)
        .collect();
    assert_eq!(inside.len(), starting_coal(ShipClass::A));
    let half = bunker_inner_size(ShipClass::A) / 2.0;
    for lump in inside {
        assert!(
            lump.x.abs() < half && lump.z.abs() < half && lump.y > 0.0,
            "lump at {lump} left the bunker"
        );
    }
}

#[test]
fn lump_in_open_furnace_adds_fuel() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    work_lever(&mut app, EnginePart::Hatch, 8.0);
    let hatch = lever_value(&mut app, ship, EnginePart::Hatch);
    assert!(hatch > 0.9, "hatch value {hatch}");

    let aft_of_mouth = Vec3::new(0.0, MOUTH_MIDDLE, FURNACE_SIZE.z / 2.0 + 0.3);
    let lump = spawn_coal(&mut app, ship, aft_of_mouth, Vec3::new(0.0, 0.5, -2.5));
    run_seconds(&mut app, 1.5);
    assert!(app.world().get_entity(lump).is_err(), "lump not consumed");
    assert_eq!(engine(&app, ship).fuel, FUEL_PER_LUMP);
}

#[test]
fn closed_hatch_keeps_coal_until_opened() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    // A lump that is already in the chamber, behind the shut hatch.
    let lump = spawn_coal(&mut app, ship, Vec3::Y * MOUTH_MIDDLE, Vec3::ZERO);
    run_seconds(&mut app, 1.0);
    assert!(app.world().get_entity(lump).is_ok(), "lump burned");
    assert_eq!(engine(&app, ship).fuel, 0.0);

    work_lever(&mut app, EnginePart::Hatch, 8.0);
    run_seconds(&mut app, 0.5);
    assert!(app.world().get_entity(lump).is_err(), "lump not consumed");
    assert_eq!(engine(&app, ship).fuel, FUEL_PER_LUMP);
}

#[test]
fn ignition_without_fuel_fails() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    work_lever(&mut app, EnginePart::Ignition, 6.0);
    let pulled = lever_value(&mut app, ship, EnginePart::Ignition);
    assert!(pulled >= IGNITION_FIRE, "ignition value {pulled}");
    assert!(!engine(&app, ship).lit);

    // Fuel alone does not light the fire. The lever must go back and be pulled again.
    set_fuel(&mut app, ship, 30.0);
    run_seconds(&mut app, 0.5);
    assert!(!engine(&app, ship).lit, "lit without a pull");
    work_lever(&mut app, EnginePart::Ignition, -6.0);
    work_lever(&mut app, EnginePart::Ignition, 6.0);
    assert!(engine(&app, ship).lit, "second pull did not light");
}

#[test]
fn lit_engine_drives_ship_forward() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    set_fuel(&mut app, ship, 300.0);
    work_lever(&mut app, EnginePart::Hatch, 8.0);
    start_engine(&mut app, ship);
    let start = app.world().get::<Position>(ship).expect("ship position").0;
    let mut speeds = Vec::new();
    for _ in 0..4 {
        run_seconds(&mut app, 5.0);
        speeds.push(forward_speed(&app, ship));
    }
    let end = app.world().get::<Position>(ship).expect("ship position").0;
    let rotation = app.world().get::<Rotation>(ship).expect("ship rotation").0;
    println!("class A forward speed every 5 s: {speeds:?}");
    let speed = speeds[3];
    assert!(speed > 2.5 && speed < 8.0, "speed {speed} m/s");
    assert!((end - start).dot(rotation * Vec3::NEG_Z) > 30.0);
    let fuel = engine(&app, ship).fuel;
    assert!(fuel < 300.0 - 15.0, "fuel {fuel}");
    let hatch = lever_value(&mut app, ship, EnginePart::Hatch);
    assert!(hatch > 0.7, "hatch swung shut to {hatch} under way");
}

#[test]
fn larger_classes_also_reach_speed() {
    for class in [ShipClass::B, ShipClass::C] {
        let (mut app, ship) = app_with_ship(class);
        set_fuel(&mut app, ship, 300.0);
        start_engine(&mut app, ship);
        run_seconds(&mut app, 15.0);
        let speed = forward_speed(&app, ship);
        println!("class {class:?} forward speed after 15 s: {speed}");
        assert!(
            speed > 2.5 && speed < 8.0,
            "class {class:?} speed {speed} m/s"
        );
    }
}

#[test]
fn empty_engine_stops_pushing() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    set_fuel(&mut app, ship, 4.0);
    start_engine(&mut app, ship);
    run_seconds(&mut app, 4.0);
    let engine_state = engine(&app, ship);
    assert!(!engine_state.lit, "fire still burns");
    assert_eq!(engine_state.fuel, 0.0);

    let moving = forward_speed(&app, ship);
    assert!(moving > 0.5, "speed {moving} m/s before the coast");
    run_seconds(&mut app, 5.0);
    let coasting = forward_speed(&app, ship);
    assert!(
        coasting < moving * 0.3,
        "speed {coasting} m/s after the coast"
    );
}

#[test]
fn despawned_ship_takes_its_engine() {
    let (mut app, ship) = app_with_ship(ShipClass::A);
    app.world_mut().despawn(ship);
    app.update();
    let mut parts = app.world_mut().query::<&PartOf>();
    let left = parts
        .iter(app.world())
        .filter(|part_of| part_of.0 == ship)
        .count();
    assert_eq!(left, 0);
}
