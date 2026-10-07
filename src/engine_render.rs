//! The meshes of the engine: the furnace and its fire, the coal bunker, the coal lumps, the
//! hatch plate, the lever knobs and the propellers. Each instance draws them from the
//! replicated [`Engine`] state, so a client sees the fire and the spinning blades too.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;

use crate::engine::{
    CHIMNEY_HEIGHT, CHIMNEY_RADIUS, COAL_SIZE, Coal, Engine, EnginePart, FURNACE_SIZE,
    HATCH_HEIGHT, HATCH_THICKNESS, HATCH_WIDTH, bunker_boxes, chimney_center, fire_chamber,
    furnace_boxes,
};
use crate::ship::ShipClass;

/// The blades turn this many times per second at full thrust.
const FULL_SPIN: f32 = 3.0;
const BLADE_LENGTH: f32 = 0.65;
/// The fire glows this bright at idle, and brighter with the throttle.
const GLOW_IDLE: f32 = 0.8;
const GLOW_FULL: f32 = 3.0;

pub struct EngineRenderPlugin;

impl Plugin for EngineRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_engine_mesh)
            .add_observer(add_coal_mesh)
            .add_observer(add_engine_part_mesh)
            .add_systems(Update, (spin_propellers, glow_furnaces));
    }
}

/// The blades of one propeller, a child of the ship. They turn about the local Z axis.
#[derive(Component, Debug)]
struct PropellerBlades;

/// The fire in the furnace chamber, a child of the ship, with its own material.
#[derive(Component, Debug)]
struct FurnaceFire {
    material: Handle<StandardMaterial>,
    /// The brightness that the material has now, so it changes only when needed.
    shown: f32,
}

fn add_engine_mesh(
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
    let iron = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.2, 0.22),
        metallic: 0.6,
        perceptual_roughness: 0.6,
        ..default()
    });
    let wood = materials.add(Color::srgb(0.35, 0.22, 0.12));
    let brass = materials.add(Color::srgb(0.85, 0.65, 0.2));
    let fire = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.05, 0.04),
        emissive: LinearRgba::BLACK,
        ..default()
    });

    let mut parts: Vec<(Handle<Mesh>, Handle<StandardMaterial>, Transform)> = Vec::new();
    for (center, size) in furnace_boxes() {
        parts.push((
            meshes.add(Cuboid::from_size(size)),
            iron.clone(),
            layout.furnace * Transform::from_translation(center),
        ));
    }
    parts.push((
        meshes.add(
            Cylinder::new(CHIMNEY_RADIUS, CHIMNEY_HEIGHT)
                .mesh()
                .resolution(10),
        ),
        iron.clone(),
        layout.furnace * Transform::from_translation(chimney_center()),
    ));
    for (center, size) in bunker_boxes(class) {
        parts.push((
            meshes.add(Cuboid::from_size(size)),
            wood.clone(),
            layout.coal_bunker * Transform::from_translation(center),
        ));
    }

    let (chamber, chamber_size) = fire_chamber();
    let fire_mesh = meshes.add(Cuboid::new(
        chamber_size.x - 0.02,
        chamber_size.y - 0.02,
        chamber_size.z - 0.1,
    ));
    let fire_transform = layout.furnace * Transform::from_translation(chamber - Vec3::Z * 0.05);
    // A draft vent below the hatch shares the fire material, so the fire shows while the
    // hatch is shut.
    parts.push((
        meshes.add(Cuboid::new(0.4, 0.08, 0.02)),
        fire.clone(),
        layout.furnace * Transform::from_xyz(0.0, 0.15, FURNACE_SIZE.z / 2.0),
    ));

    let hub = meshes
        .add(Mesh::from(Cylinder::new(0.15, 0.2)).rotated_by(Quat::from_rotation_x(FRAC_PI_2)));
    let blade = meshes.add(Cuboid::new(0.18, BLADE_LENGTH, 0.04));
    let propellers = layout.propellers.clone();

    commands.entity(add.entity).with_children(|ship| {
        for (mesh, material, transform) in parts {
            ship.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform));
        }
        ship.spawn((
            FurnaceFire {
                material: fire.clone(),
                shown: 0.0,
            },
            Mesh3d(fire_mesh),
            MeshMaterial3d(fire),
            fire_transform,
        ));
        for mount in propellers {
            ship.spawn((
                PropellerBlades,
                mount * Transform::from_xyz(0.0, 0.0, 0.12),
                Visibility::default(),
            ))
            .with_children(|blades| {
                blades.spawn((Mesh3d(hub.clone()), MeshMaterial3d(brass.clone())));
                for index in 0..3 {
                    let turn = Quat::from_rotation_z(index as f32 * TAU / 3.0);
                    // A small pitch about the blade axis makes the blades read as a screw.
                    let pitch = Quat::from_rotation_y(0.4);
                    blades.spawn((
                        Mesh3d(blade.clone()),
                        MeshMaterial3d(brass.clone()),
                        Transform::from_rotation(turn)
                            * Transform::from_translation(Vec3::Y * BLADE_LENGTH / 2.0)
                                .with_rotation(pitch),
                    ));
                }
            });
        }
    });
}

