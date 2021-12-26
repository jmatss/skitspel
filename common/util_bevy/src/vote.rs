use std::{
    collections::HashSet,
    ops::{Deref, DerefMut},
};

use bevy::{
    prelude::{Color, Handle},
    text::{Font, TextSection, TextStyle},
};

use skitspel::{PlayerId, Players};

use crate::AsBevyColor;

/// Struct that can be used as a local resource for a vote.
/// This can ex. be "continue" or "exit" events.
///
/// If there are multiple vote that can happen at the same time, one can wrap
/// this struct in another struct to make it unique.
#[derive(Debug, Default)]
pub struct PlayerVote {
    /// Contains the player IDs of players that wants this vote to go through.
    pub player_ids: HashSet<PlayerId>,
    /// Contains the total amount of players that are currently connected to the
    /// game. This can be used to see if ex. a majority of the player wants this
    /// to happen.
    pub total_amount_of_players: usize,
}

impl PlayerVote {
    /// Returns the amount of players that have voted for this to happen.
    pub fn voted_amount(&self) -> usize {
        self.player_ids.len()
    }

    /// Returns the total amount of players connected to the game.
    pub fn total_amount(&self) -> usize {
        self.total_amount_of_players
    }

    pub fn set_total_amount(&mut self, amount: usize) {
        self.total_amount_of_players = amount;
    }

    pub fn register_vote(&mut self, vote_event: &VoteEvent) {
        match vote_event {
            VoteEvent::Value(id, true) => {
                self.player_ids.insert(*id);
            }
            VoteEvent::Value(id, false) => {
                self.player_ids.remove(id);
            }
            VoteEvent::Flip(id) => {
                if self.player_ids.contains(id) {
                    self.player_ids.remove(id);
                } else {
                    self.player_ids.insert(*id);
                }
            }
            VoteEvent::Reset => {
                self.reset();
            }
        }
    }

    pub fn reset(&mut self) {
        self.player_ids.clear();
        self.total_amount_of_players = 0;
    }
}

impl Deref for PlayerVote {
    type Target = HashSet<PlayerId>;

    fn deref(&self) -> &Self::Target {
        &self.player_ids
    }
}

impl DerefMut for PlayerVote {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.player_ids
    }
}

/// Used as an event for a vote. This will be sent over `EventWriter`s &
/// `EventReader`s and be registered inside a `PlayerVote`.
#[derive(Clone)]
pub enum VoteEvent {
    /// Set to true if the player wants to vote, set to false if the player
    /// wants to remove its (potential) vote.
    Value(PlayerId, bool),
    /// Flips the current vote of the player that triggered this event.
    Flip(PlayerId),
    /// Sent to reset the contents of a vote. This can be used to reset a vote
    /// during startup or teardown.
    Reset,
}

/// Function used to create sections of text which contains a "vote" text.
///
/// This is a text which shows the amount of people that have voted, the color
/// of their characters and the amount that is required to make the vote go through.
pub fn create_vote_text_sections(
    mut text: String,
    players: &Players,
    vote: &PlayerVote,
    required_amount: usize,
    font: Handle<Font>,
    font_size: f32,
) -> Vec<TextSection> {
    let mut text_sections = Vec::default();

    let mut player_ids = vote.iter().collect::<Vec<_>>();
    player_ids.sort_unstable();

    text.push_str(&format!(" ({}/{}", player_ids.len(), required_amount));

    text_sections.push(TextSection {
        value: text,
        style: TextStyle {
            font: font.clone(),
            font_size,
            color: Color::WHITE,
        },
    });

    if !player_ids.is_empty() {
        text_sections.push(TextSection {
            value: " ".into(),
            style: TextStyle {
                font: font.clone(),
                font_size,
                color: Color::WHITE,
            },
        });

        for player_id in player_ids.iter() {
            if let Some(player) = players.get(player_id) {
                text_sections.push(TextSection {
                    value: "•".into(),
                    style: TextStyle {
                        font: font.clone(),
                        font_size,
                        color: player.color().as_bevy(),
                    },
                });
            }
        }
    }

    text_sections.push(TextSection {
        value: ")".into(),
        style: TextStyle {
            font,
            font_size,
            color: Color::WHITE,
        },
    });

    text_sections
}
