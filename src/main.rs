use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::RepliconRenetPlugins;
use pigeon_post::cloud_sea::CloudSeaPlugin;
use pigeon_post::hud::HudPlugin;
use pigeon_post::net::{NetMode, NetPlugin, parse_args, start_network, unix_time};
use pigeon_post::pigeon::{PigeonPlugin, read_keyboard};
use pigeon_post::test_level::TestLevelPlugin;

fn main() -> AppExit {
    let mode = match net_mode() {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("{error}");
            return AppExit::error();
        }
    };
    App::new()
        .insert_resource(mode)
        .add_plugins((
            DefaultPlugins,
            PhysicsPlugins::default(),
            RepliconPlugins,
            RepliconRenetPlugins,
            PigeonPlugin,
            CloudSeaPlugin,
            NetPlugin,
            TestLevelPlugin,
            HudPlugin,
        ))
        .add_systems(Startup, start_network)
        .add_systems(Update, read_keyboard)
        .run()
}

fn net_mode() -> Result<NetMode, String> {
    // Milliseconds since 1970 differ between two clients that start on one LAN.
    let client_id = unix_time()?.as_millis() as u64;
    parse_args(std::env::args().skip(1), client_id)
}
