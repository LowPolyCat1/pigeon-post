//! The coal engine of a skyship. Pigeons carry coal lumps from the bunker into the furnace,
//! light the fire with the ignition lever, and set the throttle lever. The propellers on the
//! stern then push the ship forward.
//!
//! Only the authority simulates the engine. A client receives the [`Engine`] state of each
//! ship, the coal lumps and the levers, and draws the fire and the propellers from them.

use std::f32::consts::FRAC_PI_2;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::controls::{ControlSystems, ControlValue, LeverSpec, PartOf, spawn_lever};
use crate::grab::Grabbable;
use crate::net::NetMode;
use crate::ship::{Ship, ShipClass};

/// The edge of a cubic coal lump, in meters.
pub const COAL_SIZE: f32 = 0.2;
/// 8 dm³ at this density is 1.5 kg, so one wing carries a lump with ease.
const COAL_DENSITY: f32 = 187.5;
/// The hatch plate weighs 0.8 kg. A control has no gravity, and a heavy plate on a heaving
/// deck works itself shut against the joint friction.
const HATCH_DENSITY: f32 = 40.0;
/// The fuel of one lump: this many seconds at full throttle.
pub const FUEL_PER_LUMP: f32 = 60.0;
/// The furnace holds five lumps. A lump that does not fit stays in the furnace until there
/// is room.
pub const MAX_FUEL: f32 = 5.0 * FUEL_PER_LUMP;
/// A lit fire burns this fraction of the full-throttle rate, also at zero throttle.
pub const IDLE_BURN: f32 = 0.1;
/// The throttle goes this far astern. Full ahead is 1.
pub const MAX_REVERSE: f32 = 0.25;

/// The furnace box in the frame of its mount: width, height and depth. The mouth is on the
/// aft face (+Z), so a pigeon stokes it from the stern side.
pub const FURNACE_SIZE: Vec3 = Vec3::new(0.9, 1.1, 0.9);
/// The mouth of the furnace, in the frame of its mount. A pigeon eye is about 0.85 m above
/// the deck, and a held lump hangs a bit lower, so the mouth sits at that height.
pub const MOUTH_WIDTH: f32 = 0.5;
pub const MOUTH_BOTTOM: f32 = 0.3;
pub const MOUTH_TOP: f32 = 0.75;
/// The front wall behind the fire chamber is this thick.
const FURNACE_BACK_WALL: f32 = 0.2;
/// The chimney on the furnace top.
pub const CHIMNEY_RADIUS: f32 = 0.12;
pub const CHIMNEY_HEIGHT: f32 = 0.8;
/// The hatch plate: it covers the mouth with some overlap.
pub const HATCH_WIDTH: f32 = 0.6;
pub const HATCH_HEIGHT: f32 = 0.55;
pub const HATCH_THICKNESS: f32 = 0.06;
/// The hinge of the hatch sits this far aft of the furnace face, so the plate clears it.
const HATCH_GAP: f32 = 0.04;
/// The hatch counts as open past this lever value. At the value 1 the plate lies flat.
pub const HATCH_OPEN: f32 = 0.5;
/// A detent spring pushes the hatch toward the nearer end with this torque, in N·m. The
/// rocking deck does not work an open hatch shut, and a pigeon that pulls past halfway
/// feels the hatch snap.
const HATCH_LATCH_TORQUE: f32 = 2.0;
/// The ignition fires once its lever reaches this value. It fires again only after the lever
/// went back below [`IGNITION_REARM`].
pub const IGNITION_FIRE: f32 = 0.95;
pub const IGNITION_REARM: f32 = 0.3;
/// The levers stand this far above the deck at the hinge.
const LEVER_HINGE_HEIGHT: f32 = 0.05;

/// The walls of the coal bunker.
pub const BUNKER_WALL_HEIGHT: f32 = 0.5;
pub const BUNKER_WALL_THICKNESS: f32 = 0.06;
/// The lumps lie in a grid with this pitch, so they start apart and settle.
const COAL_PITCH: f32 = 0.25;

pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.replicate_as::<Engine, (f32, bool, f32)>()
            .replicate_as::<Coal, ()>()
            .replicate_as::<EnginePart, u8>()
            .add_observer(spawn_engine)
            .add_systems(
                FixedUpdate,
                (
                    latch_hatches,
                    feed_furnace,
                    read_engine_levers,
                    burn_fuel,
                    drive_propellers,
                )
                    .chain()
                    .after(ControlSystems),
            );
    }
}

/// The engine state of a ship.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct Engine {
    /// In seconds at full throttle, up to [`MAX_FUEL`].
    pub fuel: f32,
    pub lit: bool,
    /// From -[`MAX_REVERSE`] to 1.
    pub throttle: f32,
}

impl From<Engine> for (f32, bool, f32) {
    fn from(engine: Engine) -> Self {
        (engine.fuel, engine.lit, engine.throttle)
    }
}

impl From<(f32, bool, f32)> for Engine {
    fn from((fuel, lit, throttle): (f32, bool, f32)) -> Self {
        Engine {
            fuel,
            lit,
            throttle,
        }
    }
}

impl Engine {
    /// Lights the fire if there is fuel. Returns true if the fire burns.
    pub fn ignite(&mut self) -> bool {
        if self.fuel > 0.0 {
            self.lit = true;
        }
        self.lit
    }

    /// Burns the fuel of `dt` seconds. The fire goes out when the fuel is gone.
    pub fn step(&mut self, dt: f32) {
        if !self.lit {
            return;
        }
        self.fuel = burn(self.fuel, self.throttle, dt);
        if self.fuel <= 0.0 {
            self.lit = false;
        }
    }

    /// The share of the full thrust, from -[`MAX_REVERSE`] to 1. A cold engine gives none.
    pub fn power(&self) -> f32 {
        if self.lit {
            self.throttle.clamp(-MAX_REVERSE, 1.0)
        } else {
            0.0
        }
    }
}

/// The fuel that a lit fire burns per second at `throttle`. Astern burns like ahead.
pub fn burn_rate(throttle: f32) -> f32 {
    IDLE_BURN + (1.0 - IDLE_BURN) * throttle.abs().min(1.0)
}

/// The fuel left after a lit fire burned for `dt` seconds.
pub fn burn(fuel: f32, throttle: f32, dt: f32) -> f32 {
    (fuel - burn_rate(throttle) * dt).max(0.0)
}

/// The fuel after one more lump, or `None` if the furnace is full.
pub fn add_lump(fuel: f32) -> Option<f32> {
    if fuel >= MAX_FUEL {
        return None;
    }
    Some((fuel + FUEL_PER_LUMP).min(MAX_FUEL))
}

/// Whether the ignition lever at `value` fires now, and whether it stays armed. The lever
/// fires on the way to its end, not while it stays there.
pub fn ignition_edge(value: f32, armed: bool) -> (bool, bool) {
    if armed && value >= IGNITION_FIRE {
        (true, false)
    } else if value <= IGNITION_REARM {
        (false, true)
    } else {
        (false, armed)
    }
}

/// The thrust of one propeller at full throttle, in newtons. The cloud sea drags the hull
/// with about 0.6 per second, so each class reaches about 4 m/s at full throttle.
pub fn max_thrust(class: ShipClass) -> f32 {
    match class {
        ShipClass::A => 2000.0,
        ShipClass::B => 5000.0,
        ShipClass::C => 5500.0,
    }
}

/// The thrust of one propeller of `class`, in newtons. A negative thrust pushes astern.
pub fn propeller_thrust(class: ShipClass, engine: &Engine) -> f32 {
    max_thrust(class) * engine.power()
}

/// The coal lumps in the bunker of a new ship.
pub fn starting_coal(class: ShipClass) -> usize {
    match class {
        ShipClass::A => 12,
        ShipClass::B => 20,
        ShipClass::C => 30,
    }
}

/// The inner width and depth of the coal bunker. A larger supply needs a larger bunker.
pub fn bunker_inner_size(class: ShipClass) -> f32 {
    match class {
        ShipClass::A => 0.8,
        ShipClass::B => 1.0,
        ShipClass::C => 1.2,
    }
}

/// A loose lump of coal.
#[derive(Component, Debug, Clone, Copy)]
pub struct Coal;

// Replicon needs a serde type, and a marker carries no data.
impl From<Coal> for () {
    fn from(_: Coal) -> Self {}
}

