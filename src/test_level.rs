//! The meshes of the replicated entities and the camera. `start_island` builds the scene,
//! and `first_person` moves the camera.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::controls::{
    CRANK_GRIP_LENGTH, Control, HANDLE_THICKNESS, SPOKE_OVERHANG, SPOKE_THICKNESS,
};
use crate::helm::RudderAngle;
use crate::pigeon::{CAPSULE_LENGTH, Pigeon, RADIUS};
use crate::props::{CRATE_SIZE, Crate};
use crate::sails::mast_height;
use crate::ship::{Mount, RAIL_HEIGHT, ShipClass};

pub struct TestLevelPlugin;

impl Plugin for TestLevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, turn_rudder_blades)
            .add_observer(add_pigeon_mesh)
            .add_observer(add_crate_mesh)
            .add_observer(add_ship_mesh)
            .add_observer(add_control_mesh);
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

/// The shapes of a control match its collider in [`crate::controls`]. The hinge axis is the
/// local Z axis.
fn add_control_mesh(
    add: On<Add, Control>,
    controls: Query<&Control>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(&control) = controls.get(add.entity) else {
        return;
    };
    let wood = materials.add(Color::srgb(0.55, 0.35, 0.15));
    let brass = materials.add(Color::srgb(0.85, 0.65, 0.2));
    let mut parts: Vec<(Mesh, Handle<StandardMaterial>, Transform)> = Vec::new();
    match control {
        Control::Lever { length } => {
            parts.push((
                Cuboid::new(HANDLE_THICKNESS, length, HANDLE_THICKNESS).into(),
                brass.clone(),
                Transform::from_translation(Vec3::Y * length / 2.0),
            ));
            parts.push((
                Sphere::new(HANDLE_THICKNESS).into(),
                wood.clone(),
                Transform::from_translation(Vec3::Y * length),
            ));
        }
        Control::Wheel { radius } => {
            parts.push((
                Mesh::from(Torus::new(radius - 0.04, radius + 0.04))
                    .rotated_by(Quat::from_rotation_x(FRAC_PI_2)),
                wood.clone(),
                Transform::default(),
            ));
            for index in 0..4 {
                parts.push((
                    Cuboid::new(
                        2.0 * (radius + SPOKE_OVERHANG),
                        SPOKE_THICKNESS * 0.6,
                        SPOKE_THICKNESS * 0.6,
                    )
                    .into(),
                    wood.clone(),
                    Transform::from_rotation(Quat::from_rotation_z(
                        index as f32 * std::f32::consts::FRAC_PI_4,
                    )),
                ));
            }
            parts.push((
                Mesh::from(Cylinder::new(0.1, 0.15)).rotated_by(Quat::from_rotation_x(FRAC_PI_2)),
                brass.clone(),
                Transform::default(),
            ));
        }
        Control::Crank { radius } => {
            parts.push((
                Cuboid::new(HANDLE_THICKNESS, radius, HANDLE_THICKNESS).into(),
                brass.clone(),
                Transform::from_translation(Vec3::Y * radius / 2.0),
            ));
            parts.push((
                Mesh::from(Cylinder::new(SPOKE_THICKNESS / 2.0, CRANK_GRIP_LENGTH))
                    .rotated_by(Quat::from_rotation_x(FRAC_PI_2)),
                wood.clone(),
                Transform::from_xyz(0.0, radius, CRANK_GRIP_LENGTH / 2.0),
            ));
        }
    }
    commands
        .entity(add.entity)
        .insert(Visibility::default())
        .with_children(|control| {
            for (mesh, material, transform) in parts {
                control.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    transform,
                ));
            }
        });
}

