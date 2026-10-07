//! The sails: one square sail on each mast. A pigeon hoists a sail with the halyard crank
//! and sets its angle about the mast with the sheet lever. The wind pushes the ship through
//! the hoisted sails, and the keel of the helm turns a push from the side into speed ahead.
//!
//! The authority spawns the controls with each ship and simulates the push. A client
//! receives the hoist and the angle of each sail and draws the sails.

use std::f32::consts::{FRAC_PI_2, PI};

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::controls::{
    ControlSystems, ControlValue, CrankSpec, LeverSpec, PartOf, spawn_crank, spawn_lever,
};
use crate::net::NetMode;
use crate::ship::{Ship, ShipClass, ShipLayout};
use crate::wind::Wind;

/// The sheet turns a sail this far each way from square, in radians (70°).
pub const MAX_SHEET_ANGLE: f32 = 70.0 * PI / 180.0;
/// The turns of the halyard crank from a furled sail to a full sail.
pub const HOIST_TURNS: f32 = 3.0;
/// The push on a square sail in newtons per m² of hoisted canvas and per (m/s)² of
/// apparent wind. Far above real air, because the cloud sea drags the hull much harder
/// than water: a class A ship runs at about 2 m/s before a 4 m/s wind.
pub const SAIL_COEFFICIENT: f32 = 12.0;
/// The highest push in newtons per m² of hoisted canvas. A larger push heels the ship too
/// far and lifts the windward probes out of the cloud sea.
pub const MAX_PRESSURE: f32 = 50.0;
/// The second and third masts are this much shorter than the main mast.
const MINOR_MAST_SCALE: f32 = 0.85;
/// The yard sits at this fraction of the mast height above the deck.
const YARD_HEIGHT: f32 = 0.85;
/// A full sail reaches down this fraction of the mast height from the yard. Its foot stays
/// above the head of a pigeon on deck.
const SAIL_DROP: f32 = 0.6;
/// The yard is this much wider than the hull, as on a square rigger.
const SAIL_WIDTH: f32 = 1.1;
/// The sail hangs this far aft of the mast axis, so it does not cut the mast.
const SAIL_OFFSET: f32 = 0.2;
/// The crank sits at the starboard side of the mast and the lever aft of the port side, so
/// a pigeon on deck reaches each one and the two do not touch.
const CRANK_OFFSET: Vec3 = Vec3::new(0.35, 0.9, 0.0);
const LEVER_OFFSET: Vec3 = Vec3::new(-0.45, 0.05, 0.5);
/// The lever tilts this far each way, in radians.
const LEVER_ANGLE: f32 = 1.0;

pub struct SailsPlugin;

impl Plugin for SailsPlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<Sails, Vec<(f32, f32)>>()
            .init_resource::<Wind>()
            .add_observer(spawn_sail_controls)
            .add_systems(
                FixedUpdate,
                (set_sails, fill_sails).chain().after(ControlSystems),
            );
    }
}

/// Draws the sails of each ship from its [`Sails`]. Each instance adds this plugin.
pub struct SailMeshPlugin;

impl Plugin for SailMeshPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_sail_meshes)
            .add_systems(Update, pose_sails);
    }
}

/// The state of one sail.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SailState {
    /// 0 for a furled sail, 1 for a full sail.
    pub hoist: f32,
    /// The angle of the yard about the mast from square, in radians. A positive angle
    /// turns the starboard end of the yard forward.
    pub angle: f32,
}

/// The sails of a ship, one per mast in the order of [`ShipLayout::masts`].
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct Sails(pub Vec<SailState>);

impl From<Sails> for Vec<(f32, f32)> {
    fn from(sails: Sails) -> Self {
        sails
            .0
            .iter()
            .map(|sail| (sail.hoist, sail.angle))
            .collect()
    }
}

impl From<Vec<(f32, f32)>> for Sails {
    fn from(sails: Vec<(f32, f32)>) -> Self {
        Sails(
            sails
                .into_iter()
                .map(|(hoist, angle)| SailState { hoist, angle })
                .collect(),
        )
    }
}

/// The crank that hoists the sail on the mast with this index.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalyardCrank(pub usize);

