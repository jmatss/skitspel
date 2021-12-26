use std::ops::{Deref, DerefMut};

use bevy::{prelude::Handle, sprite::ColorMaterial};
use skitspel::GameState;

/// Resource that stores the games that are playable.
#[derive(Debug, Default)]
pub struct Games(pub Vec<Game>);

impl Deref for Games {
    type Target = Vec<Game>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Games {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Represents a playable game.
#[derive(Debug)]
pub struct Game {
    pub name: &'static str,
    pub game_state: GameState,
    pub screenshot: Handle<ColorMaterial>,
}