/// The hull and the rails of [`crate::ship::ship_collider`], the rigging of a skyship, and a
/// placeholder at each mount of the [`crate::ship::ShipLayout`]. The class arrives with the
/// ship on a client too, so each instance builds the right ship.
fn add_ship_mesh(
    add: On<Add, ShipClass>,
    classes: Query<&ShipClass>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(&class) = classes.get(add.entity) else {
        return;
    };
    let layout = class.layout();
    let size = layout.hull;
    let half = size / 2.0;
    let deck = half.y;
    let hull = materials.add(Color::srgb(0.45, 0.3, 0.2));
    let rail = materials.add(Color::srgb(0.75, 0.6, 0.4));
    let stripe = materials.add(Color::srgb(0.85, 0.2, 0.15));

    // Only pictures: the collider of the ship is the hull and the rails. The parts below have
    // no mass, so the buoyancy of the hull stays as tuned.
    let mut parts = vec![
        // A red stripe along the hull, the color of the Pigeon Postal Service.
        (
            meshes.add(Cuboid::new(size.x + 0.04, 0.3, size.z + 0.04)),
            stripe.clone(),
            Transform::from_xyz(0.0, deck - 0.45, 0.0),
        ),
    ];
    let rail_y = deck + RAIL_HEIGHT / 2.0;
    let side = meshes.add(Cuboid::new(0.15, RAIL_HEIGHT, size.z));
    let end = meshes.add(Cuboid::new(size.x, RAIL_HEIGHT, 0.15));
    for (mesh, x, z) in [
        (side.clone(), -half.x, 0.0),
        (side, half.x, 0.0),
        (end.clone(), 0.0, -half.z),
        (end, 0.0, half.z),
    ] {
        parts.push((mesh, rail.clone(), Transform::from_xyz(x, rail_y, z)));
    }

    // The main mast is the tallest. `crate::sails` draws the sails.
    for (index, mast) in layout.masts.iter().enumerate() {
        let height = mast_height(&layout, index);
        let foot = mast.translation;
        parts.push((
            meshes.add(Cylinder::new(0.15, height).mesh().resolution(8)),
            rail.clone(),
            Transform::from_translation(foot + Vec3::Y * height / 2.0),
        ));
    }
    let nest = layout.crows_nest.translation;
    parts.push((
        meshes.add(Cylinder::new(0.7, 0.6).mesh().resolution(10)),
        rail.clone(),
        Transform::from_translation(nest),
    ));
    parts.push((
        meshes.add(Cuboid::new(0.05, 0.5, 0.9)),
        stripe.clone(),
        Transform::from_translation(nest + Vec3::new(0.0, 0.85, 0.45)),
    ));

    // A cabin at the stern around the map table. It is open toward the bow, so the table
    // shows.
    let cabin = layout.cabin;
    let table = layout.map_table.translation;
    for (mesh, offset) in [
        (
            Cuboid::new(cabin.x, cabin.y, 0.15),
            Vec3::new(0.0, cabin.y / 2.0, cabin.z / 2.0),
        ),
        (
            Cuboid::new(0.15, cabin.y, cabin.z),
            Vec3::new(-cabin.x / 2.0, cabin.y / 2.0, 0.0),
        ),
        (
            Cuboid::new(0.15, cabin.y, cabin.z),
            Vec3::new(cabin.x / 2.0, cabin.y / 2.0, 0.0),
        ),
    ] {
        parts.push((
            meshes.add(mesh),
            hull.clone(),
            Transform::from_translation(table + offset),
        ));
    }
    parts.push((
        meshes.add(Cuboid::new(cabin.x + 0.3, 0.15, cabin.z + 0.3)),
        stripe.clone(),
        Transform::from_translation(table + Vec3::Y * (cabin.y + 0.07)),
    ));

    // The bowsprit points forward, along -Z.
    let bowsprit = 0.3 * size.z;
    parts.push((
        meshes.add(Cylinder::new(0.1, bowsprit).mesh().resolution(6)),
        rail.clone(),
        Transform::from_xyz(0.0, deck + 0.3, -half.z - bowsprit / 3.0)
            .with_rotation(Quat::from_rotation_x(-1.2)),
    ));

    for (mount, transform) in layout.mounts() {
        if let Some((mesh, color, offset)) = mount_marker(mount) {
            parts.push((
                meshes.add(mesh),
                materials.add(color),
                transform * Transform::from_translation(offset),
            ));
        }
    }

    // The rudder hangs aft of the propeller, below the deck. The blade reaches aft of its hinge, so
    // a positive angle swings the trailing edge to starboard.
    let blade = (
        meshes.add(Cuboid::new(0.12, size.y * 0.8, 0.9)),
        rail.clone(),
    );

    commands
        .entity(add.entity)
        .insert((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(hull),
        ))
        .with_children(|ship| {
            for (mesh, material, transform) in parts {
                ship.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform));
            }
            ship.spawn((
                RudderBlade,
                Transform::from_xyz(0.0, -0.1 * size.y, half.z + 0.25),
                Visibility::default(),
            ))
            .with_child((
                Mesh3d(blade.0),
                MeshMaterial3d(blade.1),
                Transform::from_xyz(0.0, 0.0, 0.45),
            ));
        });
}

