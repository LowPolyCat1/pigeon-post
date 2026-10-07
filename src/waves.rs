//! Gerstner waves of the cloud sea.
//!
//! `assets/shaders/cloud_sea.wgsl` computes the same displacement from the numbers that
//! [`wave_uniform`] gives it, so the picture and the gameplay agree on the surface.

use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

const GRAVITY: f32 = 9.81;
/// Fixed-point steps in [`surface_height`]. The horizontal displacement shrinks the error by at
/// least half each step, so four steps leave a few centimeters at most.
const HEIGHT_STEPS: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct GerstnerWave {
    /// Travel direction on the XZ plane. A unit vector.
    pub direction: Vec2,
    pub wavelength: f32,
    pub amplitude: f32,
    /// 0 is a sine wave, 1 is the sharpest crest before the surface folds over itself.
    pub steepness: f32,
}

/// A slow swell with a few smaller waves across it. The directions differ, so the pattern does
/// not line up into rows.
pub const WAVES: [GerstnerWave; 4] = [
    GerstnerWave {
        direction: Vec2::new(1.0, 0.0),
        wavelength: 30.0,
        amplitude: 0.35,
        steepness: 0.5,
    },
    GerstnerWave {
        direction: Vec2::new(0.6, 0.8),
        wavelength: 18.0,
        amplitude: 0.2,
        steepness: 0.5,
    },
    GerstnerWave {
        direction: Vec2::new(-0.8, 0.6),
        wavelength: 11.0,
        amplitude: 0.12,
        steepness: 0.4,
    },
    GerstnerWave {
        direction: Vec2::new(0.28, -0.96),
        wavelength: 7.0,
        amplitude: 0.06,
        steepness: 0.3,
    },
];

impl GerstnerWave {
    fn wave_number(self) -> f32 {
        TAU / self.wavelength
    }

    /// Deep-water dispersion: a long wave travels faster than a short one.
    fn angular_speed(self) -> f32 {
        (GRAVITY * self.wave_number()).sqrt()
    }

    /// The sideways travel of a surface point. Divided by the wave count, so the sum of all
    /// waves stays below the fold limit when each steepness is at most 1.
    fn horizontal_amplitude(self) -> f32 {
        self.steepness / (self.wave_number() * WAVES.len() as f32)
    }
}

/// Where the surface point that rests at `rest` is at `time`, relative to its rest position on
/// the sea level.
pub fn displacement(rest: Vec2, time: f32) -> Vec3 {
    WAVES.iter().fold(Vec3::ZERO, |sum, wave| {
        let phase = wave.wave_number() * wave.direction.dot(rest) - wave.angular_speed() * time;
        let sideways = wave.direction * wave.horizontal_amplitude() * phase.cos();
        sum + Vec3::new(sideways.x, wave.amplitude * phase.sin(), sideways.y)
    })
}

/// The height of the surface above the sea level, straight above the world point `xz`.
///
/// Gerstner waves move surface points sideways, so the point that rests at `xz` is not the
/// point above `xz`. Each step moves the rest position against the displacement.
pub fn surface_height(xz: Vec2, time: f32) -> f32 {
    let mut rest = xz;
    for _ in 0..HEIGHT_STEPS {
        let moved = displacement(rest, time);
        rest = xz - Vec2::new(moved.x, moved.z);
    }
    displacement(rest, time).y
}

/// The waves as the shader reads them.
#[derive(ShaderType, Debug, Clone, Copy)]
pub struct WaveUniform {
    /// Per wave: direction x, direction z, wave number, angular speed.
    pub motion: [Vec4; 4],
    /// Per wave: amplitude, horizontal amplitude. The last two values are unused.
    pub size: [Vec4; 4],
    pub time: f32,
    pub sea_level: f32,
}

pub fn wave_uniform(time: f32, sea_level: f32) -> WaveUniform {
    WaveUniform {
        motion: WAVES.map(|wave| {
            Vec4::new(
                wave.direction.x,
                wave.direction.y,
                wave.wave_number(),
                wave.angular_speed(),
            )
        }),
        size: WAVES.map(|wave| Vec4::new(wave.amplitude, wave.horizontal_amplitude(), 0.0, 0.0)),
        time,
        sea_level,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POINTS: [Vec2; 4] = [
        Vec2::new(0.0, 0.0),
        Vec2::new(3.5, -7.0),
        Vec2::new(-12.0, 4.25),
        Vec2::new(40.0, 40.0),
    ];

    #[test]
    fn directions_are_unit_vectors() {
        for wave in WAVES {
            assert!((wave.direction.length() - 1.0).abs() < 1e-3, "{wave:?}");
        }
    }

    #[test]
    fn surface_stays_within_amplitude() {
        let limit: f32 = WAVES.iter().map(|wave| wave.amplitude).sum();
        for point in POINTS {
            for step in 0..20 {
                let height = surface_height(point, step as f32 * 0.37);
                assert!(height.abs() <= limit, "height {height} at {point}");
            }
        }
    }

    #[test]
    fn surface_height_is_above_query_point() {
        for point in POINTS {
            let time = 2.5;
            let height = surface_height(point, time);
            // Find the rest position by brute force: the displaced point must lie above `point`.
            let mut rest = point;
            for _ in 0..50 {
                let moved = displacement(rest, time);
                rest = point - Vec2::new(moved.x, moved.z);
            }
            let exact = displacement(rest, time).y;
            assert!(
                (height - exact).abs() < 0.02,
                "{height} vs {exact} at {point}"
            );
        }
    }

    #[test]
    fn surface_moves_over_time() {
        assert!((surface_height(Vec2::ZERO, 0.0) - surface_height(Vec2::ZERO, 1.0)).abs() > 0.01);
    }

    #[test]
    fn uniform_holds_every_wave() {
        let uniform = wave_uniform(3.0, -2.5);
        for (index, wave) in WAVES.iter().enumerate() {
            assert_eq!(uniform.motion[index].x, wave.direction.x);
            assert_eq!(uniform.size[index].x, wave.amplitude);
        }
        assert_eq!(uniform.time, 3.0);
    }
}
