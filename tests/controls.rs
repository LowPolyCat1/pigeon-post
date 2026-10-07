use std::time::Duration;

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::controls::{
    ControlValue, ControlsPlugin, CrankSpec, LeverSpec, PartOf, spawn_crank, spawn_lever,
};

/// Pushes the body at a point given in the frame of the body, while the push is on.
#[derive(Component, Clone, Copy)]
struct Push {
    point: Vec3,
    force: Vec3,
    on: bool,
}

fn push(mut bodies: Query<(Forces, &Push)>) {
    for (mut forces, push) in &mut bodies {
        if !push.on {
            continue;
        }
        let rotation = forces.rotation().0;
        let point = forces.position().0 + rotation * push.point;
        forces.apply_force_at_point(rotation * push.force, point);
    }
}

/// A headless app with a heavy dynamic base in zero gravity. The base stands in for a ship.
fn app_with_base() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        bevy::state::app::StatesPlugin,
        bevy_replicon::RepliconPlugins,
        PhysicsPlugins::default(),
        ControlsPlugin,
    ))
    .insert_resource(Gravity(Vec3::ZERO))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )))
    .add_systems(FixedUpdate, push);
    app.finish();
    let base = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(4.0, 1.0, 4.0),
            ColliderDensity(50.0),
            Transform::from_xyz(3.0, 1.0, -2.0).with_rotation(Quat::from_rotation_y(0.7)),
        ))
        .id();
    (app, base)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn value(app: &App, control: Entity) -> f32 {
    app.world()
        .get::<ControlValue>(control)
        .expect("control has a value")
        .0
}

fn set_push(app: &mut App, control: Entity, on: bool) {
    app.world_mut()
        .get_mut::<Push>(control)
        .expect("control has a push")
        .on = on;
}

/// A lever on the deck of the base. The handle points up, and the hinge axis is along X.
fn spawn_test_lever(app: &mut App, base: Entity) -> Entity {
    let hinge = Transform::from_xyz(0.0, 0.5, 0.0)
        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    let lever = app
        .world_mut()
        .run_system_once(move |mut commands: Commands| {
            spawn_lever(&mut commands, base, hinge, LeverSpec::default())
        })
        .expect("lever spawns");
    app.world_mut().entity_mut(lever).insert(Push {
        point: Vec3::Y * 0.75,
        // A quarter of the pull of one wing, sideways on the handle.
        force: Vec3::X * 10.0,
        on: false,
    });
    lever
}

#[test]
fn pushed_lever_moves_and_stays() {
    let (mut app, base) = app_with_base();
    let lever = spawn_test_lever(&mut app, base);
    run_seconds(&mut app, 0.5);
    let start = value(&app, lever);
    assert!((start - 0.5).abs() < 0.02, "lever starts at {start}");

    set_push(&mut app, lever, true);
    run_seconds(&mut app, 0.15);
    set_push(&mut app, lever, false);
    run_seconds(&mut app, 0.3);
    let moved = value(&app, lever);
    assert!((moved - start).abs() > 0.15, "lever moved only to {moved}");

    run_seconds(&mut app, 3.0);
    let rest = value(&app, lever);
    assert!(
        (rest - moved).abs() < 0.02,
        "lever drifted from {moved} to {rest}"
    );
}

#[test]
fn lever_stops_at_its_end() {
    let (mut app, base) = app_with_base();
    let lever = spawn_test_lever(&mut app, base);
    run_seconds(&mut app, 0.5);
    set_push(&mut app, lever, true);
    run_seconds(&mut app, 3.0);
    let end = value(&app, lever);
    assert!(end < 0.02 || end > 0.98, "lever at {end}");
}

#[test]
fn turned_crank_counts_turns() {
    let (mut app, base) = app_with_base();
    let crank = app
        .world_mut()
        .run_system_once(move |mut commands: Commands| {
            spawn_crank(
                &mut commands,
                base,
                Transform::from_xyz(0.0, 0.5, 1.0),
                CrankSpec::default(),
            )
        })
        .expect("crank spawns");
    app.world_mut().entity_mut(crank).insert(Push {
        point: Vec3::Y * 0.35,
        force: Vec3::NEG_X * 15.0,
        on: false,
    });
    run_seconds(&mut app, 0.5);
    set_push(&mut app, crank, true);
    run_seconds(&mut app, 2.0);
    set_push(&mut app, crank, false);
    run_seconds(&mut app, 1.0);

    // The push stays on the arm, so the crank turns positively about its hinge axis.
    let turns = value(&app, crank);
    assert!(turns > 1.0, "crank turned only {turns} times");
}

#[test]
fn despawned_body_takes_its_parts() {
    let (mut app, base) = app_with_base();
    spawn_test_lever(&mut app, base);
    run_seconds(&mut app, 0.2);
    let mut parts = app.world_mut().query::<&PartOf>();
    // The lever and its joint.
    assert_eq!(parts.iter(app.world()).count(), 2);

    app.world_mut().despawn(base);
    app.update();
    assert_eq!(parts.iter(app.world()).count(), 0);
}
