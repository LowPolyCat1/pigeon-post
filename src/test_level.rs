//! A flat island with a few steps to try the pigeon on, until the real world exists.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::pigeon::{CAPSULE_LENGTH, Pigeon, RADIUS, pigeon_body};

const CAMERA_OFFSET: Vec3 = Vec3::new(0.0, 4.0, 8.0);
const ISLAND_SIZE: Vec3 = Vec3::new(20.0, 1.0, 20.0);

pub struct TestLevelPlugin;

impl Plugin for TestLevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_level).add_systems(
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
        pigeon_body(),
        Mesh3d(meshes.add(Capsule3d::new(RADIUS, CAPSULE_LENGTH))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.57, 0.62))),
        Transform::from_xyz(0.0, 2.0, 0.0),
    ));

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

/// The offset is constant, so the camera keeps the rotation it spawned with.
fn follow_pigeon(
    pigeon: Single<&Transform, With<Pigeon>>,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<Pigeon>)>,
) {
    camera.translation = pigeon.translation + CAMERA_OFFSET;
}