impl From<()> for Coal {
    fn from(_: ()) -> Self {
        Coal
    }
}

/// The physics components of a coal lump, without a mesh, so tests can spawn one headless.
pub fn coal_body() -> impl Bundle {
    (
        Coal,
        Replicated,
        RigidBody::Dynamic,
        Collider::cuboid(COAL_SIZE, COAL_SIZE, COAL_SIZE),
        ColliderDensity(COAL_DENSITY),
        Grabbable,
        TransformInterpolation,
    )
}

/// The engine controls. A client draws each one from its kind.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnginePart {
    /// The furnace hatch: a lever whose handle is the door plate. The value 0 is shut.
    Hatch,
    Throttle,
    Ignition,
}

impl From<EnginePart> for u8 {
    fn from(part: EnginePart) -> Self {
        match part {
            EnginePart::Hatch => 0,
            EnginePart::Throttle => 1,
            EnginePart::Ignition => 2,
        }
    }
}

// Only `From<EnginePart>` writes this byte, so a value above 2 does not occur.
impl From<u8> for EnginePart {
    fn from(value: u8) -> Self {
        match value {
            0 => EnginePart::Hatch,
            1 => EnginePart::Throttle,
            _ => EnginePart::Ignition,
        }
    }
}

/// Whether the ignition lever can fire. See [`ignition_edge`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnitionArmed(pub bool);

/// The sensor inside the furnace mouth, a child of the ship. A lump in it burns while the
/// hatch is open.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FurnaceMouth {
    pub hatch: Entity,
}

/// The boxes of the furnace walls in the frame of the furnace mount, as center and size. The
/// chamber behind the mouth stays open.
pub fn furnace_boxes() -> [(Vec3, Vec3); 5] {
    let size = FURNACE_SIZE;
    let half = size / 2.0;
    let wall = (size.x - MOUTH_WIDTH) / 2.0;
    let chamber = MOUTH_TOP - MOUTH_BOTTOM;
    let middle = (MOUTH_BOTTOM + MOUTH_TOP) / 2.0;
    let top = size.y - MOUTH_TOP;
    [
        (
            Vec3::Y * MOUTH_BOTTOM / 2.0,
            Vec3::new(size.x, MOUTH_BOTTOM, size.z),
        ),
        (
            Vec3::Y * (MOUTH_TOP + top / 2.0),
            Vec3::new(size.x, top, size.z),
        ),
        (
            Vec3::new(-half.x + wall / 2.0, middle, 0.0),
            Vec3::new(wall, chamber, size.z),
        ),
        (
            Vec3::new(half.x - wall / 2.0, middle, 0.0),
            Vec3::new(wall, chamber, size.z),
        ),
        (
            Vec3::new(0.0, middle, -half.z + FURNACE_BACK_WALL / 2.0),
            Vec3::new(MOUTH_WIDTH, chamber, FURNACE_BACK_WALL),
        ),
    ]
}

/// The fire chamber behind the mouth, in the frame of the furnace mount, as center and size.
/// The sensor fills most of it, so a lump counts once it is well inside the mouth.
pub fn fire_chamber() -> (Vec3, Vec3) {
    let half = FURNACE_SIZE / 2.0;
    let depth = FURNACE_SIZE.z - FURNACE_BACK_WALL;
    (
        Vec3::new(0.0, (MOUTH_BOTTOM + MOUTH_TOP) / 2.0, half.z - depth / 2.0),
        Vec3::new(MOUTH_WIDTH, MOUTH_TOP - MOUTH_BOTTOM, depth),
    )
}

/// The chimney on the furnace top, in the frame of the furnace mount.
pub fn chimney_center() -> Vec3 {
    Vec3::new(0.0, FURNACE_SIZE.y + CHIMNEY_HEIGHT / 2.0, -0.15)
}

