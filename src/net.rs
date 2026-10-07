//! Host and client over UDP on a LAN. The host simulates every pigeon. A client sends its
//! input to the host and shows the state that the host replicates.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime};

use bevy::prelude::*;
use bevy_replicon::bytes::Bytes;
use bevy_replicon::postcard_utils;
use bevy_replicon::prelude::*;
use bevy_replicon::shared::backend::connected_client::NetworkId;
use bevy_replicon::shared::message::ctx::{ClientSendCtx, ServerReceiveCtx};
use bevy_replicon::shared::replication::registry::ctx::{SerializeCtx, WriteCtx};
use bevy_replicon_renet::netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeErrorEvent, NetcodeServerTransport,
    ServerAuthentication, ServerConfig,
};
use bevy_replicon_renet::renet::ConnectionConfig;
use bevy_replicon_renet::{RenetChannelsExt, RenetClient, RenetServer};

use crate::pigeon::{InputMessage, Pigeon, PigeonInput, pigeon_body};
use crate::stamina::Stamina;

pub const DEFAULT_PORT: u16 = 5000;
/// Replicon compares a hash of all registrations after the connection, so one fixed id is enough.
const PROTOCOL_ID: u64 = 0;
/// Eight pigeons with the host. The test island has room for this many.
const MAX_CLIENTS: usize = 7;
const SPAWN_HEIGHT: f32 = 2.0;
/// Wider than a pigeon, so two new pigeons do not overlap.
const SPAWN_SPACING: f32 = 1.0;

/// How this instance takes part in a game.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NetMode {
    /// No network. The instance is its own authority.
    #[default]
    Single,
    /// A listen server: the authority, and one of the players.
    Host {
        port: u16,
    },
    Client {
        server: SocketAddr,
        client_id: u64,
    },
}

impl NetMode {
    pub fn local_owner(self) -> Owner {
        match self {
            NetMode::Client { client_id, .. } => Owner::Client(client_id),
            NetMode::Single | NetMode::Host { .. } => Owner::Host,
        }
    }

    pub fn is_authority(self) -> bool {
        !matches!(self, NetMode::Client { .. })
    }
}

/// The player who controls a pigeon. A client knows its own id, so it finds its pigeon.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Host,
    /// The renet client id of the player.
    Client(u64),
}

/// The pigeon of the player on this instance. The camera and the HUD follow it.
#[derive(Component, Debug)]
pub struct LocalPigeon;

/// On the host: the pigeon of a connected client. The pigeon leaves with the client.
#[derive(Component, Debug)]
struct ClientPigeon(Entity);

// Replicon serializes with serde. The game has no serde dependency, and the Bevy feature
// `serialize` is off. So each replicated component converts to a type with a serde implementation.
impl From<Owner> for Option<u64> {
    fn from(owner: Owner) -> Self {
        match owner {
            Owner::Host => None,
            Owner::Client(id) => Some(id),
        }
    }
}

impl From<Option<u64>> for Owner {
    fn from(id: Option<u64>) -> Self {
        id.map_or(Owner::Host, Owner::Client)
    }
}

impl From<Stamina> for f32 {
    fn from(stamina: Stamina) -> Self {
        stamina.value()
    }
}

impl From<f32> for Stamina {
    fn from(value: f32) -> Self {
        Stamina::new(value)
    }
}

impl From<Pigeon> for () {
    fn from(_: Pigeon) -> Self {}
}

impl From<()> for Pigeon {
    fn from(_: ()) -> Self {
        Pigeon
    }
}

/// Replication and input routing. The host, the client and a single player use it.
/// [`start_network`] opens the connection.
pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetMode>()
            .replicate_as::<Pigeon, ()>()
            .replicate_with(RuleFns::new(serialize_transform, deserialize_transform))
            .replicate_as::<Owner, Option<u64>>()
            .replicate_as::<Stamina, f32>()
            // Ordered and reliable: a lost frame can hold the only press of a jump.
            .add_client_message_with(Channel::Ordered, serialize_input, deserialize_input)
            .add_observer(mark_local_pigeon)
            .add_observer(despawn_client_pigeon)
            .add_observer(log_netcode_error)
            .add_systems(
                Startup,
                spawn_host_pigeon.run_if(|mode: Res<NetMode>| mode.is_authority()),
            )
            .add_systems(PreUpdate, apply_input.after(ServerSystems::Receive))
            .add_systems(
                Update,
                spawn_client_pigeons.run_if(in_state(ServerState::Running)),
            );
    }
}