fn add_coal_mesh(
    add: On<Add, Coal>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.entity(add.entity).insert((
        Mesh3d(meshes.add(Cuboid::from_length(COAL_SIZE))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.07, 0.07, 0.08),
            perceptual_roughness: 0.4,
            ..default()
        })),
    ));
}

/// The plate of the hatch and the colored knobs of the levers. The lever mesh of
/// [`crate::controls`] draws the handle.
fn add_engine_part_mesh(
    add: On<Add, EnginePart>,
    parts: Query<&EnginePart>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(&part) = parts.get(add.entity) else {
        return;
    };
    let (mesh, color, transform): (Mesh, Color, Transform) = match part {
        EnginePart::Hatch => (
            Cuboid::new(HATCH_THICKNESS, HATCH_HEIGHT, HATCH_WIDTH).into(),
            Color::srgb(0.3, 0.3, 0.32),
            Transform::from_translation(Vec3::Y * HATCH_HEIGHT / 2.0),
        ),
        EnginePart::Throttle => (
            Sphere::new(0.1).into(),
            Color::srgb(0.95, 0.8, 0.1),
            Transform::from_translation(Vec3::Y * 0.8),
        ),
        EnginePart::Ignition => (
            Sphere::new(0.1).into(),
            Color::srgb(0.9, 0.1, 0.1),
            Transform::from_translation(Vec3::Y * 0.8),
        ),
    };
    commands
        .entity(add.entity)
        .insert(Visibility::default())
        .with_child((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(color)),
            transform,
        ));
}

fn spin_propellers(
    time: Res<Time>,
    ships: Query<&Engine>,
    mut blades: Query<(&ChildOf, &mut Transform), With<PropellerBlades>>,
) {
    for (child_of, mut transform) in &mut blades {
        let Ok(engine) = ships.get(child_of.parent()) else {
            continue;
        };
        let speed = engine.power() * FULL_SPIN * TAU;
        transform.rotate_local_z(speed * time.delta_secs());
    }
}

fn glow_furnaces(
    ships: Query<&Engine>,
    mut fires: Query<(&ChildOf, &mut FurnaceFire)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (child_of, mut fire) in &mut fires {
        let brightness = ships.get(child_of.parent()).map_or(0.0, |engine| {
            if engine.lit {
                GLOW_IDLE + (GLOW_FULL - GLOW_IDLE) * engine.throttle.abs().min(1.0)
            } else {
                0.0
            }
        });
        if (brightness - fire.shown).abs() < 0.1 {
            continue;
        }
        fire.shown = brightness;
        if let Some(mut material) = materials.get_mut(&fire.material) {
            material.emissive = LinearRgba::rgb(1.0, 0.25, 0.02) * brightness;
        }
    }
}
