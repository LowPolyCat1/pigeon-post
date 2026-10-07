use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::cloud_sea::SEA_LEVEL;
use pigeon_post::controls::{ControlValue, ControlsPlugin, PartOf};
use pigeon_post::helm::HelmPlugin;
use pigeon_post::net::NetMode;
use pigeon_post::sails::{
    HOIST_TURNS, HalyardCrank, MAX_SHEET_ANGLE, Sails, SailsPlugin, SheetLever,
};
use pigeon_post::ship::{ShipClass, ShipPlugin, ship_body};
use pigeon_post::wind::Wind;

/// Turns each sheet lever about its hinge axis with this torque, until the sail reaches
/// the target angle. Without a target, the torque stays on.
#[derive(Resource, Default)]
struct SheetTorque(f32, Option<f32>);

/// Turns each halyard crank about its hinge axis with this torque.
#[derive(Resource, Default)]
struct CrankTorque(f32);

/// Both queries move bodies, so they must not overlap.
type OnlyLevers = (With<SheetLever>, Without<HalyardCrank>);

fn turn_controls(
    sheet: Res<SheetTorque>,
    crank: Res<CrankTorque>,
    mut levers: Query<(Forces, &ControlValue), OnlyLevers>,
    mut cranks: Query<Forces, (With<HalyardCrank>, Without<SheetLever>)>,
) {
    for (mut forces, value) in &mut levers {
        let torque = match sheet.1 {
            Some(target) if (target - value.0).abs() < 0.02 => 0.0,
            Some(target) => sheet.0.abs() * (target - value.0).signum(),
            None => sheet.0,
        };
        let axis = forces.rotation().0 * Vec3::Z;
        forces.apply_torque(axis * torque);
    }
    for mut forces in &mut cranks {
        let axis = forces.rotation().0 * Vec3::Z;
        forces.apply_torque(axis * crank.0);
    }
}

