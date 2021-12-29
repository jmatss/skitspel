pub use despawn::{despawn_entity, despawn_system};
pub use fonts::Fonts;
pub use game::{Game, Games};

pub use shape::Shape;
pub use start::{handle_start_timer, setup_start_timer, StartEntity, StartTimer};
pub use vote::create_vote_text_sections;
pub use vote::{PlayerVote, VoteEvent};

mod despawn;
mod fonts;
mod game;
mod shape;
mod start;
mod vote;

pub trait AsBevyColor {
    fn as_bevy(&self) -> bevy::prelude::Color;
}

impl AsBevyColor for skitspel::Color {
    fn as_bevy(&self) -> bevy::prelude::Color {
        bevy::prelude::Color::rgba(self.r, self.g, self.b, self.a)
    }
}
