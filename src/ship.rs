//! The skyship: a hull that sails on the cloud sea. The host simulates the hull, and each
//! client sees the replicated motion.
//!
//! The ship comes in three classes. The host picks the class from the number of players,
//! and while the ship is docked it swaps the ship when that number changes.

use std::str::FromStr;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::cloud_sea::{SEA_LEVEL, cloud_sea_depth};
use crate::net::{NetMode, Owner};
use crate::pigeon::Pigeon;

/// Low rails on the deck edge. A pigeon can hop over them, a sliding crate stops at them.
pub const RAIL_HEIGHT: f32 = 0.5;
const RAIL_THICKNESS: f32 = 0.15;
/// 80 m³ of class A hull at this density gives a ship of 800 kg. The larger classes keep
/// the density, so a pigeon feels their extra mass when it pushes them.
const HULL_DENSITY: f32 = 10.0;
/// The part of the hull height below the surface in calm water.
const REST_DRAFT: f32 = 0.4;
/// The outermost probes sit this fraction of the hull size from the center. The spread lets a
/// crest under the bow lift the bow first, so the waves pitch and roll the ship.
const PROBE_SPREAD: f32 = 0.4;
/// Drag of the cloud sea on a submerged probe, per second. Below the critical value of the
/// heave, so the ship still bobs a few times before it settles.
const CLOUD_DRAG: f32 = 1.5;
const GRAVITY: f32 = 9.81;
/// The east edge of the main island at the pier. The docked hull stays clear of it.
const DOCK_EDGE_X: f32 = 10.6;
/// The gap between the island edge and the hull side. A pigeon can jump it.
const DOCK_CLEARANCE: f32 = 0.4;

pub struct ShipPlugin;

impl Plugin for ShipPlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<Ship, ()>()
            .replicate_as::<ShipClass, u8>()
            .init_resource::<Docked>()
            .init_resource::<ShipOverride>()
            .add_systems(
                Update,
                dock_ship.run_if(|mode: Res<NetMode>| mode.is_authority()),
            )
            .add_systems(FixedUpdate, float_ship);
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Ship;

// Replicon needs a serde type, and a marker carries no data.
impl From<Ship> for () {
    fn from(_: Ship) -> Self {}
}

impl From<()> for Ship {
    fn from(_: ()) -> Self {
        Ship
    }
}

/// The size class of a ship. Each class has its own hull and [`ShipLayout`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShipClass {
    /// 2 to 4 players.
    A,
    /// 5 or 6 players.
    B,
    /// 7 players or more.
    C,
}

impl From<ShipClass> for u8 {
    fn from(class: ShipClass) -> Self {
        match class {
            ShipClass::A => 0,
            ShipClass::B => 1,
            ShipClass::C => 2,
        }
    }
}

// Only `From<ShipClass>` writes this byte, so a value above 2 does not occur.
impl From<u8> for ShipClass {
    fn from(value: u8) -> Self {
        match value {
            0 => ShipClass::A,
            1 => ShipClass::B,
            _ => ShipClass::C,
        }
    }
}

impl FromStr for ShipClass {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "a" => Ok(ShipClass::A),
            "b" => Ok(ShipClass::B),
            "c" => Ok(ShipClass::C),
            _ => Err(format!(
                "The ship class \"{value}\" is not known. Use a, b or c."
            )),
        }
    }
}

/// The class of the ship for `players` players.
pub fn class_for_players(players: usize) -> ShipClass {
    match players {
        0..=4 => ShipClass::A,
        5..=6 => ShipClass::B,
        _ => ShipClass::C,
    }
}

/// The positions of the stations along the deck, per class. All z values are in meters,
/// and the bow is at -Z.
struct Stations {
    hull: Vec3,
    /// Probes across and along the hull bottom. More probes on a longer hull keep the
    /// waves under every part of it.
    probes: (usize, usize),
    helm_z: f32,
    /// The main mast comes first. It carries the crow's nest.
    masts: &'static [f32],
    mast_height: f32,
    furnace_z: f32,
    cabin: Vec3,
    propellers_x: &'static [f32],
    hooks: usize,
}

impl ShipClass {
    pub const ALL: [ShipClass; 3] = [ShipClass::A, ShipClass::B, ShipClass::C];

