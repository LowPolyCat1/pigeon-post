use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::cloud_sea::{CloudSeaPlugin, SEA_LEVEL, cloud_sea_depth};
use pigeon_post::pigeon::{PigeonPlugin, pigeon_body};
use pigeon_post::stamina::Stamina;

/// A headless app without a floor and one pigeon at `height`.
fn app_with_pigeon_at(height: f32) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        PhysicsPlugins::default(),
        PigeonPlugin,
        CloudSeaPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    app.finish();

    let pigeon = app
        .world_mut()
        .spawn((pigeon_body(), Transform::from_xyz(0.0, height, 0.0)))
        .id();
    (app, pigeon)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn depth(app: &App, pigeon: Entity) -> f32 {
    let position = app
        .world()
        .get::<Position>(pigeon)
        .expect("pigeon has a position");
    cloud_sea_depth(position.0)
}

#[test]
fn falling_pigeon_floats_at_surface() {
    let (mut app, pigeon) = app_with_pigeon_at(SEA_LEVEL + 5.0);
    run_seconds(&mut app, 5.0);

    let depth = depth(&app, pigeon);
    assert!((0.0..1.0).contains(&depth), "depth {depth}");
    let velocity = app
        .world()
        .get::<LinearVelocity>(pigeon)
        .expect("pigeon has a velocity");
    assert!(velocity.y.abs() < 0.5, "velocity {}", velocity.0);
}

#[test]
fn sunken_pigeon_rises() {
    let (mut app, pigeon) = app_with_pigeon_at(SEA_LEVEL - 3.0);
    run_seconds(&mut app, 3.0);

    assert!(depth(&app, pigeon) < 1.0);
}

#[test]
fn swimming_refills_stamina() {
    let (mut app, pigeon) = app_with_pigeon_at(SEA_LEVEL - 0.25);
    app.world_mut().entity_mut(pigeon).insert(Stamina::new(0.0));
    run_seconds(&mut app, 1.0);

    let stamina = app
        .world()
        .get::<Stamina>(pigeon)
        .expect("pigeon has stamina")
        .value();
    assert!((0.4..=0.6).contains(&stamina), "stamina {stamina}");
}
