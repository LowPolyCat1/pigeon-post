use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::pigeon::{PigeonInput, PigeonPlugin, pigeon_body};

const DECK_SPEED: Vec3 = Vec3::new(3.0, 0.0, 0.0);

/// A headless app with a pigeon on a deck that moves at [`DECK_SPEED`]. A kinematic deck
/// moves at an exact speed, so the test does not depend on the buoyancy of the ship.
fn app_with_pigeon_on_deck() -> (App, Entity, Entity) {
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

    let deck = app
        .world_mut()
        .spawn((
            RigidBody::Kinematic,
            Collider::cuboid(40.0, 1.0, 40.0),
            Transform::from_xyz(0.0, -0.5, 0.0),
            LinearVelocity(DECK_SPEED),
        ))
        .id();
    let pigeon = app
        .world_mut()
        .spawn((pigeon_body(), Transform::from_xyz(0.0, 0.6, 0.0)))
        .id();
    (app, deck, pigeon)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn position(app: &App, entity: Entity) -> Vec3 {
    app.world()
        .get::<Position>(entity)
        .expect("entity has a position")
        .0
}

#[test]
fn standing_pigeon_rides_the_deck() {
    let (mut app, deck, pigeon) = app_with_pigeon_on_deck();
    run_seconds(&mut app, 3.0);

    let offset = position(&app, pigeon).x - position(&app, deck).x;
    assert!(
        offset.abs() < 0.5,
        "pigeon is {offset} m from the deck center"
    );
}

#[test]
fn walking_pigeon_moves_relative_to_the_deck() {
    let (mut app, deck, pigeon) = app_with_pigeon_on_deck();
    run_seconds(&mut app, 1.0);
    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .movement = Vec2::Y;
    run_seconds(&mut app, 1.0);

    let offset = position(&app, pigeon) - position(&app, deck);
    // Forward is -Z. Along X the pigeon keeps pace with the deck.
    assert!(offset.z < -3.0, "offset {offset}");
    assert!(offset.x.abs() < 0.5, "offset {offset}");
}

#[test]
fn jump_from_deck_lands_on_deck() {
    let (mut app, deck, pigeon) = app_with_pigeon_on_deck();
    run_seconds(&mut app, 1.0);
    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .jump = true;
    run_seconds(&mut app, 2.0);

    let offset = position(&app, pigeon).x - position(&app, deck).x;
    assert!(
        offset.abs() < 0.5,
        "pigeon is {offset} m from the deck center"
    );
}