    // The helm is at the bow, the engine aft of the middle, and the map table at the stern,
    // so one pigeon cannot work two stations at once.
    fn stations(self) -> Stations {
        match self {
            ShipClass::A => Stations {
                hull: Vec3::new(4.0, 2.0, 10.0),
                probes: (2, 4),
                helm_z: -3.4,
                masts: &[-1.2],
                mast_height: 7.0,
                furnace_z: 0.9,
                cabin: Vec3::new(2.6, 1.6, 2.0),
                propellers_x: &[0.0],
                hooks: 4,
            },
            ShipClass::B => Stations {
                hull: Vec3::new(5.5, 2.4, 15.0),
                probes: (3, 6),
                helm_z: -5.6,
                masts: &[-0.4, -3.6],
                mast_height: 8.5,
                furnace_z: 2.8,
                cabin: Vec3::new(3.2, 1.8, 2.6),
                propellers_x: &[0.0],
                hooks: 6,
            },
            ShipClass::C => Stations {
                hull: Vec3::new(7.0, 3.0, 22.0),
                probes: (3, 8),
                helm_z: -8.6,
                masts: &[-1.5, -5.6, 3.0],
                mast_height: 10.0,
                furnace_z: 5.8,
                cabin: Vec3::new(4.0, 2.0, 3.2),
                propellers_x: &[-1.8, 1.8],
                hooks: 10,
            },
        }
    }

    /// Width, height and length of the hull. The deck is the top face.
    pub fn hull_size(self) -> Vec3 {
        self.stations().hull
    }

    /// The transform of a ship of this class at the dock, at its rest draft, so it does not
    /// drop onto the sea.
    pub fn dock_transform(self) -> Transform {
        let half = self.hull_size() / 2.0;
        Transform::from_xyz(
            DOCK_EDGE_X + DOCK_CLEARANCE + half.x,
            SEA_LEVEL + half.y - REST_DRAFT * 2.0 * half.y,
            0.0,
        )
    }

    /// The mount points of the ship parts, in the frame of the ship.
    pub fn layout(self) -> ShipLayout {
        let stations = self.stations();
        let half = stations.hull / 2.0;
        let deck = half.y;
        let at = |x: f32, z: f32| Transform::from_xyz(x, deck, z);

        let helm = at(0.0, stations.helm_z);
        let masts: Vec<Transform> = stations.masts.iter().map(|&z| at(0.0, z)).collect();
        let main_z = stations.masts.first().copied().unwrap_or(0.0);
        let crows_nest = Transform::from_xyz(0.0, deck + stations.mast_height - 0.6, main_z);
        let map_table_z = half.z - 0.2 - stations.cabin.z / 2.0;

        let per_side = stations.hooks / 2;
        let cargo_hooks = [-1.0, 1.0]
            .into_iter()
            .flat_map(|side| {
                (0..per_side).map(move |index| {
                    let z = if per_side > 1 {
                        -0.55 + 1.1 * index as f32 / (per_side - 1) as f32
                    } else {
                        0.0
                    };
                    at(side * (half.x - 0.3), z * half.z)
                })
            })
            .collect();

        ShipLayout {
            hull: stations.hull,
            mast_height: stations.mast_height,
            cabin: stations.cabin,
            helm,
            masts,
            crows_nest,
            furnace: at(0.0, stations.furnace_z),
            coal_bunker: at(-0.55 * half.x, stations.furnace_z),
            throttle_lever: at(0.5 * half.x, stations.furnace_z - 0.4),
            ignition_lever: at(0.5 * half.x, stations.furnace_z + 0.4),
            propellers: stations
                .propellers_x
                .iter()
                .map(|&x| Transform::from_xyz(x, -0.4 * half.y, half.z))
                .collect(),
            anchor_winch: at(0.0, -half.z + 0.6),
            bell: at(-(half.x - 0.6), main_z),
            tube_helm_engine: [
                helm * Transform::from_xyz(0.6, 0.0, 0.4),
                at(0.0, stations.furnace_z - 0.9),
            ],
            tube_helm_nest: [
                helm * Transform::from_xyz(-0.6, 0.0, 0.4),
                crows_nest * Transform::from_xyz(0.5, 0.3, 0.0),
            ],
            cargo_hooks,
            map_table: at(0.0, map_table_z),
        }
    }

    fn probe_count(self) -> f32 {
        let (across, along) = self.stations().probes;
        (across * along) as f32
    }
}

