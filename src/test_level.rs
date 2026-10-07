//! A flat island with a few steps to try the pigeon on, until the real world exists.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::net::LocalPigeon;
use crate::pigeon::{CAPSULE_LENGTH, Pigeon, RADIUS};
use crate::props::{CRATE_SIZE, Crate};
use crate::ship::{HULL_SIZE, RAIL_HEIGHT, Ship};

const CAMERA_OFFSET: Vec3 = Vec3::new(0.0, 4.0, 8.0);
/// Deep enough to reach into the cloud sea, so the ship cannot drift under the island.
const ISLAND_SIZE: Vec3 = Vec3::new(20.0, 6.0, 20.0);

pub struct TestLevelPlugin;

impl Plugin for TestLevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_level)
            .add_observer(add_pigeon_mesh)
            .add_observer(add_crate_mesh)
            .add_observer(add_ship_mesh)
            .add_systems(
                PostUpdate,
                follow_pigeon.before(TransformSystems::Propagate),
            );
    }
}

fn spawn_level(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let grass = materials.add(Color::srgb(0.45, 0.62, 0.35));
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(ISLAND_SIZE.x, ISLAND_SIZE.y, ISLAND_SIZE.z),
        Mesh3d(meshes.add(Cuboid::from_size(ISLAND_SIZE))),
        MeshMaterial3d(grass.clone()),
        Transform::from_xyz(0.0, -ISLAND_SIZE.y / 2.0, 0.0),
    ));

    let wood = materials.add(Color::srgb(0.6, 0.45, 0.3));
    for (index, height) in [0.4, 0.8, 1.2].into_iter().enumerate() {
        let size = Vec3::new(2.0, height, 2.0);
        commands.spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(wood.clone()),
            Transform::from_xyz(-4.0 + index as f32 * 2.0, height / 2.0, -5.0),
        ));
    }

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(CAMERA_OFFSET).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// The authority spawns the pigeons, and a client receives them. Each instance adds the mesh.
fn add_pigeon_mesh(
    add: On<Add, Pigeon>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.entity(add.entity).insert((
        Mesh3d(meshes.add(Capsule3d::new(RADIUS, CAPSULE_LENGTH))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.57, 0.62))),
    ));
}

fn add_crate_mesh(
    add: On<Add, Crate>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.entity(add.entity).insert((
        Mesh3d(meshes.add(Cuboid::from_length(CRATE_SIZE))),
        MeshMaterial3d(materials.add(Color::srgb(0.7, 0.5, 0.25))),
    ));
}

/// The hull and the rails of [`crate::ship::ship_collider`], as one mesh each.
fn add_ship_mesh(
    add: On<Add, Ship>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let hull = materials.add(Color::srgb(0.45, 0.3, 0.2));
    let rail = materials.add(Color::srgb(0.75, 0.6, 0.4));
    let half = HULL_SIZE / 2.0;
    let rail_y = half.y + RAIL_HEIGHT / 2.0;
    let side = meshes.add(Cuboid::new(0.15, RAIL_HEIGHT, HULL_SIZE.z));
    let end = meshes.add(Cuboid::new(HULL_SIZE.x, RAIL_HEIGHT, 0.15));
    commands
        .entity(add.entity)
        .insert((
            Mesh3d(meshes.add(Cuboid::from_size(HULL_SIZE))),
            MeshMaterial3d(hull),
        ))
        .with_children(|ship| {
            for (mesh, x, z) in [
                (side.clone(), -half.x, 0.0),
                (side.clone(), half.x, 0.0),
                (end.clone(), 0.0, -half.z),
                (end.clone(), 0.0, half.z),
            ] {
                ship.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(rail.clone()),
                    Transform::from_xyz(x, rail_y, z),
                ));
            }
        });
}

/// The offset is constant, so the camera keeps the rotation it spawned with.
fn follow_pigeon(
    pigeon: Single<&Transform, With<LocalPigeon>>,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<LocalPigeon>)>,
) {
    camera.translation = pigeon.translation + CAMERA_OFFSET;
}
