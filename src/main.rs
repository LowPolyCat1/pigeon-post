use avian3d::prelude::*;
use bevy::prelude::*;
use pigeon_post::pigeon::{PigeonPlugin, read_keyboard};
use pigeon_post::test_level::TestLevelPlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PhysicsPlugins::default(),
            PigeonPlugin,
            TestLevelPlugin,
        ))
        .add_systems(Update, read_keyboard)
        .run()
}