/// The hinge of the rudder blade, a child of the ship.
#[derive(Component, Debug)]
struct RudderBlade;

fn turn_rudder_blades(
    ships: Query<&RudderAngle>,
    mut blades: Query<(&ChildOf, &mut Transform), With<RudderBlade>>,
) {
    for (child_of, mut transform) in &mut blades {
        if let Ok(rudder) = ships.get(child_of.parent()) {
            transform.rotation = Quat::from_rotation_y(rudder.0);
        }
    }
}

/// A placeholder shape for the part at a mount, until the part has a model. The offset
/// lifts the shape onto the mount surface. The masts and the crow's nest have a full mesh.
fn mount_marker(mount: Mount) -> Option<(Mesh, Color, Vec3)> {
    let brass = Color::srgb(0.85, 0.65, 0.2);
    let iron = Color::srgb(0.2, 0.2, 0.22);
    let facing_z = Quat::from_rotation_x(FRAC_PI_2);
    let marker = match mount {
        // The helm is a control with its own mesh.
        Mount::Mast | Mount::CrowsNest | Mount::Helm => return None,
        Mount::Furnace => (Cuboid::new(1.0, 1.2, 1.0).into(), iron, Vec3::Y * 0.6),
        Mount::CoalBunker => (
            Cuboid::new(0.9, 0.7, 0.9).into(),
            Color::srgb(0.1, 0.1, 0.1),
            Vec3::Y * 0.35,
        ),
        Mount::ThrottleLever => (
            Cuboid::new(0.1, 0.8, 0.1).into(),
            Color::srgb(0.95, 0.8, 0.1),
            Vec3::Y * 0.4,
        ),
        Mount::IgnitionLever => (
            Cuboid::new(0.1, 0.8, 0.1).into(),
            Color::srgb(0.9, 0.1, 0.1),
            Vec3::Y * 0.4,
        ),
        Mount::Propeller => (
            Mesh::from(Cylinder::new(0.6, 0.1)).rotated_by(facing_z),
            iron,
            Vec3::Z * 0.1,
        ),
        Mount::AnchorWinch => (
            Mesh::from(Cylinder::new(0.3, 1.0)).rotated_by(Quat::from_rotation_z(FRAC_PI_2)),
            iron,
            Vec3::Y * 0.4,
        ),
        Mount::Bell => (Sphere::new(0.25).into(), brass, Vec3::Y * 1.2),
        Mount::SpeakingTube => (Cylinder::new(0.08, 1.0).into(), brass, Vec3::Y * 0.5),
        Mount::CargoHook => (
            Sphere::new(0.12).into(),
            Color::srgb(0.6, 0.6, 0.65),
            Vec3::Y * 0.12,
        ),
        Mount::MapTable => (
            Cuboid::new(1.2, 0.8, 0.8).into(),
            Color::srgb(0.2, 0.5, 0.3),
            Vec3::Y * 0.4,
        ),
    };
    Some(marker)
}
