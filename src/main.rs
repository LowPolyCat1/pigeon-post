use avian3d::prelude::*;
use bevy::prelude::*;
use pigeon_post::cloud_sea::CloudSeaPlugin;
use pigeon_post::hud::HudPlugin;
use pigeon_post::pigeon::{PigeonPlugin, read_keyboard};
use pigeon_post::test_level::TestLevelPlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PhysicsPlugins::default(),
            PigeonPlugin,
            CloudSeaPlugin,
            TestLevelPlugin,
            HudPlugin,
        ))
        .add_systems(Update, read_keyboard)
        .run()
}