/// Reads the command line. `client_id` identifies this instance if it connects to a host.
pub fn parse_args(
    args: impl IntoIterator<Item = String>,
    client_id: u64,
) -> Result<NetMode, String> {
    let mut args = args.into_iter();
    let Some(flag) = args.next() else {
        return Ok(NetMode::Single);
    };
    let mode = match flag.as_str() {
        "--host" => {
            let port = match args.next() {
                None => DEFAULT_PORT,
                Some(port) => port
                    .parse()
                    .map_err(|_| format!("The port \"{port}\" is not a number from 0 to 65535."))?,
            };
            NetMode::Host { port }
        }
        "--connect" => {
            let address = args
                .next()
                .ok_or("The flag --connect needs an address. Example: --connect 127.0.0.1:5000")?;
            let server = address.parse().map_err(|_| {
                format!("The address \"{address}\" is not an IP address with a port.")
            })?;
            NetMode::Client { server, client_id }
        }
        _ => return Err(unknown_argument(&flag)),
    };
    match args.next() {
        Some(extra) => Err(unknown_argument(&extra)),
        None => Ok(mode),
    }
}

fn unknown_argument(argument: &str) -> String {
    format!(
        "The argument \"{argument}\" is not known. Use --host [PORT] or --connect ADDRESS:PORT."
    )
}

/// The time since 1970. Netcode needs it, and it gives a client id that is new on each start.
pub fn unix_time() -> Result<Duration, String> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| format!("The system clock is before the year 1970: {error}"))
}

/// Opens the UDP socket of the host or the client. If this fails, the app exits.
pub fn start_network(
    mut commands: Commands,
    mode: Res<NetMode>,
    channels: Res<RepliconChannels>,
    mut exit: MessageWriter<AppExit>,
) {
    let result = match *mode {
        NetMode::Single => return,
        NetMode::Host { port } => start_host(&mut commands, &channels, port),
        NetMode::Client { server, client_id } => {
            start_client(&mut commands, &channels, server, client_id)
        }
    };
    if let Err(error) = result {
        error!("{error}");
        exit.write(AppExit::error());
    }
}

fn connection_config(channels: &RepliconChannels) -> ConnectionConfig {
    ConnectionConfig {
        server_channels_config: channels.server_configs(),
        client_channels_config: channels.client_configs(),
        ..default()
    }
}

fn start_host(
    commands: &mut Commands,
    channels: &RepliconChannels,
    port: u16,
) -> Result<(), String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
        .map_err(|error| format!("The host cannot open the UDP port {port}: {error}"))?;
    let config = ServerConfig {
        current_time: unix_time()?,
        max_clients: MAX_CLIENTS,
        protocol_id: PROTOCOL_ID,
        public_addresses: Vec::new(),
        authentication: ServerAuthentication::Unsecure,
    };
    let transport = NetcodeServerTransport::new(config, socket)
        .map_err(|error| format!("The host cannot start on the UDP port {port}: {error}"))?;
    commands.insert_resource(RenetServer::new(connection_config(channels)));
    commands.insert_resource(transport);
    info!("The host listens on the UDP port {port}.");
    Ok(())
}

fn start_client(
    commands: &mut Commands,
    channels: &RepliconChannels,
    server: SocketAddr,
    client_id: u64,
) -> Result<(), String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|error| format!("The client cannot open a UDP port: {error}"))?;
    let authentication = ClientAuthentication::Unsecure {
        protocol_id: PROTOCOL_ID,
        client_id,
        server_addr: server,
        user_data: None,
    };
    let transport = NetcodeClientTransport::new(unix_time()?, authentication, socket)
        .map_err(|error| format!("The client cannot connect to {server}: {error}"))?;
    commands.insert_resource(RenetClient::new(connection_config(channels)));
    commands.insert_resource(transport);
    info!("The client {client_id} connects to {server}.");
    Ok(())
}

fn log_netcode_error(error: On<NetcodeErrorEvent>) {
    error!("The network transport failed: {}", *error);
}

/// Only translation and rotation: the scale of a pigeon does not change.
fn serialize_transform(
    _: &mut SerializeCtx,
    transform: &Transform,
    bytes: &mut Vec<u8>,
) -> Result<()> {
    postcard_utils::to_extend_mut(&(transform.translation, transform.rotation), bytes)?;
    Ok(())
}

fn deserialize_transform(_: &mut WriteCtx, bytes: &mut Bytes) -> Result<Transform> {
    let (translation, rotation): (Vec3, Quat) = postcard_utils::from_buf(bytes)?;
    Ok(Transform::from_translation(translation).with_rotation(rotation))
}

fn serialize_input(
    _: &mut ClientSendCtx,
    message: &InputMessage,
    bytes: &mut Vec<u8>,
) -> Result<()> {
    let input = message.0;
    postcard_utils::to_extend_mut(&(input.movement, input.jump, input.glide), bytes)?;
    Ok(())
}

fn deserialize_input(_: &mut ServerReceiveCtx, bytes: &mut Bytes) -> Result<InputMessage> {
    let (movement, jump, glide): (Vec2, bool, bool) = postcard_utils::from_buf(bytes)?;
    Ok(InputMessage(PigeonInput {
        movement,
        jump,
        glide,
    }))
}