/// The kinds of ship parts, one per mount in [`ShipLayout::mounts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mount {
    Helm,
    Mast,
    CrowsNest,
    Furnace,
    CoalBunker,
    ThrottleLever,
    IgnitionLever,
    Propeller,
    AnchorWinch,
    Bell,
    SpeakingTube,
    CargoHook,
    MapTable,
}

/// The mount points of one ship class. Each transform is in the frame of the ship and sits on
/// the surface that carries the part: the deck, the hull, or the top of the main mast.
/// Forward is -Z.
#[derive(Debug, Clone, PartialEq)]
pub struct ShipLayout {
    pub hull: Vec3,
    /// The height of the main mast above the deck.
    pub mast_height: f32,
    /// Width, height and depth of the cabin around the map table.
    pub cabin: Vec3,
    pub helm: Transform,
    /// The foot of each mast. The first mast is the main mast.
    pub masts: Vec<Transform>,
    /// The floor of the crow's nest, near the top of the main mast.
    pub crows_nest: Transform,
    pub furnace: Transform,
    pub coal_bunker: Transform,
    pub throttle_lever: Transform,
    pub ignition_lever: Transform,
    /// The hubs on the stern face of the hull.
    pub propellers: Vec<Transform>,
    pub anchor_winch: Transform,
    pub bell: Transform,
    /// The helm end first, then the engine end.
    pub tube_helm_engine: [Transform; 2],
    /// The helm end first, then the crow's nest end.
    pub tube_helm_nest: [Transform; 2],
    pub cargo_hooks: Vec<Transform>,
    pub map_table: Transform,
}

impl ShipLayout {
    /// Every mount with its kind, for code that treats all parts alike.
    pub fn mounts(&self) -> Vec<(Mount, Transform)> {
        let mut mounts = vec![
            (Mount::Helm, self.helm),
            (Mount::CrowsNest, self.crows_nest),
            (Mount::Furnace, self.furnace),
            (Mount::CoalBunker, self.coal_bunker),
            (Mount::ThrottleLever, self.throttle_lever),
            (Mount::IgnitionLever, self.ignition_lever),
            (Mount::AnchorWinch, self.anchor_winch),
            (Mount::Bell, self.bell),
            (Mount::MapTable, self.map_table),
        ];
        mounts.extend(self.masts.iter().map(|&mount| (Mount::Mast, mount)));
        mounts.extend(
            self.propellers
                .iter()
                .map(|&mount| (Mount::Propeller, mount)),
        );
        mounts.extend(
            self.tube_helm_engine
                .iter()
                .chain(&self.tube_helm_nest)
                .map(|&mount| (Mount::SpeakingTube, mount)),
        );
        mounts.extend(
            self.cargo_hooks
                .iter()
                .map(|&mount| (Mount::CargoHook, mount)),
        );
        mounts
    }
}

/// True while the ship lies at the dock. Only then does the host change the class of the
/// ship. Raising the anchor sets it to false.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Docked(pub bool);

impl Default for Docked {
    fn default() -> Self {
        Docked(true)
    }
}

/// A class from the command line. It replaces the class for the player count.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ShipOverride(pub Option<ShipClass>);

/// Removes `--ship CLASS` from the command line. The other arguments stay in their order
/// for [`crate::net::parse_args`].
pub fn take_ship_arg(
    args: impl IntoIterator<Item = String>,
) -> Result<(Vec<String>, Option<ShipClass>), String> {
    let mut rest = Vec::new();
    let mut class = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg != "--ship" {
            rest.push(arg);
            continue;
        }
        let value = args
            .next()
            .ok_or("The flag --ship needs a class. Example: --ship b")?;
        class = Some(value.parse()?);
    }
    Ok((rest, class))
}

/// The hull and the rails as one compound collider, so the ship is one rigid body.
pub fn ship_collider(class: ShipClass) -> Collider {
    let size = class.hull_size();
    let half = size / 2.0;
    let rail_y = half.y + RAIL_HEIGHT / 2.0;
    let side = Collider::cuboid(RAIL_THICKNESS, RAIL_HEIGHT, size.z);
    let end = Collider::cuboid(size.x, RAIL_HEIGHT, RAIL_THICKNESS);
    Collider::compound(vec![
        (
            Vec3::ZERO,
            Quat::IDENTITY,
            Collider::cuboid(size.x, size.y, size.z),
        ),
        (
            Vec3::new(-half.x, rail_y, 0.0),
            Quat::IDENTITY,
            side.clone(),
        ),
        (Vec3::new(half.x, rail_y, 0.0), Quat::IDENTITY, side),
        (Vec3::new(0.0, rail_y, -half.z), Quat::IDENTITY, end.clone()),
        (Vec3::new(0.0, rail_y, half.z), Quat::IDENTITY, end),
    ])
}

