//! The starting scene: a floating island in the style of the Skylands, with the post office,
//! a pier for the skyship, and small islets in the sky around it.
//!
//! Every instance builds the scene itself. Nothing here moves, so nothing replicates.

use std::f32::consts::FRAC_PI_4;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::island_mesh::IslandShape;
use crate::sky::SUN_DIRECTION;

/// The outline of the main island stays inside this radius, and the turf lip overhangs it
/// by 3.5 %. Both stay clear of the skyship at its spawn point.
const MAIN_RADIUS: f32 = 10.0;
/// The spawn points of the pigeons and the crates assume flat grass at y = 0 inside this
/// radius.
const MAIN_FLAT_RADIUS: f32 = 7.5;

pub struct StartIslandPlugin;

impl Plugin for StartIslandPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.85, 0.9, 1.0),
            brightness: 500.0,
            ..default()
        })
        .add_systems(Startup, spawn_start_island);
    }
}

struct Palette {
    /// White, so the vertex colors of an island mesh show unchanged.
    terrain: Handle<StandardMaterial>,
    bark: Handle<StandardMaterial>,
    leaves: [Handle<StandardMaterial>; 2],
    stone: Handle<StandardMaterial>,
    wall: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    post_red: Handle<StandardMaterial>,
    dark: Handle<StandardMaterial>,
}

impl Palette {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let mut matte = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.9,
                ..default()
            })
        };
        Self {
            terrain: matte(Color::WHITE),
            bark: matte(Color::srgb(0.45, 0.30, 0.18)),
            leaves: [
                matte(Color::srgb(0.25, 0.62, 0.25)),
                matte(Color::srgb(0.40, 0.75, 0.28)),
            ],
            stone: matte(Color::srgb(0.66, 0.66, 0.70)),
            wall: matte(Color::srgb(0.95, 0.88, 0.70)),
            roof: matte(Color::srgb(0.20, 0.42, 0.75)),
            wood: matte(Color::srgb(0.68, 0.50, 0.30)),
            post_red: matte(Color::srgb(0.85, 0.15, 0.12)),
            dark: matte(Color::srgb(0.25, 0.18, 0.12)),
        }
    }
}

/// The scene builder: the asset stores and the palette, so the helpers take one argument.
struct Builder<'a, 'w, 's> {
    commands: Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    palette: Palette,
}

/// The shape of the main island. Its grass top is at y = 0 in the world.
pub fn main_island_shape() -> IslandShape {
    IslandShape {
        flat_radius: MAIN_FLAT_RADIUS,
        bump: 0.3,
        ..IslandShape::new(1, MAIN_RADIUS)
    }
}

/// One floating island. `top` is the center of the grass surface.
struct Island {
    top: Vec3,
    shape: IslandShape,
}

impl Island {
    fn new(seed: u32, top: Vec3, radius: f32) -> Self {
        Self {
            top,
            shape: IslandShape::new(seed, radius),
        }
    }

    /// The point on the grass above or below world `(x, z)`.
    fn ground(&self, x: f32, z: f32) -> Vec3 {
        let height = self.shape.top_height(x - self.top.x, z - self.top.z);
        Vec3::new(x, self.top.y + height, z)
    }
}

fn spawn_start_island(
    commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let palette = Palette::new(&mut materials);
    let mut builder = Builder {
        commands,
        meshes: &mut meshes,
        palette,
    };

    let main = Island {
        top: Vec3::ZERO,
        shape: main_island_shape(),
    };
    builder.island(&main)?;
    // A raised meadow on the far side. A pigeon can jump onto it.
    let meadow = Island::new(2, Vec3::new(-6.0, 0.8, -7.5), 4.5);
    builder.island(&meadow)?;

    builder.post_office(main.ground(-3.0, 6.0));
    builder.pier();
    builder.tree(main.ground(6.0, -5.0), 1.0);
    builder.tree(meadow.ground(-7.5, -8.5), 1.3);
    builder.tree(main.ground(-7.5, 2.5), 0.8);
    builder.tree(main.ground(4.0, 7.0), 1.1);
    builder.rock(main.ground(7.5, 3.5), 0.7);
    builder.rock(main.ground(-1.0, -7.0), 0.9);
    builder.rock(main.ground(2.2, -8.5), 0.5);

    // Islets in the sky. The near ones are in reach of a pigeon with a full stamina bar.
    let west = Island::new(3, Vec3::new(-19.0, 3.0, 11.0), 3.5);
    builder.island(&west)?;
    builder.tree(west.ground(-19.5, 11.5), 0.9);
    builder.island(&Island::new(4, Vec3::new(6.0, 5.0, -21.0), 2.5))?;
    let far = Island::new(5, Vec3::new(45.0, 2.0, -50.0), 9.0);
    builder.island(&far)?;
    builder.tree(far.ground(43.0, -48.0), 1.6);
    builder.island(&Island::new(6, Vec3::new(-55.0, 8.0, -30.0), 6.0))?;

    builder.commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            illuminance: 9000.0,
            ..default()
        },
        // The sun of the sky dome and the light of the cloud sea point the same way.
        Transform::from_translation(SUN_DIRECTION).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    Ok(())
}

