//! The sky: a dome with a gradient, a sun and toon clouds, and fog in the horizon color.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    DistanceFog, FogFalloff, Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin,
};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

/// The fog, the lower sky and the far cloud sea share this color, so they meet without a seam.
pub const HORIZON: Color = Color::srgb(0.72, 0.85, 0.97);
const ZENITH: Color = Color::srgb(0.30, 0.58, 0.93);
/// Toward the sun. The directional light of the scene shines from here.
pub const SUN_DIRECTION: Vec3 = Vec3::new(4.0, 10.0, 6.0);
pub const FOG_START: f32 = 40.0;
pub const FOG_END: f32 = 140.0;
/// Inside the far plane of the default camera, which is 1000 m.
const DOME_RADIUS: f32 = 800.0;
const SHADER: &str = "shaders/sky.wgsl";
const NOISE_SHADER: &str = "shaders/noise.wgsl";

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<SkyMaterial>::default())
            .insert_resource(ClearColor(HORIZON))
            .add_systems(Startup, load_noise)
            .add_systems(Update, update_sky_time)
            .add_observer(add_sky);
    }
}

/// Keeps the noise library loaded. The sky and the cloud sea shaders import it by name.
#[derive(Resource)]
struct NoiseShader(#[expect(dead_code, reason = "only the handle keeps the asset")] Handle<Shader>);

#[derive(ShaderType, Debug, Clone, Copy)]
struct SkyUniform {
    zenith: Vec4,
    horizon: Vec4,
    sun: Vec4,
    time: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SkyMaterial {
    #[uniform(0)]
    sky: SkyUniform,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    /// The camera is inside the dome, so the inner faces must draw.
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

pub fn linear(color: Color) -> Vec4 {
    color.to_linear().to_vec4()
}

fn load_noise(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(NoiseShader(assets.load(NOISE_SHADER)));
}

/// The dome is a child of the camera, so it never comes closer. The shader colors each pixel
/// by its direction from the camera, so a turn of the camera does not move the sky.
fn add_sky(
    add: On<Add, Camera3d>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    let dome = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(DOME_RADIUS).mesh().uv(32, 18))),
            MeshMaterial3d(materials.add(SkyMaterial {
                sky: SkyUniform {
                    zenith: linear(ZENITH),
                    horizon: linear(HORIZON),
                    sun: SUN_DIRECTION.normalize().extend(0.0),
                    time: 0.0,
                },
            })),
            NotShadowCaster,
            NoFrustumCulling,
        ))
        .id();
    commands
        .entity(add.entity)
        .insert(DistanceFog {
            color: HORIZON,
            falloff: FogFalloff::Linear {
                start: FOG_START,
                end: FOG_END,
            },
            ..default()
        })
        .add_child(dome);
}

fn update_sky_time(time: Res<Time>, mut materials: ResMut<Assets<SkyMaterial>>) {
    for (_, material) in materials.iter_mut() {
        material.sky.time = time.elapsed_secs();
    }
}