/// The boxes of the bunker walls in the frame of the bunker mount, as center and size. The
/// deck is the floor and the top is open.
pub fn bunker_boxes(class: ShipClass) -> [(Vec3, Vec3); 4] {
    let inner = bunker_inner_size(class);
    let outer = inner + 2.0 * BUNKER_WALL_THICKNESS;
    let offset = (inner + BUNKER_WALL_THICKNESS) / 2.0;
    let y = BUNKER_WALL_HEIGHT / 2.0;
    let across = Vec3::new(outer, BUNKER_WALL_HEIGHT, BUNKER_WALL_THICKNESS);
    let along = Vec3::new(BUNKER_WALL_THICKNESS, BUNKER_WALL_HEIGHT, inner);
    [
        (Vec3::new(0.0, y, -offset), across),
        (Vec3::new(0.0, y, offset), across),
        (Vec3::new(-offset, y, 0.0), along),
        (Vec3::new(offset, y, 0.0), along),
    ]
}

/// The start positions of the coal lumps in the frame of the bunker mount: a grid per layer,
/// with a gap between the lumps.
pub fn coal_positions(class: ShipClass) -> Vec<Vec3> {
    let per_row = ((bunker_inner_size(class) / COAL_PITCH).floor() as usize).max(1);
    let per_layer = per_row * per_row;
    let start = -(per_row as f32 - 1.0) * COAL_PITCH / 2.0;
    (0..starting_coal(class))
        .map(|index| {
            let layer = index / per_layer;
            let row = (index % per_layer) / per_row;
            let column = index % per_row;
            Vec3::new(
                start + column as f32 * COAL_PITCH,
                COAL_SIZE / 2.0 + 0.02 + layer as f32 * COAL_PITCH,
                start + row as f32 * COAL_PITCH,
            )
        })
        .collect()
}

/// The hinge of the hatch in the frame of the ship. The hinge axis points to starboard, so
/// a positive angle swings the plate aft and down, open.
pub fn hatch_hinge(class: ShipClass) -> Transform {
    class.layout().furnace
        * Transform::from_xyz(0.0, MOUTH_BOTTOM, FURNACE_SIZE.z / 2.0 + HATCH_GAP)
            .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
}

/// The hinge of an engine lever at `mount`. The hinge axis points to port, so a positive
/// angle pushes the handle toward the bow.
fn lever_hinge(mount: Transform) -> Transform {
    mount
        * Transform::from_xyz(0.0, LEVER_HINGE_HEIGHT, 0.0)
            .with_rotation(Quat::from_rotation_y(-FRAC_PI_2))
}

pub fn throttle_hinge(class: ShipClass) -> Transform {
    lever_hinge(class.layout().throttle_lever)
}

pub fn ignition_hinge(class: ShipClass) -> Transform {
    lever_hinge(class.layout().ignition_lever)
}

/// The hatch starts shut, at the value 0, and opens to almost flat.
pub const HATCH_SPEC: LeverSpec = LeverSpec {
    length: HATCH_HEIGHT,
    angles: (0.0, 1.45),
    values: (0.0, 1.0),
    friction: 4.0,
};

/// The throttle starts at stop. The hinge angle equals the value.
pub const THROTTLE_SPEC: LeverSpec = LeverSpec {
    length: 0.8,
    angles: (-MAX_REVERSE, 1.0),
    values: (-MAX_REVERSE, 1.0),
    friction: 4.0,
};

/// The ignition lever starts at rest, at the value 0.
pub const IGNITION_SPEC: LeverSpec = LeverSpec {
    length: 0.8,
    angles: (0.0, 0.9),
    values: (0.0, 1.0),
    friction: 3.0,
};

/// The collider of the hatch: the plate along the handle axis. The handle of a lever is
/// along its local Y axis, and the hinge axis is its local Z axis.
fn hatch_collider() -> Collider {
    Collider::compound(vec![(
        Vec3::Y * HATCH_HEIGHT / 2.0,
        Quat::IDENTITY,
        Collider::cuboid(HATCH_THICKNESS, HATCH_HEIGHT, HATCH_WIDTH),
    )])
}

/// The furnace walls, the chimney and the bunker walls as one collider in the frame of the
/// ship.
fn housing_collider(class: ShipClass) -> Collider {
    let layout = class.layout();
    let furnace = layout.furnace;
    let bunker = layout.coal_bunker;
    let mut shapes: Vec<(Vec3, Quat, Collider)> = furnace_boxes()
        .into_iter()
        .map(|(center, size)| {
            (
                furnace.transform_point(center),
                furnace.rotation,
                Collider::cuboid(size.x, size.y, size.z),
            )
        })
        .collect();
    shapes.push((
        furnace.transform_point(chimney_center()),
        furnace.rotation,
        Collider::cylinder(CHIMNEY_RADIUS, CHIMNEY_HEIGHT),
    ));
    shapes.extend(bunker_boxes(class).into_iter().map(|(center, size)| {
        (
            bunker.transform_point(center),
            bunker.rotation,
            Collider::cuboid(size.x, size.y, size.z),
        )
    }));
    Collider::compound(shapes)
}

