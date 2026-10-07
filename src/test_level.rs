//! The meshes of the replicated entities and the camera. `start_island` builds the scene,
//! and `first_person` moves the camera.

use bevy::prelude::*;

use crate::pigeon::{CAPSULE_LENGTH, Pigeon, RADIUS};
use crate::props::{CRATE_SIZE, Crate};
use crate::ship::{HULL_SIZE, RAIL_HEIGHT, Ship};

pub struct TestLevelPlugin;

impl Plugin for TestLevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_observer(add_pigeon_mesh)
            .add_observer(add_crate_mesh)
            .add_observer(add_ship_mesh);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::default()));
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

/// The hull and the rails of [`crate::ship::ship_collider`], and the rigging of a skyship.
fn add_ship_mesh(
    add: On<Add, Ship>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let hull = materials.add(Color::srgb(0.45, 0.3, 0.2));
    let rail = materials.add(Color::srgb(0.75, 0.6, 0.4));
    let stripe = materials.add(Color::srgb(0.85, 0.2, 0.15));
    let canvas = materials.add(Color::srgb(0.97, 0.94, 0.84));
    let half = HULL_SIZE / 2.0;
    let rail_y = half.y + RAIL_HEIGHT / 2.0;
    let side = meshes.add(Cuboid::new(0.15, RAIL_HEIGHT, HULL_SIZE.z));
    let end = meshes.add(Cuboid::new(HULL_SIZE.x, RAIL_HEIGHT, 0.15));

    // Only pictures: the collider of the ship is the hull and the rails. The parts below have
    // no mass, so the buoyancy of the hull stays as tuned.
    let mast_height = 7.0;
    let mast_foot = half.y;
    let parts = [
        // A red stripe along the hull, the color of the Pigeon Postal Service.
        (
            meshes.add(Cuboid::new(HULL_SIZE.x + 0.04, 0.3, HULL_SIZE.z + 0.04)),
            stripe.clone(),
            Transform::from_xyz(0.0, half.y - 0.45, 0.0),
        ),
        (
            meshes.add(Cylinder::new(0.15, mast_height).mesh().resolution(8)),
            rail.clone(),
            Transform::from_xyz(0.0, mast_foot + mast_height / 2.0, -0.5),
        ),
        (
            meshes.add(Cuboid::new(3.2, 3.6, 0.08)),
            canvas,
            Transform::from_xyz(0.0, mast_foot + 3.6, -0.3),
        ),
        // The crow's nest at the top of the mast.
        (
            meshes.add(Cylinder::new(0.7, 0.6).mesh().resolution(10)),
            rail.clone(),
            Transform::from_xyz(0.0, mast_foot + mast_height - 0.6, -0.5),
        ),
        (
            meshes.add(Cuboid::new(0.05, 0.5, 0.9)),
            stripe.clone(),
            Transform::from_xyz(0.0, mast_foot + mast_height + 0.25, -0.05),
        ),
        // A cabin at the stern for the map table.
        (
            meshes.add(Cuboid::new(2.6, 1.4, 2.0)),
            hull.clone(),
            Transform::from_xyz(0.0, mast_foot + 0.7, half.z - 1.3),
        ),
        (
            meshes.add(Cuboid::new(2.9, 0.15, 2.3)),
            stripe,
            Transform::from_xyz(0.0, mast_foot + 1.47, half.z - 1.3),
        ),
        // The bowsprit points forward, along -Z.
        (
            meshes.add(Cylinder::new(0.1, 3.0).mesh().resolution(6)),
            rail.clone(),
            Transform::from_xyz(0.0, mast_foot + 0.3, -half.z - 1.0)
                .with_rotation(Quat::from_rotation_x(-1.2)),
        ),
    ];

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
            for (mesh, material, transform) in parts {
                ship.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform));
            }
        });
}
