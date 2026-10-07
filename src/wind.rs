//! The wind and its picture: thin white streaks that drift with the wind, curl, and fade.
//! Each instance draws its own streaks. Only the wind is the same on all instances.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::cloud_sea::SEA_LEVEL;

/// The number of streaks in the air at one time.
const STREAK_COUNT: u32 = 24;
/// The seconds of path that one trail shows behind its head.
const TRAIL_TIME: f32 = 1.1;
const TRAIL_POINTS: usize = 32;
/// The streak at full opacity is still a little transparent, so it reads as air, not as paint.
const MAX_ALPHA: f32 = 0.85;
/// In pixels, for every distance. With perspective, near streaks grew into heavy ribbons.
const LINE_WIDTH: f32 = 3.0;
/// A streak never comes nearer than this to the cloud sea.
const SEA_CLEARANCE: f32 = 1.5;

pub struct WindLinesPlugin;

impl Plugin for WindLinesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wind>()
            .init_resource::<Streaks>()
            .insert_gizmo_config(
                WindGizmos,
                GizmoConfig {
                    line: GizmoLineConfig {
                        width: LINE_WIDTH,
                        // Joints overlap their segments, and the overlap shows as bright ticks
                        // on a transparent line. Short segments hide the gaps between them.
                        joints: GizmoLineJoint::None,
                        ..default()
                    },
                    ..default()
                },
            )
            .add_systems(Update, (renew_streaks, draw_streaks).chain());
    }
}

/// The wind of the world. The value is a constant, so every instance has the same wind.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Wind {
    /// A unit vector on the XZ plane: x along world +X, y along world +Z.
    pub direction: Vec2,
    /// The speed of the air in meters per second.
    pub strength: f32,
}

impl Default for Wind {
    /// A gentle breeze.
    fn default() -> Self {
        Self {
            direction: Vec2::new(1.0, -0.4).normalize(),
            strength: 4.0,
        }
    }
}

impl Wind {
    pub fn velocity(&self) -> Vec3 {
        Vec3::new(self.direction.x, 0.0, self.direction.y) * self.strength
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
struct WindGizmos;

/// A small loop that a streak flies once, up and back over its own path.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Loop {
    /// The age in seconds at the start of the loop.
    start: f32,
    duration: f32,
    radius: f32,
}

/// One streak. Its path is a function of its age, so it keeps no history of points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Streak {
    generation: u32,
    origin: Vec3,
    /// The time of the birth on the clock of [`Time`].
    birth: f32,
    lifetime: f32,
    /// x: sideways, y: vertical. A sine on each axis makes the curl.
    curl_amplitude: Vec2,
    curl_frequency: Vec2,
    curl_phase: Vec2,
    looping: Option<Loop>,
}

#[derive(Resource, Debug, Default)]
struct Streaks(Vec<Streak>);