/// The lever that sets the angle of the sail on the mast with this index.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetLever(pub usize);

/// The size and the place of one sail, in the frame of the ship.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SailRig {
    /// The foot of the mast on the deck.
    pub mast: Vec3,
    /// The height of the yard above the foot of the mast.
    pub yard_height: f32,
    pub width: f32,
    /// The height of the full sail.
    pub drop: f32,
}

impl SailRig {
    /// The area of the full sail in m².
    pub fn area(&self) -> f32 {
        self.width * self.drop
    }

    /// The middle of the hoisted canvas. The head of the sail stays at the yard, so a
    /// lower hoist moves the middle up.
    pub fn center(&self, sail: SailState) -> Vec3 {
        let hoist = sail.hoist.clamp(0.0, 1.0);
        self.mast
            + Vec3::Y * (self.yard_height - self.drop * hoist / 2.0)
            + Quat::from_rotation_y(sail.angle) * Vec3::Z * SAIL_OFFSET
    }
}

/// The height of the mast with `index` above the deck.
pub fn mast_height(layout: &ShipLayout, index: usize) -> f32 {
    if index == 0 {
        layout.mast_height
    } else {
        layout.mast_height * MINOR_MAST_SCALE
    }
}

/// The sails of a ship of `class`, one per mast.
pub fn sail_rigs(class: ShipClass) -> Vec<SailRig> {
    let layout = class.layout();
    layout
        .masts
        .iter()
        .enumerate()
        .map(|(index, mast)| {
            let height = mast_height(&layout, index);
            SailRig {
                mast: mast.translation,
                yard_height: height * YARD_HEIGHT,
                width: layout.hull.x * SAIL_WIDTH,
                drop: height * SAIL_DROP,
            }
        })
        .collect()
}

/// The normal of a sail at `angle`, in the frame of the ship. A square sail faces the bow.
pub fn sail_normal(angle: f32) -> Vec3 {
    Quat::from_rotation_y(angle) * Vec3::Z
}

/// The hoist for a crank at `turns`.
pub fn hoist_for_turns(turns: f32) -> f32 {
    (turns / HOIST_TURNS).clamp(0.0, 1.0)
}

/// The push of the wind on a flat sail. Only the horizontal parts of `apparent`, the
/// wind relative to the sail, and of `normal` count. The push acts along the normal and
/// grows with the part of the wind across the sail times the wind speed, so an edge-on
/// sail feels nothing and a square sail feels the most.
pub fn sail_force(apparent: Vec3, normal: Vec3, area: f32, hoist: f32) -> Vec3 {
    let apparent = Vec3::new(apparent.x, 0.0, apparent.z);
    let normal = Vec3::new(normal.x, 0.0, normal.z).normalize_or_zero();
    let canvas = area.max(0.0) * hoist.clamp(0.0, 1.0);
    let limit = MAX_PRESSURE * canvas;
    let push = SAIL_COEFFICIENT * canvas * apparent.dot(normal) * apparent.length();
    normal * push.clamp(-limit, limit)
}

/// The hinge of the halyard crank of a sail. The axis points to starboard, toward the
/// pigeon at the crank.
pub fn crank_hinge(rig: &SailRig) -> Transform {
    Transform::from_translation(rig.mast + CRANK_OFFSET)
        .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
}

/// The hinge of the sheet lever of a sail. The axis points to port, toward the pigeon at
/// the lever, and the handle swings fore and aft.
pub fn lever_hinge(rig: &SailRig) -> Transform {
    Transform::from_translation(rig.mast + LEVER_OFFSET)
        .with_rotation(Quat::from_rotation_y(-FRAC_PI_2))
}

