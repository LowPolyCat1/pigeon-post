//! The picture of the cloud sea. Each instance draws the waves at the clock of the host, so a
//! client sees the ship ride the same crest that the host simulates.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy_replicon::prelude::*;

use crate::cloud_sea::SEA_LEVEL;
use crate::net::NetMode;
use crate::waves::{WaveUniform, wave_uniform};

const SHADER: &str = "shaders/cloud_sea.wgsl";
/// Larger than the view distance of the test level, so the edge stays out of sight.
const SEA_SIZE: f32 = 240.0;
/// 1.5 m between vertices. The shortest wave is 7 m long, so it keeps its shape.
const SEA_SUBDIVISIONS: u32 = 160;

pub struct CloudSeaRenderPlugin;

impl Plugin for CloudSeaRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<CloudSeaMaterial>::default())
            .replicate_as::<CloudSea, ()>()
            .replicate_as::<SeaClock, f32>()
            .init_resource::<SeaTime>()
            .add_observer(add_sea_mesh)
            .add_systems(
                Startup,
                spawn_cloud_sea.run_if(|mode: Res<NetMode>| mode.is_authority()),
            )
            .add_systems(
                FixedUpdate,
                tick_clock.run_if(|mode: Res<NetMode>| mode.is_authority()),
            )
            .add_systems(Update, (update_sea_time, update_material).chain());
    }
}

/// The one entity that carries the sea mesh and the clock of the host.
#[derive(Component, Debug, Clone, Copy)]
#[require(SeaClock)]
pub struct CloudSea;

/// The wave time of the host, in seconds. The host writes it each fixed step.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct SeaClock(pub f32);

/// The wave time that this instance draws.
#[derive(Resource, Debug, Default)]
struct SeaTime {
    /// The host clock minus the local clock. Zero on the authority.
    offset: f32,
    now: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CloudSeaMaterial {
    #[uniform(0)]
    waves: WaveUniform,
}

impl Material for CloudSeaMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

// Replicon needs a serde type. The game has no serde dependency.
impl From<CloudSea> for () {
    fn from(_: CloudSea) -> Self {}
}

impl From<()> for CloudSea {
    fn from(_: ()) -> Self {
        CloudSea
    }
}

impl From<SeaClock> for f32 {
    fn from(clock: SeaClock) -> Self {
        clock.0
    }
}

impl From<f32> for SeaClock {
    fn from(seconds: f32) -> Self {
        SeaClock(seconds)
    }
}

fn spawn_cloud_sea(mut commands: Commands) {
    commands.spawn((CloudSea, Replicated));
}

/// The gameplay reads the fixed clock, so the clock that clients copy is the fixed clock too.
fn tick_clock(time: Res<Time>, mut clock: Single<&mut SeaClock>) {
    clock.0 = time.elapsed_secs();
}

fn update_sea_time(
    time: Res<Time>,
    mode: Res<NetMode>,
    clock: Option<Single<&SeaClock, Changed<SeaClock>>>,
    mut sea_time: ResMut<SeaTime>,
) {
    let local = time.elapsed_secs();
    // A new value arrives each host tick. The latency of one packet is too small to see.
    if let Some(clock) = clock
        && !mode.is_authority()
    {
        sea_time.offset = clock.0 - local;
    }
    sea_time.now = local + sea_time.offset;
}

fn add_sea_mesh(
    add: On<Add, CloudSea>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CloudSeaMaterial>>,
) {
    commands.entity(add.entity).insert((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(SEA_SIZE, SEA_SIZE)
                    .subdivisions(SEA_SUBDIVISIONS),
            ),
        ),
        MeshMaterial3d(materials.add(CloudSeaMaterial {
            waves: wave_uniform(0.0, SEA_LEVEL),
        })),
        Transform::default(),
        // The shader moves the vertices, so the bounds of the flat plane are wrong.
        NoFrustumCulling,
        // The shadow pass draws the flat plane, not the waves.
        NotShadowCaster,
    ));
}

fn update_material(
    sea_time: Res<SeaTime>,
    sea: Option<Single<&MeshMaterial3d<CloudSeaMaterial>>>,
    mut materials: ResMut<Assets<CloudSeaMaterial>>,
) {
    let Some(sea) = sea else {
        return;
    };
    if let Some(mut material) = materials.get_mut(&sea.0) {
        material.waves.time = sea_time.now;
    }
}
