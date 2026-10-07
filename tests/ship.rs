use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::cloud_sea::SEA_LEVEL;
use pigeon_post::net::NetMode;
use pigeon_post::ship::{HULL_SIZE, ShipPlugin, ship_body};

/// A headless app with a test ship at `height` above the sea level. The plugin of the ship needs
/// replication, so the app has the replicon plugins without a transport. As the authority, the
/// plugin also spawns its own ship at `SHIP_SPAWN`. The test ship is far from it.
fn app_with_ship_at(height: f32) -> (App, Entity) {
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
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    app.init_resource::<NetMode>();
    app.finish();

    let ship = app
        .world_mut()
        .spawn((
            ship_body(),
            Transform::from_xyz(-20.0, SEA_LEVEL + height, 0.0),
        ))
        .id();
    (app, ship)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn position(app: &App, ship: Entity) -> Vec3 {
    app.world()
        .get::<Position>(ship)
        .expect("ship has a position")
        .0
}

fn tilt(app: &App, ship: Entity) -> f32 {
    let rotation = app
        .world()
        .get::<Rotation>(ship)
        .expect("ship has a rotation")
        .0;
    (rotation * Vec3::Y).angle_between(Vec3::Y)
}

#[test]
fn dropped_ship_floats_near_rest_draft() {
    let (mut app, ship) = app_with_ship_at(3.0);
    run_seconds(&mut app, 15.0);

    // At the rest draft the center is 0.2 m above the sea level. Waves move it around that.
    let height = position(&app, ship).y - SEA_LEVEL;
    assert!((-0.6..1.0).contains(&height), "height {height}");
    assert!(tilt(&app, ship) < 0.35, "tilt {}", tilt(&app, ship));
}

#[test]
fn sunken_ship_rises() {
    let (mut app, ship) = app_with_ship_at(-HULL_SIZE.y * 2.0);
    run_seconds(&mut app, 10.0);

    let height = position(&app, ship).y - SEA_LEVEL;
    assert!(height > -0.6, "height {height}");
}

#[test]
fn waves_rock_the_ship() {
    let (mut app, ship) = app_with_ship_at(0.2);
    run_seconds(&mut app, 5.0);

    let mut lowest = f32::MAX;
    let mut highest = f32::MIN;
    for _ in 0..300 {
        app.update();
        let height = position(&app, ship).y;
        lowest = lowest.min(height);
        highest = highest.max(height);
    }
    assert!(highest - lowest > 0.05, "range {}", highest - lowest);
}