fn spawn_engine(
    add: On<Add, Ship>,
    mode: Res<NetMode>,
    ships: Query<(&ShipClass, &Transform)>,
    mut commands: Commands,
) {
    if !mode.is_authority() {
        return;
    }
    let ship = add.entity;
    let Ok((&class, &pose)) = ships.get(ship) else {
        return;
    };
    let layout = class.layout();

    let hatch = spawn_lever(&mut commands, ship, hatch_hinge(class), HATCH_SPEC);
    // The lever gets its own collider when the commands apply, and this one replaces it.
    commands.entity(hatch).try_insert((
        EnginePart::Hatch,
        hatch_collider(),
        ColliderDensity(HATCH_DENSITY),
    ));
    let throttle = spawn_lever(&mut commands, ship, throttle_hinge(class), THROTTLE_SPEC);
    commands.entity(throttle).insert(EnginePart::Throttle);
    let ignition = spawn_lever(&mut commands, ship, ignition_hinge(class), IGNITION_SPEC);
    commands
        .entity(ignition)
        .insert((EnginePart::Ignition, IgnitionArmed(true)));

    let (chamber, chamber_size) = fire_chamber();
    commands
        .entity(ship)
        .insert(Engine::default())
        .with_children(|parts| {
            // Without density, the housing does not change the buoyancy that the hull is tuned for.
            parts.spawn((
                housing_collider(class),
                ColliderDensity(0.0),
                Transform::IDENTITY,
            ));
            parts.spawn((
                FurnaceMouth { hatch },
                Sensor,
                Collider::cuboid(
                    chamber_size.x - 0.1,
                    chamber_size.y - 0.1,
                    chamber_size.z - 0.1,
                ),
                CollidingEntities::default(),
                layout.furnace * Transform::from_translation(chamber),
            ));
        });

    for position in coal_positions(class) {
        commands.spawn((
            coal_body(),
            PartOf(ship),
            pose * layout.coal_bunker * Transform::from_translation(position),
        ));
    }
}

/// The detent torque about the hinge axis of a hatch at lever `value`. Positive opens.
pub fn hatch_latch_torque(value: f32) -> f32 {
    if value >= HATCH_OPEN {
        HATCH_LATCH_TORQUE
    } else {
        -HATCH_LATCH_TORQUE
    }
}

fn latch_hatches(mut hatches: Query<(Forces, &EnginePart, &ControlValue)>) {
    for (mut forces, part, value) in &mut hatches {
        if *part != EnginePart::Hatch {
            continue;
        }
        let axis = forces.rotation().0 * Vec3::Z;
        forces.apply_torque(axis * hatch_latch_torque(value.0));
    }
}

/// Burns each lump that is inside an open furnace, if the furnace has room for it.
fn feed_furnace(
    mut commands: Commands,
    mouths: Query<(&FurnaceMouth, &ChildOf, &CollidingEntities)>,
    hatches: Query<&ControlValue>,
    coal: Query<(), With<Coal>>,
    mut engines: Query<&mut Engine>,
) {
    for (mouth, child_of, colliding) in &mouths {
        let open = hatches
            .get(mouth.hatch)
            .is_ok_and(|value| value.0 >= HATCH_OPEN);
        if !open {
            continue;
        }
        let Ok(mut engine) = engines.get_mut(child_of.parent()) else {
            continue;
        };
        for &lump in colliding.iter() {
            if !coal.contains(lump) {
                continue;
            }
            let Some(fuel) = add_lump(engine.fuel) else {
                break;
            };
            engine.fuel = fuel;
            commands.entity(lump).despawn();
        }
    }
}