/// A hash of `seed` and `salt` to a value in [0, 1). The same input gives the same value on
/// each run, so the streaks of one instance repeat, but there is no visible pattern.
fn random(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// 0 at `x` = 0 and at `x` = 1, and 1 between `fade_in` and `1 - fade_out`. Smooth at the edges.
fn fade(x: f32, fade_in: f32, fade_out: f32) -> f32 {
    let rise = if fade_in > 0.0 {
        smoothstep(x / fade_in)
    } else {
        1.0
    };
    let fall = if fade_out > 0.0 {
        smoothstep((1.0 - x) / fade_out)
    } else {
        1.0
    };
    rise * fall
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// A new streak somewhere around `center`, which is the camera. The streak flies through the
/// area near its spawn point, so its origin lies half a flight upwind.
fn new_streak(index: u32, generation: u32, center: Vec3, wind: &Wind, now: f32) -> Streak {
    let seed = index
        .wrapping_mul(7919)
        .wrapping_add(generation.wrapping_mul(104_729));
    let r = |salt| random(seed, salt);

    let lifetime = 3.0 + 2.0 * r(1);
    let angle = TAU * r(2);
    let distance = 5.0 + 30.0 * r(3);
    let height = -1.5 + 9.0 * r(4) * r(4).sqrt();
    let mut spawn = center + Vec3::new(angle.cos(), 0.0, angle.sin()) * distance;
    spawn.y = (center.y + height).max(SEA_LEVEL + SEA_CLEARANCE);
    let origin = spawn - wind.velocity() * (lifetime / 2.0);

    let looping = (r(5) < 0.35).then(|| Loop {
        start: lifetime * (0.3 + 0.3 * r(6)),
        duration: 0.9 + 0.4 * r(7),
        radius: 0.6 + 0.5 * r(8),
    });

    Streak {
        generation,
        origin,
        birth: now,
        lifetime,
        curl_amplitude: Vec2::new(0.3 + 0.4 * r(9), 0.15 + 0.3 * r(10)),
        curl_frequency: Vec2::new(2.5 + 2.0 * r(11), 1.5 + 2.0 * r(12)),
        curl_phase: Vec2::new(TAU * r(13), TAU * r(14)),
        looping,
    }
}

/// The position of `streak` at `age` seconds after its birth.
fn streak_position(streak: &Streak, wind: &Wind, age: f32) -> Vec3 {
    let forward = Vec3::new(wind.direction.x, 0.0, wind.direction.y);
    let side = Vec3::Y.cross(forward);
    let curl = streak.curl_amplitude
        * Vec2::new(
            (streak.curl_frequency.x * age + streak.curl_phase.x).sin(),
            (streak.curl_frequency.y * age + streak.curl_phase.y).sin(),
        );
    let mut position = streak.origin + wind.velocity() * age + side * curl.x + Vec3::Y * curl.y;
    if let Some(looping) = streak.looping {
        // The smoothstep starts and ends the turn at zero angular speed, so the path has no kink.
        let turn = TAU * smoothstep((age - looping.start) / looping.duration);
        position += (forward * turn.sin() + Vec3::Y * (1.0 - turn.cos())) * looping.radius;
    }
    position
}

/// Replaces each streak at the end of its life with a new one near the camera.
fn renew_streaks(
    time: Res<Time>,
    wind: Res<Wind>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut streaks: ResMut<Streaks>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let center = camera.translation();
    let now = time.elapsed_secs();
    if streaks.0.is_empty() {
        // Births in the future spread the streaks over time, so they do not all fade together.
        streaks.0 = (0..STREAK_COUNT)
            .map(|index| {
                let delay = 4.0 * index as f32 / STREAK_COUNT as f32;
                new_streak(index, 0, center, &wind, now + delay)
            })
            .collect();
        return;
    }
    for (index, streak) in (0..).zip(streaks.0.iter_mut()) {
        if now - streak.birth > streak.lifetime {
            *streak = new_streak(index, streak.generation + 1, center, &wind, now);
        }
    }
}

fn draw_streaks(
    time: Res<Time>,
    wind: Res<Wind>,
    streaks: Res<Streaks>,
    mut gizmos: Gizmos<WindGizmos>,
) {
    let now = time.elapsed_secs();
    for streak in &streaks.0 {
        let age = now - streak.birth;
        if age < 0.0 {
            continue;
        }
        let life = fade(age / streak.lifetime, 0.2, 0.3);
        gizmos.linestrip_gradient((0..=TRAIL_POINTS).map(|k| {
            // u is 0 at the tail and 1 at the head. The tail fades over a longer part.
            let u = k as f32 / TRAIL_POINTS as f32;
            let point = streak_position(streak, &wind, age - TRAIL_TIME * (1.0 - u));
            let alpha = MAX_ALPHA * life * fade(u, 0.6, 0.15);
            (point, Color::srgba(1.0, 1.0, 1.0, alpha))
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streak() -> Streak {
        new_streak(3, 7, Vec3::new(1.0, 2.0, 3.0), &Wind::default(), 10.0)
    }

    #[test]
    fn fade_is_zero_at_the_ends() {
        assert_eq!(fade(0.0, 0.2, 0.3), 0.0);
        assert_eq!(fade(1.0, 0.2, 0.3), 0.0);
    }

    #[test]
    fn fade_is_one_in_the_middle() {
        assert_eq!(fade(0.5, 0.2, 0.3), 1.0);
        assert_eq!(fade(0.5, 0.5, 0.5), 1.0);
    }

    #[test]
    fn fade_rises_monotonically() {
        let mut last = 0.0;
        for k in 0..=20 {
            let value = fade(k as f32 * 0.01, 0.2, 0.3);
            assert!(value >= last, "{value} < {last}");
            last = value;
        }
    }

    #[test]
    fn path_moves_along_the_wind() {
        let wind = Wind::default();
        let mut streak = streak();
        streak.looping = None;
        let start = streak_position(&streak, &wind, 0.0);
        let end = streak_position(&streak, &wind, 4.0);
        let along = (end - start).dot(wind.velocity().normalize());
        // The curl is less than 1 m, so the drift of 16 m dominates.
        assert!(along > 14.0 && along < 18.0, "{along}");
        let across = (end - start).reject_from(wind.velocity()).length();
        assert!(across < 3.0, "{across}");
    }

    #[test]
    fn loop_returns_to_the_plain_path() {
        let wind = Wind::default();
        let mut with_loop = streak();
        let looping = Loop {
            start: 1.0,
            duration: 1.0,
            radius: 0.8,
        };
        with_loop.looping = Some(looping);
        let mut plain = with_loop;
        plain.looping = None;
        let after = 2.5;
        let delta =
            streak_position(&with_loop, &wind, after) - streak_position(&plain, &wind, after);
        assert!(delta.length() < 1e-4, "{delta}");
        let middle = streak_position(&with_loop, &wind, 1.5) - streak_position(&plain, &wind, 1.5);
        assert!((middle.y - 2.0 * looping.radius).abs() < 1e-4, "{middle}");
    }

    #[test]
    fn streaks_are_deterministic() {
        assert_eq!(streak(), streak());
        let wind = Wind::default();
        assert_eq!(
            streak_position(&streak(), &wind, 1.3),
            streak_position(&streak(), &wind, 1.3)
        );
    }

    #[test]
    fn generations_differ() {
        let wind = Wind::default();
        let a = new_streak(3, 7, Vec3::ZERO, &wind, 0.0);
        let b = new_streak(3, 8, Vec3::ZERO, &wind, 0.0);
        assert_ne!(a.origin, b.origin);
    }

    #[test]
    fn streaks_stay_above_the_sea() {
        let wind = Wind::default();
        let low_camera = Vec3::new(0.0, SEA_LEVEL, 0.0);
        for index in 0..STREAK_COUNT {
            for generation in 0..50 {
                let streak = new_streak(index, generation, low_camera, &wind, 0.0);
                assert!(streak.origin.y >= SEA_LEVEL + SEA_CLEARANCE);
            }
        }
    }

    #[test]
    fn random_is_in_unit_range() {
        for seed in 0..1000 {
            let value = random(seed, seed ^ 0x55);
            assert!((0.0..1.0).contains(&value), "{value}");
        }
    }
}