/// The physics components of the ship, without a mesh, so tests can spawn one headless.
/// A client receives only [`Ship`], the class and the transform, so it does not simulate
/// the ship.
pub fn ship_body(class: ShipClass) -> impl Bundle {
    (
        Ship,
        class,
        Replicated,
        RigidBody::Dynamic,
        ship_collider(class),
        ColliderDensity(HULL_DENSITY),
        TransformInterpolation,
    )
}

/// The ship that [`dock_ship`] spawned, and the player count that it saw last.
#[derive(Default)]
struct DockState {
    ship: Option<Entity>,
    players: Option<usize>,
}

/// Spawns the first ship, and swaps it for another class when the player count changes
/// while the ship is docked. A count change at sea does not swap the ship later at the dock,
/// because the crew already sails the ship it has.
fn dock_ship(
    mut commands: Commands,
    docked: Res<Docked>,
    forced: Res<ShipOverride>,
    players: Query<(), (With<Pigeon>, With<Owner>)>,
    ships: Query<&ShipClass, With<Ship>>,
    mut state: Local<DockState>,
) {
    let count = players.iter().count();
    let changed = state.players != Some(count);
    state.players = Some(count);
    let wanted = forced.0.unwrap_or_else(|| class_for_players(count));

    if let Some(ship) = state.ship {
        let Ok(class) = ships.get(ship) else {
            return;
        };
        if !docked.0 || !changed || *class == wanted {
            return;
        }
        commands.entity(ship).despawn();
        info!("The ship changes from class {class:?} to class {wanted:?} for {count} players.");
    }
    let ship = commands
        .spawn((ship_body(wanted), wanted.dock_transform()))
        .id();
    state.ship = Some(ship);
}

/// The probe points on the hull bottom, in the frame of the ship.
fn probe_points(class: ShipClass) -> impl Iterator<Item = Vec3> {
    let size = class.hull_size();
    let (across, along) = class.stations().probes;
    let spread = move |index: usize, count: usize| {
        if count > 1 {
            PROBE_SPREAD * (2.0 * index as f32 / (count - 1) as f32 - 1.0)
        } else {
            0.0
        }
    };
    (0..across).flat_map(move |x| {
        (0..along).map(move |z| Vec3::new(spread(x, across), -0.5, spread(z, along)) * size)
    })
}

/// The upward force on one probe. The probes together carry the whole weight at the rest
/// draft, and the force stops growing when the probe is a full hull height deep.
fn probe_lift(class: ShipClass, depth: f32, mass: f32) -> f32 {
    let height = class.hull_size().y;
    let stiffness = mass * GRAVITY / (class.probe_count() * REST_DRAFT * height);
    stiffness * depth.clamp(0.0, height)
}