fn read_engine_levers(
    mut levers: Query<(
        &EnginePart,
        &PartOf,
        &ControlValue,
        Option<&mut IgnitionArmed>,
    )>,
    mut engines: Query<&mut Engine>,
) {
    for (part, part_of, value, armed) in &mut levers {
        let Ok(mut engine) = engines.get_mut(part_of.0) else {
            continue;
        };
        match part {
            EnginePart::Hatch => {}
            EnginePart::Throttle => {
                let throttle = value.0.clamp(-MAX_REVERSE, 1.0);
                if engine.throttle != throttle {
                    engine.throttle = throttle;
                }
            }
            EnginePart::Ignition => {
                let Some(mut armed) = armed else {
                    continue;
                };
                let (fire, rearmed) = ignition_edge(value.0, armed.0);
                armed.set_if_neq(IgnitionArmed(rearmed));
                if fire && !engine.lit {
                    engine.ignite();
                }
            }
        }
    }
}

fn burn_fuel(time: Res<Time>, mut engines: Query<&mut Engine>) {
    let dt = time.delta_secs();
    for mut engine in &mut engines {
        if engine.lit {
            engine.step(dt);
        }
    }
}

/// Pushes each ship at the hubs of its propellers, along its bow.
fn drive_propellers(mut ships: Query<(Forces, &ShipClass, &Engine), With<Ship>>) {
    for (mut forces, &class, engine) in &mut ships {
        let thrust = propeller_thrust(class, engine);
        if thrust == 0.0 {
            continue;
        }
        let position = forces.position().0;
        let rotation = forces.rotation().0;
        let push = rotation * Vec3::NEG_Z * thrust;
        for hub in class.layout().propellers {
            forces.apply_force_at_point(push, position + rotation * hub.translation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(fuel: f32, throttle: f32) -> Engine {
        Engine {
            fuel,
            lit: true,
            throttle,
        }
    }

    #[test]
    fn one_lump_lasts_a_minute_at_full_throttle() {
        let mut engine = lit(FUEL_PER_LUMP, 1.0);
        for _ in 0..59 {
            engine.step(1.0);
        }
        assert!(engine.lit, "fire went out early with {}", engine.fuel);
        engine.step(1.0);
        assert!(!engine.lit);
        assert_eq!(engine.fuel, 0.0);
    }

    #[test]
    fn idle_fire_burns_slowly() {
        assert_eq!(burn_rate(0.0), IDLE_BURN);
        assert_eq!(burn_rate(1.0), 1.0);
        assert!(burn_rate(0.5) > burn_rate(0.2));
        assert_eq!(burn_rate(-MAX_REVERSE), burn_rate(MAX_REVERSE));
        assert_eq!(burn(1.0, 0.0, 2.0), 1.0 - 2.0 * IDLE_BURN);
    }

    #[test]
    fn burn_stops_at_zero() {
        assert_eq!(burn(0.5, 1.0, 3.0), 0.0);
    }

    #[test]
    fn cold_engine_keeps_its_fuel() {
        let mut engine = Engine {
            fuel: 10.0,
            lit: false,
            throttle: 1.0,
        };
        engine.step(5.0);
        assert_eq!(engine.fuel, 10.0);
    }

    #[test]
    fn lumps_fill_up_to_the_cap() {
        assert_eq!(add_lump(0.0), Some(FUEL_PER_LUMP));
        assert_eq!(add_lump(MAX_FUEL - 10.0), Some(MAX_FUEL));
        assert_eq!(add_lump(MAX_FUEL), None);
    }

    #[test]
    fn ignition_needs_fuel() {
        let mut empty = Engine::default();
        assert!(!empty.ignite());
        let mut fueled = Engine {
            fuel: 1.0,
            ..Engine::default()
        };
        assert!(fueled.ignite());
        assert!(fueled.lit);
    }

    #[test]
    fn ignition_fires_once_per_pull() {
        let (fire, armed) = ignition_edge(1.0, true);
        assert!(fire && !armed);
        // Held at the end, the lever does not fire again.
        assert_eq!(ignition_edge(1.0, false), (false, false));
        // Halfway back, it is not armed yet.
        assert_eq!(ignition_edge(0.6, false), (false, false));
        assert_eq!(ignition_edge(0.1, false), (false, true));
        assert_eq!(ignition_edge(0.6, true), (false, true));
    }

    #[test]
    fn thrust_follows_throttle_while_lit() {
        let class = ShipClass::A;
        assert_eq!(propeller_thrust(class, &lit(10.0, 1.0)), max_thrust(class));
        assert_eq!(
            propeller_thrust(class, &lit(10.0, 0.5)),
            max_thrust(class) / 2.0
        );
        assert!(propeller_thrust(class, &lit(10.0, -MAX_REVERSE)) < 0.0);
        assert_eq!(
            propeller_thrust(class, &lit(10.0, 3.0)),
            max_thrust(class),
            "throttle is capped"
        );
        let cold = Engine {
            fuel: 10.0,
            lit: false,
            throttle: 1.0,
        };
        assert_eq!(propeller_thrust(class, &cold), 0.0);
    }

    #[test]
    fn larger_classes_push_harder() {
        let total = |class: ShipClass| max_thrust(class) * class.layout().propellers.len() as f32;
        assert!(total(ShipClass::A) < total(ShipClass::B));
        assert!(total(ShipClass::B) < total(ShipClass::C));
    }

    #[test]
    fn coal_fits_the_bunker() {
        for class in ShipClass::ALL {
            let positions = coal_positions(class);
            assert_eq!(positions.len(), starting_coal(class));
            let inner = bunker_inner_size(class) / 2.0;
            for position in positions {
                assert!(
                    position.x.abs() + COAL_SIZE / 2.0 < inner
                        && position.z.abs() + COAL_SIZE / 2.0 < inner,
                    "{class:?} lump at {position}"
                );
                assert!(position.y + COAL_SIZE / 2.0 < BUNKER_WALL_HEIGHT);
            }
        }
    }

    #[test]
    fn engine_parts_stay_apart() {
        for class in ShipClass::ALL {
            let layout = class.layout();
            let furnace = layout.furnace.translation;
            let half_furnace = FURNACE_SIZE.x / 2.0;
            let bunker_reach = bunker_inner_size(class) / 2.0 + BUNKER_WALL_THICKNESS;
            let gap = (furnace.x - layout.coal_bunker.translation.x).abs();
            assert!(gap > half_furnace + bunker_reach, "{class:?} gap {gap}");
            let rail = layout.hull.x / 2.0 - 0.1;
            assert!(
                layout.coal_bunker.translation.x.abs() + bunker_reach < rail,
                "{class:?} bunker past the rail"
            );
            for lever in [layout.throttle_lever, layout.ignition_lever] {
                let side = (lever.translation.x - furnace.x).abs();
                assert!(side > half_furnace + 0.2, "{class:?} lever in the furnace");
            }
        }
    }

    #[test]
    fn hatch_latch_pushes_to_nearer_end() {
        assert!(hatch_latch_torque(0.0) < 0.0);
        assert!(hatch_latch_torque(0.4) < 0.0);
        assert!(hatch_latch_torque(0.6) > 0.0);
        assert!(hatch_latch_torque(1.0) > 0.0);
    }

    #[test]
    fn hatch_opens_aft() {
        let hinge = hatch_hinge(ShipClass::A);
        let open = hinge.rotation * Quat::from_rotation_z(HATCH_SPEC.angles.1) * Vec3::Y;
        assert!(open.z > 0.9, "open plate points along {open}");
    }

    #[test]
    fn levers_push_toward_bow() {
        let hinge = throttle_hinge(ShipClass::A);
        let pushed = hinge.rotation * Quat::from_rotation_z(0.5) * Vec3::Y;
        assert!(pushed.z < -0.4, "pushed handle points along {pushed}");
    }

    #[test]
    fn chamber_lies_inside_furnace() {
        let (center, size) = fire_chamber();
        let half = FURNACE_SIZE / 2.0;
        assert!(
            (center.z + size.z / 2.0 - half.z).abs() < 1e-5,
            "open at the aft face"
        );
        assert!(center.x.abs() + size.x / 2.0 <= half.x);
    }

    #[test]
    fn engine_state_survives_conversion() {
        let engine = lit(42.0, 0.5);
        assert_eq!(Engine::from(<(f32, bool, f32)>::from(engine)), engine);
        for part in [
            EnginePart::Hatch,
            EnginePart::Throttle,
            EnginePart::Ignition,
        ] {
            assert_eq!(EnginePart::from(u8::from(part)), part);
        }
    }
}
