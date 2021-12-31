use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    ops::{Deref, DerefMut},
};

use crate::{ActionEvent, Color, PlayerAction};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(u64);

impl Deref for PlayerId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u64> for PlayerId {
    fn from(n: u64) -> Self {
        PlayerId(n)
    }
}

impl Display for PlayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
pub struct PlayerIdGenerator {
    id: u64,
}

impl PlayerIdGenerator {
    pub fn generate(&mut self) -> PlayerId {
        let id = self.id;
        self.id += 1;
        PlayerId(id)
    }
}

impl Default for PlayerIdGenerator {
    fn default() -> Self {
        Self { id: 1 }
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    id: PlayerId,
    score: usize,

    /// Name specified by the client when it connected to the server.
    name: String,

    /// This will be used for all color related to this player ex. on the icon
    /// or game items.
    color: Color,

    /// Indicates the current buttons that this player is currently pushing/
    /// not pushing.
    action: PlayerAction,
}

impl Player {
    pub fn new(id: PlayerId, name: String, color: Color) -> Self {
        Self {
            id,
            score: 0,
            name,
            color,
            action: Default::default(),
        }
    }

    pub fn update_action(&mut self, action_event: &ActionEvent) {
        match action_event {
            ActionEvent::UpPressed => self.action.up_pressed = true,
            ActionEvent::UpReleased => self.action.up_pressed = false,
            ActionEvent::RightPressed => self.action.right_pressed = true,
            ActionEvent::RightReleased => self.action.right_pressed = false,
            ActionEvent::DownPressed => self.action.down_pressed = true,
            ActionEvent::DownReleased => self.action.down_pressed = false,
            ActionEvent::LeftPressed => self.action.left_pressed = true,
            ActionEvent::LeftReleased => self.action.left_pressed = false,
            ActionEvent::APressed => self.action.a_pressed = true,
            ActionEvent::AReleased => self.action.a_pressed = false,
            ActionEvent::BPressed => self.action.b_pressed = true,
            ActionEvent::BReleased => self.action.b_pressed = false,
            ActionEvent::None => (),
        }

        self.action.prev_action = *action_event;
        self.action.new_action_since_last_read = true;
    }

    pub fn reset_action(&mut self) {
        self.action.up_pressed = false;
        self.action.right_pressed = false;
        self.action.down_pressed = false;
        self.action.left_pressed = false;
        self.action.a_pressed = false;
        self.action.b_pressed = false;
    }

    pub fn has_no_action(&self) -> bool {
        !self.action.up_pressed
            && !self.action.right_pressed
            && !self.action.down_pressed
            && !self.action.left_pressed
            && !self.action.a_pressed
            && !self.action.b_pressed
    }

    /// Returns the previous/latest action only once. When this function has
    /// been called, it will return None until a new action have been set.
    pub fn previous_action_once(&mut self) -> Option<ActionEvent> {
        if self.action.new_action_since_last_read {
            self.action.new_action_since_last_read = false;
            Some(self.action.prev_action)
        } else {
            None
        }
    }

    pub fn movement_x(&self) -> f32 {
        if self.action.right_pressed && self.action.left_pressed {
            0.0
        } else if self.action.right_pressed {
            1.0
        } else if self.action.left_pressed {
            -1.0
        } else {
            0.0
        }
    }

    pub fn movement_y(&self) -> f32 {
        if self.action.up_pressed && self.action.down_pressed {
            0.0
        } else if self.action.up_pressed {
            1.0
        } else if self.action.down_pressed {
            -1.0
        } else {
            0.0
        }
    }

    pub fn a_is_pressed(&self) -> bool {
        self.action.a_pressed
    }

    pub fn b_is_pressed(&self) -> bool {
        self.action.b_pressed
    }

    pub fn id(&self) -> PlayerId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    pub fn score(&self) -> usize {
        self.score
    }

    pub fn increment_score(&mut self) {
        self.score += 1;
    }

    pub fn reset_score(&mut self) {
        self.score = 0;
    }
}

/// Will be a resource in bevy that contains all currently active players.
#[derive(Debug, Default)]
pub struct Players(HashMap<PlayerId, Player>);

impl Deref for Players {
    type Target = HashMap<PlayerId, Player>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Players {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Will be a resource in bevy which gets populated when new players are connected.
/// This can be checked inside running games to see if new players have recently
/// connected.
#[derive(Debug, Default)]
pub struct ConnectedPlayers(HashMap<PlayerId, Player>);

impl Deref for ConnectedPlayers {
    type Target = HashMap<PlayerId, Player>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ConnectedPlayers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Will be a resource in bevy which gets populated when players disconnect.
/// This can be checked inside running games to see if new players have recently
/// disconnected.
#[derive(Debug, Default)]
pub struct DisconnectedPlayers(HashSet<PlayerId>);

impl Deref for DisconnectedPlayers {
    type Target = HashSet<PlayerId>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DisconnectedPlayers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
