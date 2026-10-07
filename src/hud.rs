//! On-screen stamina bar of the local pigeon.

use bevy::prelude::*;

use crate::pigeon::Pigeon;
use crate::stamina::Stamina;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_stamina_bar)
            .add_systems(Update, update_stamina_bar);
    }
}

#[derive(Component)]
struct StaminaFill;

fn spawn_stamina_bar(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(16.0),
                width: Val::Px(200.0),
                height: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        ))
        .with_child((
            StaminaFill,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.95, 0.85, 0.3)),
        ));
}

fn update_stamina_bar(
    stamina: Single<&Stamina, With<Pigeon>>,
    mut fill: Single<&mut Node, With<StaminaFill>>,
) {
    fill.width = Val::Percent(stamina.value() * 100.0);
}
