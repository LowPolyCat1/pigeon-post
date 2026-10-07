//! The first-person view: the camera sits in the head of the local pigeon, and the mouse turns it.

use std::f32::consts::FRAC_PI_2;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::net::LocalPigeon;
use crate::pigeon::CAPSULE_LENGTH;

/// Above the center of the capsule, near the top cap.
const EYE_HEIGHT: f32 = CAPSULE_LENGTH / 2.0 + 0.15;
/// Radians per pixel of mouse motion.
const MOUSE_SENSITIVITY: f32 = 0.0025;
/// A bit less than straight up or down, so the view never flips over.
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.05;

pub struct FirstPersonPlugin;

impl Plugin for FirstPersonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookAngles>()
            .add_systems(Startup, grab_cursor)
            .add_systems(Update, (toggle_cursor, turn_view).chain())
            .add_systems(PostUpdate, place_camera.before(TransformSystems::Propagate))
            .add_observer(hide_own_pigeon);
    }
}

/// Where the local player looks. Yaw 0 looks along -Z. A positive yaw turns left.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct LookAngles {
    pub yaw: f32,
    pub pitch: f32,
}

/// Turns `input` (x right, y forward, as the player sees it) into a direction on the world
/// XZ plane, in the form of [`crate::pigeon::PigeonInput::movement`]: x along +X, y along -Z.
pub fn view_to_world(input: Vec2, yaw: f32) -> Vec2 {
    let forward = Vec2::new(-yaw.sin(), yaw.cos());
    let right = Vec2::new(yaw.cos(), yaw.sin());
    right * input.x + forward * input.y
}

/// Applies `delta` mouse pixels to `angles`.
fn turn(angles: LookAngles, delta: Vec2) -> LookAngles {
    LookAngles {
        yaw: angles.yaw - delta.x * MOUSE_SENSITIVITY,
        pitch: (angles.pitch - delta.y * MOUSE_SENSITIVITY).clamp(-PITCH_LIMIT, PITCH_LIMIT),
    }
}

fn set_grab(cursor: &mut CursorOptions, grab: bool) {
    cursor.grab_mode = if grab {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    cursor.visible = !grab;
}

fn grab_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    set_grab(&mut cursor, true);
}

/// Escape frees the cursor, for example to leave the window. A click takes it back.
fn toggle_cursor(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        set_grab(&mut cursor, false);
    } else if buttons.just_pressed(MouseButton::Left) {
        set_grab(&mut cursor, true);
    }
}

fn turn_view(
    motion: Res<AccumulatedMouseMotion>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut angles: ResMut<LookAngles>,
) {
    // A free cursor moves over the desktop, not the view.
    if cursor.grab_mode == CursorGrabMode::None {
        return;
    }
    *angles = turn(*angles, motion.delta);
}

fn place_camera(
    angles: Res<LookAngles>,
    pigeon: Single<&Transform, With<LocalPigeon>>,
    mut camera: Single<&mut Transform, (With<Camera3d>, Without<LocalPigeon>)>,
) {
    camera.translation = pigeon.translation + Vec3::Y * EYE_HEIGHT;
    camera.rotation = Quat::from_euler(EulerRot::YXZ, angles.yaw, angles.pitch, 0.0);
}

/// The camera is inside the own pigeon, so its mesh would cover the view.
fn hide_own_pigeon(add: On<Add, LocalPigeon>, mut commands: Commands) {
    commands.entity(add.entity).insert(Visibility::Hidden);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn forward_without_turn_is_minus_z() {
        assert!(close(view_to_world(Vec2::Y, 0.0), Vec2::Y));
        assert!(close(view_to_world(Vec2::X, 0.0), Vec2::X));
    }

    #[test]
    fn quarter_turn_left_looks_along_minus_x() {
        let forward = view_to_world(Vec2::Y, FRAC_PI_2);
        assert!(close(forward, Vec2::new(-1.0, 0.0)), "{forward}");
    }

    #[test]
    fn camera_forward_matches_movement() {
        // The camera at this yaw looks where the movement of "forward" goes.
        let yaw = 0.7;
        let look = Quat::from_euler(EulerRot::YXZ, yaw, 0.0, 0.0) * Vec3::NEG_Z;
        let movement = view_to_world(Vec2::Y, yaw);
        assert!(
            close(movement, Vec2::new(look.x, -look.z)),
            "{movement} vs {look}"
        );
    }

    #[test]
    fn mouse_right_turns_right() {
        let angles = turn(LookAngles::default(), Vec2::new(100.0, 0.0));
        assert!(angles.yaw < 0.0);
    }

    #[test]
    fn pitch_stops_before_straight_up() {
        let angles = turn(LookAngles::default(), Vec2::new(0.0, -1.0e6));
        assert_eq!(angles.pitch, PITCH_LIMIT);
    }
}
