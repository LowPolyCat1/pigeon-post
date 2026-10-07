use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::pigeon::{
    CAPSULE_LENGTH, Grounded, PigeonInput, PigeonPlugin, RADIUS, WALK_SPEED, pigeon_body,
};
use pigeon_post::start_island::main_island_shape;

/// A headless app with the collider of the main island and one pigeon above `start`.
fn app_with_island(start: Vec3) -> (App, Entity) {
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

    let collider = main_island_shape()
        .collider()
        .expect("the main island has a collider");
    app.world_mut()
        .spawn((RigidBody::Static, collider, Transform::default()));
    let pigeon = app
        .world_mut()
        .spawn((pigeon_body(), Transform::from_translation(start)))
        .id();
    (app, pigeon)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn position(app: &App, pigeon: Entity) -> Vec3 {
    app.world()
        .get::<Position>(pigeon)
        .expect("pigeon has a position")
        .0
}

fn is_grounded(app: &App, pigeon: Entity) -> bool {
    app.world()
        .get::<Grounded>(pigeon)
        .expect("pigeon has Grounded")
        .0
}

fn walk(app: &mut App, pigeon: Entity, movement: Vec2) {
    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .movement = movement;
}

/// The height of the capsule center above the grass when the pigeon stands.
const STAND_HEIGHT: f32 = CAPSULE_LENGTH / 2.0 + RADIUS;

#[test]
fn pigeon_lands_on_the_spawn_row() {
    let (mut app, pigeon) = app_with_island(Vec3::new(3.0, 2.0, 0.0));
    run_seconds(&mut app, 2.0);
    assert!(is_grounded(&app, pigeon));
    let y = position(&app, pigeon).y;
    assert!((y - STAND_HEIGHT).abs() < 0.05, "{y}");
}

#[test]
fn pigeon_walks_across_the_island() {
    let shape = main_island_shape();
    let (mut app, pigeon) = app_with_island(Vec3::new(0.0, 2.0, 0.0));
    run_seconds(&mut app, 1.0);
    // Toward -Z, out of the flat center onto the uneven grass near the rim.
    walk(&mut app, pigeon, Vec2::Y);
    let mut grounded_steps = 0;
    let steps = 90;
    for _ in 0..steps {
        app.update();
        let at = position(&app, pigeon);
        let ground = shape.top_height(at.x, at.z);
        assert!(at.y > ground + STAND_HEIGHT - 0.1, "sank at {at}");
        if is_grounded(&app, pigeon) {
            grounded_steps += 1;
        }
    }
    let at = position(&app, pigeon);
    assert!(at.z < -WALK_SPEED * 1.2, "{at}");
    assert!(grounded_steps > steps * 9 / 10, "{grounded_steps}");
}