fn spawn_sail_controls(
    add: On<Add, Ship>,
    mode: Res<NetMode>,
    classes: Query<&ShipClass>,
    mut commands: Commands,
) {
    if !mode.is_authority() {
        return;
    }
    let Ok(&class) = classes.get(add.entity) else {
        return;
    };
    let rigs = sail_rigs(class);
    commands
        .entity(add.entity)
        .insert(Sails(vec![SailState::default(); rigs.len()]));
    for (index, rig) in rigs.iter().enumerate() {
        let crank = spawn_crank(
            &mut commands,
            add.entity,
            crank_hinge(rig),
            CrankSpec {
                turns: (0.0, HOIST_TURNS),
                ..default()
            },
        );
        commands.entity(crank).insert(HalyardCrank(index));
        let lever = spawn_lever(
            &mut commands,
            add.entity,
            lever_hinge(rig),
            LeverSpec {
                angles: (-LEVER_ANGLE, LEVER_ANGLE),
                values: (-MAX_SHEET_ANGLE, MAX_SHEET_ANGLE),
                ..default()
            },
        );
        commands.entity(lever).insert(SheetLever(index));
    }
}

fn set_sails(
    cranks: Query<(&PartOf, &HalyardCrank, &ControlValue)>,
    levers: Query<(&PartOf, &SheetLever, &ControlValue)>,
    mut ships: Query<&mut Sails, With<Ship>>,
) {
    for (part_of, crank, value) in &cranks {
        let Ok(mut sails) = ships.get_mut(part_of.0) else {
            continue;
        };
        let hoist = hoist_for_turns(value.0);
        // Only a real change marks the sails as changed, so replication stays quiet.
        if let Some(sail) = sails.bypass_change_detection().0.get_mut(crank.0)
            && sail.hoist != hoist
        {
            sail.hoist = hoist;
            sails.set_changed();
        }
    }
    for (part_of, lever, value) in &levers {
        let Ok(mut sails) = ships.get_mut(part_of.0) else {
            continue;
        };
        let angle = value.0.clamp(-MAX_SHEET_ANGLE, MAX_SHEET_ANGLE);
        if let Some(sail) = sails.bypass_change_detection().0.get_mut(lever.0)
            && sail.angle != angle
        {
            sail.angle = angle;
            sails.set_changed();
        }
    }
}

fn fill_sails(wind: Res<Wind>, mut ships: Query<(Forces, &ShipClass, &Sails), With<Ship>>) {
    let wind = wind.velocity();
    for (mut forces, &class, sails) in &mut ships {
        let position = forces.position().0;
        let rotation = forces.rotation().0;
        for (rig, &sail) in sail_rigs(class).iter().zip(&sails.0) {
            if sail.hoist <= 0.0 {
                continue;
            }
            let center = position + rotation * rig.center(sail);
            let apparent = wind - forces.velocity_at_point(center);
            let normal = rotation * sail_normal(sail.angle);
            let force = sail_force(apparent, normal, rig.area(), sail.hoist);
            // At the sail, high above the deck, so the push also heels the ship.
            forces.apply_force_at_point(force, center);
        }
    }
}

/// The yard of a sail, a child of the ship at the top of the mast. It turns with the
/// sheet angle.
#[derive(Component, Debug)]
struct SailYard(usize);

/// The canvas of a sail, a child of the yard. Its head is at the yard, and it scales down
/// with the hoist.
#[derive(Component, Debug)]
struct SailCanvas;

/// The furled canvas at the foot of the hoisted part. It is thick when the sail is furled.
#[derive(Component, Debug)]
struct SailRoll;

/// The yard, apart from the canvas and the roll, so the transform queries do not overlap.
type NotCloth = (Without<SailCanvas>, Without<SailRoll>);

const ROLL_RADIUS: f32 = 0.18;
const SPAR_RADIUS: f32 = 0.08;