impl Builder<'_, '_, '_> {
    /// A static body with one collider and one visible mesh.
    fn solid(
        &mut self,
        collider: Collider,
        mesh: Mesh,
        material: Handle<StandardMaterial>,
        transform: Transform,
    ) {
        let mesh = self.meshes.add(mesh);
        self.commands.spawn((
            RigidBody::Static,
            collider,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            transform,
        ));
    }

    /// A mesh without a collider.
    fn decoration(&mut self, mesh: Mesh, material: Handle<StandardMaterial>, transform: Transform) {
        let mesh = self.meshes.add(mesh);
        self.commands
            .spawn((Mesh3d(mesh), MeshMaterial3d(material), transform));
    }

    /// One island mesh with a triangle mesh collider of the same shape.
    fn island(&mut self, island: &Island) -> Result<(), String> {
        let collider = island.shape.collider()?;
        self.solid(
            collider,
            island.shape.mesh(),
            self.palette.terrain.clone(),
            Transform::from_translation(island.top),
        );
        Ok(())
    }

    /// A round, cartoon tree. `ground` is the foot of the trunk.
    fn tree(&mut self, ground: Vec3, scale: f32) {
        let trunk_height = 2.0 * scale;
        self.solid(
            Collider::cylinder(0.25 * scale, trunk_height),
            Cylinder::new(0.25 * scale, trunk_height)
                .mesh()
                .resolution(8)
                .into(),
            self.palette.bark.clone(),
            Transform::from_translation(ground + Vec3::Y * trunk_height / 2.0),
        );
        // A pigeon can land on the crown.
        for (index, (offset, radius)) in [
            (Vec3::new(0.0, 2.6, 0.0), 1.3),
            (Vec3::new(0.5, 3.4, 0.3), 0.85),
        ]
        .into_iter()
        .enumerate()
        {
            self.solid(
                Collider::sphere(radius * scale),
                Sphere::new(radius * scale)
                    .mesh()
                    .ico(1)
                    .unwrap_or_else(|_| Sphere::new(radius * scale).mesh().uv(12, 8)),
                self.palette.leaves[index].clone(),
                Transform::from_translation(ground + offset * scale),
            );
        }
    }

    /// A boulder, half sunk into the grass.
    fn rock(&mut self, ground: Vec3, radius: f32) {
        self.solid(
            Collider::sphere(radius),
            Sphere::new(radius)
                .mesh()
                .ico(0)
                .unwrap_or_else(|_| Sphere::new(radius).mesh().uv(8, 6)),
            self.palette.stone.clone(),
            Transform::from_translation(ground + Vec3::Y * radius * 0.3),
        );
    }

    /// The post office of the island: the place where a delivery job starts.
    fn post_office(&mut self, ground: Vec3) {
        let walls = Vec3::new(4.0, 2.6, 3.5);
        self.solid(
            Collider::cuboid(walls.x, walls.y, walls.z),
            Cuboid::from_size(walls).into(),
            self.palette.wall.clone(),
            Transform::from_translation(ground + Vec3::Y * walls.y / 2.0),
        );
        // A cone with four sides is a pyramid roof. Turned by 45°, its edges meet the walls.
        let roof_height = 1.8;
        self.solid(
            Collider::cone(3.2, roof_height),
            Cone {
                radius: 3.2,
                height: roof_height,
            }
            .mesh()
            .resolution(4)
            .into(),
            self.palette.roof.clone(),
            Transform::from_translation(ground + Vec3::Y * (walls.y + roof_height / 2.0))
                .with_rotation(Quat::from_rotation_y(FRAC_PI_4)),
        );
        self.decoration(
            Cuboid::new(1.0, 1.7, 0.1).into(),
            self.palette.dark.clone(),
            Transform::from_translation(ground + Vec3::new(0.0, 0.85, walls.z / 2.0 + 0.05)),
        );
        // A red post box in front of the door.
        let post_box = ground + Vec3::new(2.6, 0.0, walls.z / 2.0 + 1.0);
        self.solid(
            Collider::cylinder(0.3, 1.1),
            Cylinder::new(0.3, 1.1).mesh().resolution(12).into(),
            self.palette.post_red.clone(),
            Transform::from_translation(post_box + Vec3::Y * 0.55),
        );
        self.decoration(
            Sphere::new(0.3).mesh().uv(12, 6),
            self.palette.post_red.clone(),
            Transform::from_translation(post_box + Vec3::Y * 1.1),
        );
    }

    /// A wooden pier on the east edge, toward the skyship. Its end stays clear of the hull.
    fn pier(&mut self) {
        let plank = Vec3::new(2.4, 0.2, 2.2);
        let top = -0.3;
        let center = Vec3::new(9.3, top - plank.y / 2.0, 0.0);
        self.solid(
            Collider::cuboid(plank.x, plank.y, plank.z),
            Cuboid::from_size(plank).into(),
            self.palette.wood.clone(),
            Transform::from_translation(center),
        );
        // The posts reach down into the cloud sea.
        let post_length = 3.5;
        for z in [-0.9, 0.9] {
            self.decoration(
                Cylinder::new(0.12, post_length).mesh().resolution(6).into(),
                self.palette.wood.clone(),
                Transform::from_xyz(10.3, top - post_length / 2.0, z),
            );
        }
    }
}
