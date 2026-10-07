//! A host app and a client app in one process. Replicon's test helpers move the messages
//! in memory, so no socket opens.

use std::net::SocketAddr;
use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use bevy_replicon::test_app::{ServerTestAppExt, TestClientEntity};
use pigeon_post::net::{DEFAULT_PORT, LocalPigeon, NetMode, NetPlugin, Owner};
use pigeon_post::pigeon::{InputMessage, Pigeon, PigeonInput, PigeonPlugin, WALK_SPEED};
use pigeon_post::props::{CRATE_SIZE, Crate, PropsPlugin, crate_body};

const CLIENT_ID: u64 = 42;

fn app(mode: NetMode) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        PhysicsPlugins::default(),
        // A tick on each update, so each update replicates.
        RepliconPlugins.set(ServerPlugin::new(PostUpdate)),
        PigeonPlugin,
        NetPlugin,
        PropsPlugin,
    ))
    .insert_resource(mode)
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    app.finish();

    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(50.0, 1.0, 50.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    app
}

/// A host with its own pigeon and one connected client.
fn host_and_client() -> (App, App) {
    let mut host = app(NetMode::Host { port: DEFAULT_PORT });
    let mut client = app(NetMode::Client {
        server: SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
        client_id: CLIENT_ID,
    });
    host.connect_client(&mut client);
    // The renet backend inserts this id. The test helpers do not.
    let client_entity = **client.world().resource::<TestClientEntity>();
    host.world_mut()
        .entity_mut(client_entity)
        .insert(NetworkId::new(CLIENT_ID));
    run_frames(&mut host, &mut client, 5);
    (host, client)
}

fn run_frames(host: &mut App, client: &mut App, frames: usize) {
    for _ in 0..frames {
        client.update();
        host.exchange_with_client(client);
        host.update();
        host.exchange_with_client(client);
    }
}

fn pigeon_of(app: &mut App, owner: Owner) -> Entity {
    let mut pigeons = app.world_mut().query::<(Entity, &Owner)>();
    pigeons
        .iter(app.world())
        .find(|(_, other)| **other == owner)
        .map(|(entity, _)| entity)
        .expect("the pigeon of the owner exists")
}

fn pigeon_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<Pigeon>>()
        .iter(app.world())
        .count()
}

#[test]
fn client_receives_both_pigeons_and_marks_its_own() {
    let (mut host, mut client) = host_and_client();

    assert_eq!(pigeon_count(&mut host), 2);
    assert_eq!(pigeon_count(&mut client), 2);

    let mut local = client
        .world_mut()
        .query_filtered::<&Owner, With<LocalPigeon>>();
    let owners: Vec<_> = local.iter(client.world()).copied().collect();
    assert_eq!(owners, [Owner::Client(CLIENT_ID)]);

    let mut local = host
        .world_mut()
        .query_filtered::<&Owner, With<LocalPigeon>>();
    let owners: Vec<_> = local.iter(host.world()).copied().collect();
    assert_eq!(owners, [Owner::Host]);
}

#[test]
fn client_input_moves_only_its_pigeon() {
    let (mut host, mut client) = host_and_client();
    // Let both pigeons land.
    run_frames(&mut host, &mut client, 60);
    let client_pigeon = pigeon_of(&mut client, Owner::Client(CLIENT_ID));
    let start = client
        .world()
        .get::<Transform>(client_pigeon)
        .expect("pigeon has a transform")
        .translation;

    for _ in 0..60 {
        client.world_mut().write_message(InputMessage(PigeonInput {
            movement: Vec2::Y,
            ..default()
        }));
        run_frames(&mut host, &mut client, 1);
    }

    let moved = pigeon_of(&mut host, Owner::Client(CLIENT_ID));
    let velocity = host
        .world()
        .get::<LinearVelocity>(moved)
        .expect("pigeon has a velocity")
        .0;
    assert!((velocity.z + WALK_SPEED).abs() < 0.1, "velocity {velocity}");

    let still = pigeon_of(&mut host, Owner::Host);
    let velocity = host
        .world()
        .get::<LinearVelocity>(still)
        .expect("pigeon has a velocity")
        .0;
    assert!(velocity.length() < 0.1, "velocity {velocity}");

    // Forward is -Z. The client sees the motion that the host replicates.
    let end = client
        .world()
        .get::<Transform>(client_pigeon)
        .expect("pigeon has a transform")
        .translation;
    assert!(end.z < start.z - 1.0, "start {start}, end {end}");
}

#[test]
fn disconnect_removes_client_pigeon() {
    let (mut host, mut client) = host_and_client();

    host.disconnect_client(&mut client);
    host.update();

    assert_eq!(pigeon_count(&mut host), 1);
    pigeon_of(&mut host, Owner::Host);
}

fn crate_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<Crate>>()
        .iter(app.world())
        .count()
}

#[test]
fn client_receives_crates_without_physics() {
    let (mut host, mut client) = host_and_client();

    assert!(crate_count(&mut host) > 0);
    assert_eq!(crate_count(&mut client), crate_count(&mut host));
    let mut bodies = client
        .world_mut()
        .query_filtered::<(), (With<Crate>, With<RigidBody>)>();
    assert_eq!(bodies.iter(client.world()).count(), 0);
}

#[test]
fn client_pigeon_pushes_crate_and_client_sees_it() {
    let (mut host, mut client) = host_and_client();
    let pigeon = pigeon_of(&mut host, Owner::Client(CLIENT_ID));
    let pigeon_x = host
        .world()
        .get::<Transform>(pigeon)
        .expect("pigeon has a transform")
        .translation
        .x;
    // In the walk path of the client pigeon, which walks toward -Z.
    let pushed = host
        .world_mut()
        .spawn((
            crate_body(),
            Transform::from_xyz(pigeon_x, CRATE_SIZE / 2.0, -1.5),
        ))
        .id();
    run_frames(&mut host, &mut client, 60);
    let start = host
        .world()
        .get::<Transform>(pushed)
        .expect("crate has a transform")
        .translation;

    for _ in 0..90 {
        client.world_mut().write_message(InputMessage(PigeonInput {
            movement: Vec2::Y,
            ..default()
        }));
        run_frames(&mut host, &mut client, 1);
    }

    let end = host
        .world()
        .get::<Transform>(pushed)
        .expect("crate has a transform")
        .translation;
    assert!(end.z < start.z - 1.0, "start {start}, end {end}");

    let mut crates = client
        .world_mut()
        .query_filtered::<&Transform, With<Crate>>();
    let seen = crates
        .iter(client.world())
        .any(|transform| transform.translation.distance(end) < 0.1);
    assert!(seen, "the client has no crate near {end}");
}