fn add_sail_meshes(
    add: On<Add, ShipClass>,
    classes: Query<&ShipClass>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(&class) = classes.get(add.entity) else {
        return;
    };
    let wood = materials.add(Color::srgb(0.75, 0.6, 0.4));
    let canvas = materials.add(Color::srgb(0.97, 0.94, 0.84));
    let along_x = Quat::from_rotation_z(FRAC_PI_2);
    commands.entity(add.entity).with_children(|ship| {
        for (index, rig) in sail_rigs(class).iter().enumerate() {
            let spar = meshes
                .add(Mesh::from(Cylinder::new(SPAR_RADIUS, rig.width + 0.3)).rotated_by(along_x));
            let sheet = meshes.add(
                Mesh::from(Cuboid::new(rig.width, rig.drop, 0.04))
                    .translated_by(Vec3::NEG_Y * rig.drop / 2.0),
            );
            let roll = meshes
                .add(Mesh::from(Cylinder::new(ROLL_RADIUS, rig.width * 0.95)).rotated_by(along_x));
            ship.spawn((
                SailYard(index),
                Transform::from_translation(rig.mast + Vec3::Y * rig.yard_height),
                Visibility::default(),
            ))
            .with_children(|yard| {
                yard.spawn((
                    Mesh3d(spar),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, 0.0, SAIL_OFFSET),
                ));
                yard.spawn((
                    SailCanvas,
                    Mesh3d(sheet),
                    MeshMaterial3d(canvas.clone()),
                    Transform::from_xyz(0.0, 0.0, SAIL_OFFSET),
                ));
                yard.spawn((
                    SailRoll,
                    Mesh3d(roll),
                    MeshMaterial3d(canvas.clone()),
                    Transform::from_xyz(0.0, -ROLL_RADIUS, SAIL_OFFSET),
                ));
            });
        }
    });
}

