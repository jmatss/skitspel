pub use action::{ActionEvent, PlayerAction};
pub use color::Color;
pub use network::{Port, TLSCertificate};
pub use player::{
    ConnectedPlayers, DisconnectedPlayers, Player, PlayerId, PlayerIdGenerator, Players,
};

mod action;
mod color;
mod network;
mod player;

/// All possible states in the game.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
pub enum GameState {
    /// State when players are joining the game.
    StartMenu,
    /// State between games when selecting a new game to play.
    GameSelectionMenu,

    /// The currently active game.
    PushGame,
    HockeyGame,
    VolleyBallGame,
    AchtungGame,
}

/// The units used in the the rapier is recommended to represent SI units.
/// So when converting the values from "rapier-units" to "bevy-units", the
/// units will be scaled with a factor of `RAPIER_SCALE_FACTOR`.
pub const RAPIER_SCALE_FACTOR: f32 = 25.0;

/// The width of the game viewport. This size will be fixed and the game will
/// be scaled to always fit the window size.
pub const GAME_WIDTH: f32 = 1920.0;

/// The height of the game viewport. This size will be fixed and the game will
/// be scaled to always fit the window size.
pub const GAME_HEIGHT: f32 = 1080.0;

/// The maximum amount of players that can play. This can be used to ensure that
/// everything can support atleast this amount of players. Ex. amount of spawn
/// points.
pub const MAX_PLAYERS: usize = 9;

/// The default amount of acceleration that should be applied to the player
/// every tick (during acceleration).
pub const ACCEL_AMOUNT: f32 = 300.0;

/// The default amount of torque acceleration that should be applied to the player
/// every tick (during a spin action).
pub const TORQUE_ACCEL_AMOUNT: f32 = 300.0;

/// The default radius of the "default" player characters.
pub const PLAYER_RADIUS: f32 = 60.0;

/// Colors that can be picked by the players.
pub const COLORS: [Color; MAX_PLAYERS] = [
    Color::new(0.6, 0.0, 0.0), // red
    Color::new(0.0, 0.6, 0.0), // green
    Color::new(0.0, 0.5, 0.8), // blue
    Color::new(1.0, 0.5, 0.0), // orange
    Color::new(0.6, 0.0, 0.6), // purple
    Color::new(0.4, 0.4, 0.4), // grey
    Color::new(0.4, 0.2, 0.0), // brown
    Color::new(1.0, 0.2, 0.6), // pink
    Color::new(0.0, 0.5, 1.0), // light blue
];

/// Amount of vertices that a shape can have. A value can be picked randomly from
/// this array. 0 => circle.
pub const VERTEX_AMOUNT: [usize; 7] = [0, 3, 4, 5, 6, 7, 8];
