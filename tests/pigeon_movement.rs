use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::pigeon::{
    Grounded, JUMP_SPEED, PigeonInput, PigeonPlugin, WALK_SPEED, pigeon_body,
};

/// A headless app with a floor at y = 0 and one pigeon above it.
fn app_with_pigeon() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        PhysicsPlugins::default(),
        PigeonPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    app.finish();

    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(50.0, 1.0, 50.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    let pigeon = app
        .world_mut()
        .spawn((pigeon_body(), Transform::from_xyz(0.0, 1.0, 0.0)))
        .id();
    (app, pigeon)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn velocity(app: &App, pigeon: Entity) -> Vec3 {
    app.world()
        .get::<LinearVelocity>(pigeon)
        .expect("pigeon has a velocity")
        .0
}

fn is_grounded(app: &App, pigeon: Entity) -> bool {
    app.world()
        .get::<Grounded>(pigeon)
        .expect("pigeon has Grounded")
        .0
}

#[test]
fn pigeon_lands_on_floor() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 2.0);

    assert!(is_grounded(&app, pigeon));
    assert!(velocity(&app, pigeon).y.abs() < 0.1);
}

#[test]
fn pigeon_walks_forward_at_walk_speed() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 1.0);

    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .movement = Vec2::Y;
    run_seconds(&mut app, 1.0);

    let velocity = velocity(&app, pigeon);
    assert!((velocity.z + WALK_SPEED).abs() < 0.1, "velocity {velocity}");
    assert!(velocity.x.abs() < 0.1, "velocity {velocity}");
}

#[test]
fn pigeon_jumps_only_from_ground() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 1.0);

    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .jump = true;
    app.update();
    let after_jump = velocity(&app, pigeon).y;
    assert!(
        after_jump > JUMP_SPEED * 0.8,
        "vertical velocity {after_jump}"
    );

    run_seconds(&mut app, 0.2);
    assert!(!is_grounded(&app, pigeon));
    let before_second_press = velocity(&app, pigeon).y;
    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .jump = true;
    app.update();
    assert!(velocity(&app, pigeon).y < before_second_press);
}