fn pose_sails(
    ships: Query<(&ShipClass, Option<&Sails>)>,
    mut yards: Query<(&SailYard, &ChildOf, &Children, &mut Transform), NotCloth>,
    mut canvases: Query<&mut Transform, (With<SailCanvas>, Without<SailRoll>)>,
    mut rolls: Query<&mut Transform, (With<SailRoll>, Without<SailCanvas>)>,
) {
    for (yard, child_of, children, mut transform) in &mut yards {
        let Ok((&class, sails)) = ships.get(child_of.parent()) else {
            continue;
        };
        let Some(rig) = sail_rigs(class).get(yard.0).copied() else {
            continue;
        };
        let sail = sails
            .and_then(|sails| sails.0.get(yard.0).copied())
            .unwrap_or_default();
        let hoist = sail.hoist.clamp(0.0, 1.0);
        transform.rotation = Quat::from_rotation_y(sail.angle);
        for &child in children {
            if let Ok(mut canvas) = canvases.get_mut(child) {
                // A zero scale breaks the normals of the mesh.
                canvas.scale.y = hoist.max(0.01);
            }
            if let Ok(mut roll) = rolls.get_mut(child) {
                // The furled cloth thins as more of it hangs in the sail.
                let thickness = 1.0 - 0.6 * hoist;
                roll.translation.y = -rig.drop * hoist - ROLL_RADIUS * thickness;
                roll.scale = Vec3::new(thickness, 1.0, thickness);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: f32 = 20.0;

    #[test]
    fn no_wind_gives_no_force() {
        let force = sail_force(Vec3::ZERO, Vec3::Z, AREA, 1.0);
        assert_eq!(force, Vec3::ZERO);
    }

    #[test]
    fn edge_on_sail_gives_no_force() {
        let force = sail_force(Vec3::X * 4.0, sail_normal(0.0), AREA, 1.0);
        assert!(force.length() < 1e-3, "{force}");
    }

    #[test]
    fn square_sail_gives_most_force() {
        let wind = Vec3::NEG_Z * 2.0;
        let square = sail_force(wind, sail_normal(0.0), AREA, 1.0).length();
        for angle in [0.3, 0.7, 1.2, -0.5] {
            let turned = sail_force(wind, sail_normal(angle), AREA, 1.0).length();
            assert!(turned < square, "{angle}: {turned} >= {square}");
        }
        assert!(
            (square - SAIL_COEFFICIENT * AREA * 4.0).abs() < 1e-3,
            "{square}"
        );
    }

    #[test]
    fn furled_sail_gives_no_force() {
        let force = sail_force(Vec3::NEG_Z * 4.0, Vec3::Z, AREA, 0.0);
        assert_eq!(force, Vec3::ZERO);
    }

    #[test]
    fn force_grows_with_hoist() {
        let wind = Vec3::NEG_Z * 2.0;
        let half = sail_force(wind, Vec3::Z, AREA, 0.5).length();
        let full = sail_force(wind, Vec3::Z, AREA, 1.0).length();
        assert!((full - 2.0 * half).abs() < 1e-3, "{half} {full}");
    }

    #[test]
    fn force_pushes_downwind() {
        let wind = Vec3::new(-3.0, 0.0, -1.0);
        for normal in [Vec3::Z, Vec3::NEG_Z, sail_normal(0.6), sail_normal(-0.6)] {
            let force = sail_force(wind, normal, AREA, 1.0);
            assert!(force.dot(wind) > 0.0, "{normal}: {force}");
            assert!(force.normalize().cross(normal).length() < 1e-4, "{force}");
        }
    }

    #[test]
    fn angled_sail_in_side_wind_pushes_forward() {
        // The wind blows from starboard to port, and the bow points to -Z.
        let force = sail_force(Vec3::NEG_X * 4.0, sail_normal(MAX_SHEET_ANGLE), AREA, 1.0);
        assert!(force.z < 0.0, "{force}");
        let mirrored = sail_force(Vec3::X * 4.0, sail_normal(-MAX_SHEET_ANGLE), AREA, 1.0);
        assert!(mirrored.z < 0.0, "{mirrored}");
    }

    #[test]
    fn force_ignores_vertical_wind() {
        let force = sail_force(Vec3::new(0.0, 5.0, -2.0), Vec3::Z, AREA, 1.0);
        assert_eq!(force.y, 0.0);
        assert_eq!(force, sail_force(Vec3::NEG_Z * 2.0, Vec3::Z, AREA, 1.0));
    }

    #[test]
    fn force_stays_below_limit() {
        let force = sail_force(Vec3::NEG_Z * 50.0, Vec3::Z, AREA, 1.0);
        assert!(
            (force.length() - MAX_PRESSURE * AREA).abs() < 1e-2,
            "{force}"
        );
    }

    #[test]
    fn crank_turns_map_to_hoist() {
        assert_eq!(hoist_for_turns(0.0), 0.0);
        assert_eq!(hoist_for_turns(HOIST_TURNS / 2.0), 0.5);
        assert_eq!(hoist_for_turns(HOIST_TURNS), 1.0);
        assert_eq!(hoist_for_turns(-1.0), 0.0);
        assert_eq!(hoist_for_turns(9.0), 1.0);
    }

    #[test]
    fn larger_classes_carry_more_canvas() {
        let canvas = |class| sail_rigs(class).iter().map(SailRig::area).sum::<f32>();
        assert!(canvas(ShipClass::B) > 2.0 * canvas(ShipClass::A));
        assert!(canvas(ShipClass::C) > 2.0 * canvas(ShipClass::B));
    }

    #[test]
    fn one_sail_per_mast() {
        for class in ShipClass::ALL {
            assert_eq!(sail_rigs(class).len(), class.layout().masts.len());
        }
    }

    #[test]
    fn sail_center_rises_as_sail_furls() {
        let rig = sail_rigs(ShipClass::A)[0];
        let full = rig.center(SailState {
            hoist: 1.0,
            angle: 0.0,
        });
        let half = rig.center(SailState {
            hoist: 0.5,
            angle: 0.0,
        });
        assert!(half.y > full.y);
        assert!(full.y > rig.mast.y + 1.0, "{full}");
    }

    #[test]
    fn controls_stay_on_deck_near_mast() {
        for class in ShipClass::ALL {
            let half = class.hull_size() / 2.0;
            for rig in sail_rigs(class) {
                for hinge in [crank_hinge(&rig), lever_hinge(&rig)] {
                    let offset = hinge.translation - rig.mast;
                    assert!(offset.length() < 1.2, "{offset}");
                    assert!(hinge.translation.x.abs() < half.x - 0.5);
                    assert!(hinge.translation.z.abs() < half.z - 0.5);
                }
            }
        }
    }

    #[test]
    fn sails_survive_conversion() {
        let sails = Sails(vec![
            SailState {
                hoist: 0.4,
                angle: -0.3,
            },
            SailState {
                hoist: 1.0,
                angle: 0.0,
            },
        ]);
        assert_eq!(Sails::from(Vec::<(f32, f32)>::from(sails.clone())), sails);
    }
}