/// The start position of the pigeon with the number `index`, in a row along +X.
fn spawn_point(index: usize) -> Vec3 {
    Vec3::new(index as f32 * SPAWN_SPACING, SPAWN_HEIGHT, 0.0)
}

fn networked_pigeon(owner: Owner, index: usize) -> impl Bundle {
    (
        pigeon_body(),
        owner,
        Replicated,
        Transform::from_translation(spawn_point(index)),
    )
}

fn spawn_host_pigeon(mut commands: Commands) {
    commands.spawn(networked_pigeon(Owner::Host, 0));
}

/// Connected clients that have no pigeon yet.
type NewClients = (With<AuthorizedClient>, Without<ClientPigeon>);

fn spawn_client_pigeons(
    mut commands: Commands,
    clients: Query<(Entity, &NetworkId), NewClients>,
    pigeons: Query<(), With<Pigeon>>,
) {
    let first_index = pigeons.iter().count();
    for (index, (client, network_id)) in (first_index..).zip(&clients) {
        let pigeon = commands
            .spawn(networked_pigeon(Owner::Client(network_id.get()), index))
            .id();
        commands.entity(client).insert(ClientPigeon(pigeon));
        info!("The client {} joined.", network_id.get());
    }
}

fn despawn_client_pigeon(
    remove: On<Remove, ClientPigeon>,
    clients: Query<&ClientPigeon>,
    mut commands: Commands,
) {
    if let Ok(pigeon) = clients.get(remove.entity) {
        commands.entity(pigeon.0).try_despawn();
    }
}

fn mark_local_pigeon(
    add: On<Add, Owner>,
    owners: Query<&Owner>,
    mode: Res<NetMode>,
    mut commands: Commands,
) {
    if owners
        .get(add.entity)
        .is_ok_and(|owner| *owner == mode.local_owner())
    {
        commands.entity(add.entity).insert(LocalPigeon);
    }
}

/// Runs on every instance. Only the authority receives [`FromClient`] messages.
fn apply_input(
    mut messages: MessageReader<FromClient<InputMessage>>,
    clients: Query<&ClientPigeon>,
    mut pigeons: Query<(Entity, &Owner, &mut PigeonInput)>,
) {
    for message in messages.read() {
        let pigeon = match message.client_id {
            ClientId::Server => pigeons
                .iter()
                .find(|(_, owner, _)| **owner == Owner::Host)
                .map(|(entity, ..)| entity),
            ClientId::Client(client) => clients.get(client).ok().map(|pigeon| pigeon.0),
        };
        // A client sends input before the host spawns its pigeon.
        let Some(pigeon) = pigeon else {
            continue;
        };
        if let Ok((_, _, mut input)) = pigeons.get_mut(pigeon) {
            input.merge(message.message.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn no_arguments_is_single_player() {
        assert_eq!(parse_args(args(&[]), 7), Ok(NetMode::Single));
    }

    #[test]
    fn host_uses_default_port() {
        assert_eq!(
            parse_args(args(&["--host"]), 7),
            Ok(NetMode::Host { port: DEFAULT_PORT })
        );
    }

    #[test]
    fn host_takes_port() {
        assert_eq!(
            parse_args(args(&["--host", "6000"]), 7),
            Ok(NetMode::Host { port: 6000 })
        );
    }

    #[test]
    fn connect_takes_address() {
        assert_eq!(
            parse_args(args(&["--connect", "127.0.0.1:5000"]), 7),
            Ok(NetMode::Client {
                server: SocketAddr::from(([127, 0, 0, 1], 5000)),
                client_id: 7,
            })
        );
    }

    #[test]
    fn bad_port_names_input() {
        let error = parse_args(args(&["--host", "abc"]), 7).unwrap_err();
        assert!(error.contains("\"abc\""), "{error}");
    }

    #[test]
    fn connect_without_address_fails() {
        assert!(parse_args(args(&["--connect"]), 7).is_err());
    }

    #[test]
    fn bad_address_names_input() {
        let error = parse_args(args(&["--connect", "localhost"]), 7).unwrap_err();
        assert!(error.contains("\"localhost\""), "{error}");
    }

    #[test]
    fn unknown_argument_names_input() {
        let error = parse_args(args(&["--host", "5000", "extra"]), 7).unwrap_err();
        assert!(error.contains("\"extra\""), "{error}");
    }

    #[test]
    fn owner_survives_conversion() {
        for owner in [Owner::Host, Owner::Client(42)] {
            assert_eq!(Owner::from(Option::<u64>::from(owner)), owner);
        }
    }

    #[test]
    fn spawn_points_do_not_overlap() {
        assert!(spawn_point(0).distance(spawn_point(1)) >= SPAWN_SPACING);
    }

    #[test]
    fn local_owner_of_client_is_its_id() {
        let mode = NetMode::Client {
            server: SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
            client_id: 9,
        };
        assert_eq!(mode.local_owner(), Owner::Client(9));
        assert_eq!(NetMode::Host { port: 1 }.local_owner(), Owner::Host);
    }
}