fn float_ship(time: Res<Time>, mut ships: Query<(Forces, &ComputedMass, &ShipClass), With<Ship>>) {
    let now = time.elapsed_secs();
    for (mut forces, mass, &class) in &mut ships {
        let mass = mass.value();
        let height = class.hull_size().y;
        let probe_count = class.probe_count();
        let position = forces.position().0;
        let rotation = forces.rotation().0;
        for probe in probe_points(class) {
            let point = position + rotation * probe;
            let depth = cloud_sea_depth(point, now);
            if depth <= 0.0 {
                continue;
            }
            let submerged = (depth / height).min(1.0);
            let drag = -forces.velocity_at_point(point) * CLOUD_DRAG * mass / probe_count;
            forces.apply_force_at_point(
                Vec3::Y * probe_lift(class, depth, mass) + drag * submerged,
                point,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn probes_carry_weight_at_rest_draft() {
        let mass = 800.0;
        for class in ShipClass::ALL {
            let depth = REST_DRAFT * class.hull_size().y;
            let total = probe_lift(class, depth, mass) * class.probe_count();
            assert!(
                (total - mass * GRAVITY).abs() < 1e-2,
                "{class:?} lift {total}"
            );
        }
    }

    #[test]
    fn probe_above_surface_has_no_lift() {
        assert_eq!(probe_lift(ShipClass::A, -0.3, 800.0), 0.0);
    }

    #[test]
    fn probe_lift_stops_at_hull_height() {
        let height = ShipClass::B.hull_size().y;
        assert_eq!(
            probe_lift(ShipClass::B, height * 3.0, 800.0),
            probe_lift(ShipClass::B, height, 800.0)
        );
    }

    #[test]
    fn probes_lie_on_hull_bottom() {
        for class in ShipClass::ALL {
            let size = class.hull_size();
            let points: Vec<Vec3> = probe_points(class).collect();
            assert_eq!(points.len(), class.probe_count() as usize);
            for point in points {
                assert_eq!(point.y, -size.y / 2.0);
                assert!(point.x.abs() < size.x / 2.0 && point.z.abs() < size.z / 2.0);
            }
        }
    }

    #[test]
    fn class_follows_player_count() {
        let expected = [
            (1, ShipClass::A),
            (2, ShipClass::A),
            (4, ShipClass::A),
            (5, ShipClass::B),
            (6, ShipClass::B),
            (7, ShipClass::C),
            (8, ShipClass::C),
        ];
        for (players, class) in expected {
            assert_eq!(class_for_players(players), class, "{players} players");
        }
    }

    #[test]
    fn class_survives_conversion() {
        for class in ShipClass::ALL {
            assert_eq!(ShipClass::from(u8::from(class)), class);
        }
    }

    #[test]
    fn mounts_lie_inside_hull_bounds() {
        for class in ShipClass::ALL {
            let layout = class.layout();
            let half = layout.hull / 2.0;
            for (mount, transform) in layout.mounts() {
                let point = transform.translation;
                assert!(
                    point.x.abs() <= half.x && point.z.abs() <= half.z,
                    "{class:?} {mount:?} at {point}"
                );
                assert!(
                    point.y >= -half.y && point.y <= half.y + layout.mast_height,
                    "{class:?} {mount:?} at {point}"
                );
            }
        }
    }

    #[test]
    fn larger_classes_have_more_parts() {
        let counts = |class: ShipClass| {
            let layout = class.layout();
            (
                layout.masts.len(),
                layout.propellers.len(),
                layout.cargo_hooks.len(),
            )
        };
        assert_eq!(counts(ShipClass::A), (1, 1, 4));
        assert_eq!(counts(ShipClass::B), (2, 1, 6));
        assert_eq!(counts(ShipClass::C), (3, 2, 10));
    }

    #[test]
    fn stations_lie_far_apart() {
        for class in ShipClass::ALL {
            let layout = class.layout();
            let stations = [
                layout.helm,
                layout.furnace,
                layout.map_table,
                layout.crows_nest,
            ];
            for (index, a) in stations.iter().enumerate() {
                for b in &stations[index + 1..] {
                    let distance = a.translation.distance(b.translation);
                    assert!(distance > 2.5, "{class:?} stations {distance} m apart");
                }
            }
        }
    }

    #[test]
    fn docked_hull_clears_island() {
        for class in ShipClass::ALL {
            let west = class.dock_transform().translation.x - class.hull_size().x / 2.0;
            assert!(
                (west - DOCK_EDGE_X - DOCK_CLEARANCE).abs() < 1e-4,
                "{class:?}"
            );
        }
    }

    #[test]
    fn ship_arg_is_removed() {
        assert_eq!(
            take_ship_arg(args(&["--host", "--ship", "b", "6000"])),
            Ok((args(&["--host", "6000"]), Some(ShipClass::B)))
        );
    }

    #[test]
    fn no_ship_arg_keeps_args() {
        assert_eq!(
            take_ship_arg(args(&["--host"])),
            Ok((args(&["--host"]), None))
        );
    }

    #[test]
    fn bad_ship_class_names_input() {
        let error = take_ship_arg(args(&["--ship", "z"])).unwrap_err();
        assert!(error.contains("\"z\""), "{error}");
    }

    #[test]
    fn ship_flag_without_class_fails() {
        assert!(take_ship_arg(args(&["--ship"])).is_err());
    }
}
