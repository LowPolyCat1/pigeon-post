use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::cloud_sea::SEA_LEVEL;
use pigeon_post::net::NetMode;
use pigeon_post::net::Owner;
use pigeon_post::pigeon::Pigeon;
use pigeon_post::ship::{Docked, Ship, ShipClass, ShipPlugin, ship_body};

/// A headless app with the ship plugin. The plugin needs replication, so the app has the
/// replicon plugins without a transport. As the authority, the plugin spawns its own ship at
/// the dock.
fn app() -> App {
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
    app
}

/// A test ship of `class` at `height` above the sea level, far from the ship at the dock.
fn app_with_ship_at(class: ShipClass, height: f32) -> (App, Entity) {
    let mut app = app();
    let ship = app
        .world_mut()
        .spawn((
            ship_body(class),
            Transform::from_xyz(-40.0, SEA_LEVEL + height, 0.0),
        ))
        .id();
    (app, ship)
}

/// The classes of all ships in the world.
fn ship_classes(app: &mut App) -> Vec<ShipClass> {
    let mut ships = app.world_mut().query_filtered::<&ShipClass, With<Ship>>();
    ships.iter(app.world()).copied().collect()
}

fn spawn_players(app: &mut App, count: usize) -> Vec<Entity> {
    (0..count)
        .map(|index| {
            app.world_mut()
                .spawn((Pigeon, Owner::Client(index as u64)))
                .id()
        })
        .collect()
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
    for class in ShipClass::ALL {
        let (mut app, ship) = app_with_ship_at(class, 3.0);
        run_seconds(&mut app, 15.0);

        // At the rest draft the center is a tenth of the hull height above the sea level.
        // Waves move it around that.
        let rest = class.hull_size().y * 0.1;
        let height = position(&app, ship).y - SEA_LEVEL;
        assert!(
            (height - rest).abs() < 0.8,
            "{class:?} height {height}, rest {rest}"
        );
        let tilt = tilt(&app, ship);
        assert!(tilt < 0.35, "{class:?} tilt {tilt}");
    }
}

#[test]
fn ship_stays_upright_for_a_while() {
    for class in ShipClass::ALL {
        let (mut app, ship) = app_with_ship_at(class, class.hull_size().y * 0.1);
        let mut worst = 0.0_f32;
        for _ in 0..(30 * 60) {
            app.update();
            worst = worst.max(tilt(&app, ship));
        }
        assert!(worst < 0.35, "{class:?} tilt {worst}");
    }
}

#[test]
fn sunken_ship_rises() {
    for class in ShipClass::ALL {
        let (mut app, ship) = app_with_ship_at(class, -class.hull_size().y * 2.0);
        run_seconds(&mut app, 10.0);

        let height = position(&app, ship).y - SEA_LEVEL;
        assert!(height > -0.6, "{class:?} height {height}");
    }
}

#[test]
fn docked_ship_follows_player_count() {
    let mut app = app();
    let players = spawn_players(&mut app, 2);
    app.update();
    assert_eq!(ship_classes(&mut app), [ShipClass::A]);

    spawn_players(&mut app, 3);
    app.update();
    app.update();
    assert_eq!(ship_classes(&mut app), [ShipClass::B]);

    spawn_players(&mut app, 2);
    app.update();
    app.update();
    assert_eq!(ship_classes(&mut app), [ShipClass::C]);

    for player in players {
        app.world_mut().despawn(player);
    }
    app.update();
    app.update();
    assert_eq!(ship_classes(&mut app), [ShipClass::B]);
}

#[test]
fn ship_at_sea_keeps_its_class() {
    let mut app = app();
    spawn_players(&mut app, 2);
    app.update();
    app.insert_resource(Docked(false));
    spawn_players(&mut app, 5);
    app.update();
    app.update();
    assert_eq!(ship_classes(&mut app), [ShipClass::A]);

    // The count does not change again, so the dock does not swap the ship either.
    app.insert_resource(Docked(true));
    app.update();
    assert_eq!(ship_classes(&mut app), [ShipClass::A]);
}

#[test]
fn waves_rock_the_ship() {
    let (mut app, ship) = app_with_ship_at(ShipClass::A, 0.2);
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