/// A headless app as the authority, with a ship of `class` at its rest draft far from the
/// dock and a steady wind.
fn app_with_ship(class: ShipClass, wind: Vec3) -> (App, Entity) {
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
        SailsPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )))
    .insert_resource(Wind {
        direction: Vec2::new(wind.x, wind.z).normalize_or_zero(),
        strength: wind.length(),
    })
    .init_resource::<SheetTorque>()
    .init_resource::<CrankTorque>()
    .add_systems(FixedUpdate, turn_controls);
    app.init_resource::<NetMode>();
    app.finish();
    let ship = app
        .world_mut()
        .spawn((
            ship_body(class),
            Transform::from_xyz(-60.0, SEA_LEVEL + class.hull_size().y * 0.1, 0.0),
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

/// Sets each halyard crank of the ship to `turns`. The crank counts on from its value.
fn set_cranks(app: &mut App, ship: Entity, turns: f32) {
    let mut cranks = app
        .world_mut()
        .query_filtered::<(&PartOf, &mut ControlValue), With<HalyardCrank>>();
    for (part_of, mut value) in cranks.iter_mut(app.world_mut()) {
        if part_of.0 == ship {
            value.0 = turns;
        }
    }
}

fn sails(app: &App, ship: Entity) -> Sails {
    app.world()
        .get::<Sails>(ship)
        .expect("ship has sails")
        .clone()
}

fn velocity(app: &App, ship: Entity) -> Vec3 {
    app.world()
        .get::<LinearVelocity>(ship)
        .expect("ship has a velocity")
        .0
}

fn position(app: &App, ship: Entity) -> Vec3 {
    app.world()
        .get::<Position>(ship)
        .expect("ship has a position")
        .0
}

fn bow(app: &App, ship: Entity) -> Vec3 {
    let rotation = app
        .world()
        .get::<Rotation>(ship)
        .expect("ship has a rotation")
        .0;
    let bow = rotation * Vec3::NEG_Z;
    Vec3::new(bow.x, 0.0, bow.z).normalize_or_zero()
}

/// The speed of the ship along `direction` after `seconds`.
fn speed_after(app: &mut App, ship: Entity, direction: Vec3, seconds: f32) -> f32 {
    run_seconds(app, seconds);
    velocity(app, ship).dot(direction)
}

#[test]
fn each_mast_gets_a_sail_and_its_controls() {
    for class in ShipClass::ALL {
        let (mut app, ship) = app_with_ship(class, Vec3::ZERO);
        let masts = class.layout().masts.len();
        assert_eq!(sails(&app, ship).0.len(), masts);
        let mut cranks = app
            .world_mut()
            .query_filtered::<&PartOf, With<HalyardCrank>>();
        let crank_count = cranks
            .iter(app.world())
            .filter(|part_of| part_of.0 == ship)
            .count();
        let mut levers = app
            .world_mut()
            .query_filtered::<&PartOf, With<SheetLever>>();
        let lever_count = levers
            .iter(app.world())
            .filter(|part_of| part_of.0 == ship)
            .count();
        assert_eq!((crank_count, lever_count), (masts, masts), "{class:?}");
    }
}

#[test]
fn turned_crank_hoists_sail() {
    let (mut app, ship) = app_with_ship(ShipClass::A, Vec3::ZERO);
    let start = sails(&app, ship).0[0].hoist;
    assert!(start < 0.01, "hoist at start {start}");
    app.insert_resource(CrankTorque(12.0));
    run_seconds(&mut app, 3.0);
    app.insert_resource(CrankTorque(0.0));
    run_seconds(&mut app, 1.0);
    let hoist = sails(&app, ship).0[0].hoist;
    assert!(hoist > 0.2, "hoist {hoist}");
}

#[test]
fn pushed_lever_turns_sail() {
    let (mut app, ship) = app_with_ship(ShipClass::A, Vec3::ZERO);
    app.insert_resource(SheetTorque(10.0, None));
    run_seconds(&mut app, 2.0);
    let angle = sails(&app, ship).0[0].angle;
    assert!(angle > MAX_SHEET_ANGLE * 0.9, "angle {angle}");
    app.insert_resource(SheetTorque(-10.0, None));
    run_seconds(&mut app, 2.0);
    let angle = sails(&app, ship).0[0].angle;
    assert!(angle < -MAX_SHEET_ANGLE * 0.9, "angle {angle}");
}

#[test]
fn square_sail_runs_downwind() {
    let wind = Vec3::NEG_Z * 4.0;
    let (mut app, ship) = app_with_ship(ShipClass::A, wind);
    set_cranks(&mut app, ship, HOIST_TURNS);
    let speed = speed_after(&mut app, ship, Vec3::NEG_Z, 20.0);
    println!("downwind speed {speed} m/s");
    assert!(speed > 1.5 && speed < 4.0, "speed {speed}");
}

#[test]
fn furled_sail_does_not_move_ship() {
    let wind = Vec3::NEG_Z * 4.0;
    let (mut app, ship) = app_with_ship(ShipClass::A, wind);
    let speed = speed_after(&mut app, ship, Vec3::NEG_Z, 20.0);
    assert!(speed.abs() < 0.2, "speed {speed}");
}

/// The sail stands 40° from square, about the best angle for a beam reach.
const SHEET_ANGLE_ON_REACH: f32 = 0.7;

#[test]
fn angled_sail_reaches_across_wind() {
    // The wind blows from starboard to port, square to the bow.
    let wind = Vec3::NEG_X * 4.0;
    let (mut app, ship) = app_with_ship(ShipClass::A, wind);
    set_cranks(&mut app, ship, HOIST_TURNS);
    app.insert_resource(SheetTorque(10.0, Some(SHEET_ANGLE_ON_REACH)));
    run_seconds(&mut app, 20.0);
    // The mean over a few seconds, because the ship rolls on the waves.
    let start = position(&app, ship);
    run_seconds(&mut app, 5.0);
    let velocity = (position(&app, ship) - start) / 5.0;
    let bow = bow(&app, ship);
    let forward = velocity.dot(bow);
    let leeway = velocity.dot(bow.cross(Vec3::Y)).abs();
    println!("reach: forward {forward} m/s, leeway {leeway} m/s");
    assert!(forward > 1.0, "forward {forward}");
    assert!(leeway < forward * 0.5, "leeway {leeway}");
}

#[test]
fn larger_ships_also_sail() {
    for class in [ShipClass::B, ShipClass::C] {
        let wind = Vec3::NEG_Z * 4.0;
        let (mut app, ship) = app_with_ship(class, wind);
        set_cranks(&mut app, ship, HOIST_TURNS);
        let speed = speed_after(&mut app, ship, Vec3::NEG_Z, 20.0);
        println!("{class:?} downwind speed {speed} m/s");
        assert!(speed > 1.5 && speed < 4.0, "{class:?}: speed {speed}");
    }
}
