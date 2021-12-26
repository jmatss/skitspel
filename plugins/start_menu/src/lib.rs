use std::ops::{Deref, DerefMut};

use bevy::{
    math::Vec2,
    prelude::{
        Assets, BuildChildren, Children, Color, Commands, Entity, EventReader, EventWriter, Handle,
        HorizontalAlign, IntoSystem, Local, Mesh, ParallelSystemDescriptorCoercion, Plugin, Query,
        Res, ResMut, Shader, State, SystemSet, Transform, VerticalAlign,
    },
    render::{mesh::VertexAttributeValues, pipeline::PipelineDescriptor},
    text::{Font, Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_rapier2d::prelude::ColliderType;

use skitspel::{
    ActionEvent, ConnectedPlayers, DisconnectedPlayers, GameState, PlayerId, Players, COLORS,
    GAME_HEIGHT, PLAYER_RADIUS,
};
use util_bevy::{despawn_entity, despawn_system, AsBevyColor, Fonts, PlayerVote, VoteEvent};
use util_rapier::{move_players, spawn_border_walls, spawn_player};

const GAME_STATE: GameState = GameState::StartMenu;

const HEADER_TEXT: &str = "SKITSPEL";
const READY_TEXT: &str = "Ready";
const NOT_READY_TEXT: &str = "Not ready";

/// Event triggered when a player changes color.
struct ColorChangeEvent(PlayerId);

impl Deref for ColorChangeEvent {
    type Target = PlayerId;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ColorChangeEvent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Will be used as a tag/component added to all entities related to the menu.
///
/// The movable player entities in the menu are tagged with the `PlayerId`, so
/// querying for all entities with the `PlayerId` tag will return all players
/// that are currently displayed on the screen.
#[derive(Clone)]
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut bevy::prelude::AppBuilder) {
        app.add_event::<ColorChangeEvent>()
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_menu.system()),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_connect.system().label("network"))
                    .with_system(handle_disconnect.system().label("network"))
                    .with_system(handle_player_input.system().label("vote").after("network"))
                    .with_system(handle_color_change.system().after("vote"))
                    .with_system(handle_ready_event.system().after("vote"))
                    .with_system(move_players.system()),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE).with_system(despawn_system::<MenuPlugin>.system()),
            );
    }
}

fn handle_connect(
    mut commands: Commands,
    fonts: Res<Fonts>,
    connected_players: Res<ConnectedPlayers>,
) {
    if !connected_players.is_empty() {
        for player in connected_players.values() {
            // TODO: Randomize position?
            let pos = Vec2::new(0.0, 0.0);
            spawn_player_with_text(
                &mut commands,
                &fonts,
                player.id(),
                player.score(),
                player.color().as_bevy(),
                pos,
                PLAYER_RADIUS,
            );
        }
    }
}

fn handle_disconnect(
    mut commands: Commands,
    disconnected_players: Res<DisconnectedPlayers>,
    players_alive_query: Query<(Entity, &PlayerId)>,
    mut ready_event_writer: EventWriter<VoteEvent>,
) {
    if !disconnected_players.is_empty() {
        for (entity, player_id) in players_alive_query.iter() {
            if disconnected_players.contains(player_id) {
                ready_event_writer.send(VoteEvent::Value(*player_id, false));
                despawn_entity(&mut commands, entity);
            }
        }
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    mut ready_event_writer: EventWriter<VoteEvent>,
    mut color_event_writer: EventWriter<ColorChangeEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                if let ActionEvent::APressed = prev_action {
                    ready_event_writer.send(VoteEvent::Flip(player.id()));
                } else if let ActionEvent::BPressed = prev_action {
                    let cur_color = player.color();
                    if let Some(idx) = COLORS.iter().position(|&c| c == cur_color) {
                        let next_color_idx = if idx == COLORS.len() - 1 { 0 } else { idx + 1 };
                        let next_color = COLORS[next_color_idx];
                        player.set_color(next_color);
                    }
                    color_event_writer.send(ColorChangeEvent(player.id()));
                }
            }
        }
    }
}

// TODO: Currently not possible update the color in the `ShapeColors` directly,
//       so change the color in the mesh instead. In a future lyon update, the
//       `ShapeColors` have been replaced with a `DrawMode`. Use that in the
//       future.
/// Updates the color of the characters on screen representing the players.
fn handle_color_change(
    mut meshes: ResMut<Assets<Mesh>>,
    players: Res<Players>,
    player_characters: Query<(&PlayerId, &Handle<Mesh>)>,
    mut color_event_reader: EventReader<ColorChangeEvent>,
) {
    for ColorChangeEvent(player_id) in color_event_reader.iter() {
        for (mesh_player_id, mesh_handle) in player_characters.iter() {
            let player = match players.get(player_id) {
                Some(player) if player_id == mesh_player_id => player,
                _ => continue,
            };

            let color = player.color();
            let mesh = meshes.get_mut(mesh_handle).unwrap();
            if let Some(VertexAttributeValues::Float4(color_vecs)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
            {
                for color_vec in color_vecs {
                    *color_vec = [color.r, color.g, color.b, color.a];
                }
            }
        }
    }
}

/// If all players are ready we should move on to the game selecting.
fn handle_ready_event(
    players: Res<Players>,
    fonts: Res<Fonts>,
    player_characters: Query<(&PlayerId, &Children)>,
    mut player_ready_text: Query<&mut Text>,
    mut game_state: ResMut<State<GameState>>,
    mut player_ready_vote: Local<PlayerVote>,
    mut ready_event_reader: EventReader<VoteEvent>,
) {
    // Make a copy since we want to iterate it twice but it gets consumed after
    // the first iteration.
    let ready_events = ready_event_reader.iter().cloned().collect::<Vec<_>>();

    // The `Text` is spawned as a child bundle of the player to allow it to be
    // positioned below the player. We therefore have to use two queries (one for
    // parent & one for child) and then "zip" them using their entity handles.
    for ready_event in ready_events.iter() {
        let (event_player_id, is_ready) = match ready_event {
            VoteEvent::Value(player_id, value) => (player_id, *value),
            VoteEvent::Flip(player_id) => (player_id, !player_ready_vote.contains(player_id)),
            VoteEvent::Reset => {
                player_ready_vote.reset();
                return;
            }
        };

        for (player_id, children) in player_characters.iter() {
            if event_player_id != player_id {
                continue;
            }

            for child_entity in children.iter() {
                if let Ok(mut text) = player_ready_text.get_mut(*child_entity) {
                    if let Some(ready_section) = text.sections.get_mut(1) {
                        let font = fonts.regular.clone();
                        let font_size = 24.0;
                        *ready_section = ready_text_section(is_ready, font, font_size);
                    }
                }
            }
        }
    }

    let voted_amount_before = player_ready_vote.voted_amount();
    let total_amount_before = player_ready_vote.total_amount();

    ready_events
        .iter()
        .for_each(|vote| player_ready_vote.register_vote(vote));

    let voted_amount_after = player_ready_vote.len();
    let total_amount_after = players.len();

    if voted_amount_before != voted_amount_after || total_amount_before != total_amount_after {
        player_ready_vote.set_total_amount(total_amount_after);

        let required_amount = players.len();
        if total_amount_after >= 2 && voted_amount_after >= required_amount {
            game_state.set(GameState::GameSelectionMenu).unwrap();
        }
    }
}

fn spawn_player_with_text(
    commands: &mut Commands,
    fonts: &Fonts,
    player_id: PlayerId,
    player_score: usize,
    color: Color,
    pos: Vec2,
    radius: f32,
) {
    let font = fonts.regular.clone();
    let font_size = 24.0;

    let text_bundle = Text2dBundle {
        text: Text {
            sections: vec![
                TextSection {
                    value: format!(" \n\n\nScore: {}\n", player_score),
                    style: TextStyle {
                        font: font.clone(),
                        font_size,
                        color: Color::WHITE,
                    },
                },
                ready_text_section(false, font, font_size),
            ],
            alignment: TextAlignment {
                vertical: VerticalAlign::Bottom,
                horizontal: HorizontalAlign::Center,
            },
        },
        ..Default::default()
    };

    spawn_player(commands, player_id, color, pos, radius)
        .insert(MenuPlugin)
        .with_children(|parent| {
            parent.spawn_bundle(text_bundle);
        });
}

fn ready_text_section(is_ready: bool, font: Handle<Font>, font_size: f32) -> TextSection {
    let (value, color) = if is_ready {
        (READY_TEXT.into(), Color::GREEN)
    } else {
        (NOT_READY_TEXT.into(), Color::RED)
    };
    TextSection {
        value,
        style: TextStyle {
            font,
            font_size,
            color,
        },
    }
}

fn reset_votes(mut ready_event_writer: EventWriter<VoteEvent>) {
    ready_event_writer.send(VoteEvent::Reset);
}

fn setup_menu(
    mut commands: Commands,
    mut players: ResMut<Players>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    mut shaders: ResMut<Assets<Shader>>,
    fonts: Res<Fonts>,
) {
    let bold_font = fonts.bold.clone();
    let bold_font_size = 64.0;
    let regular_font = fonts.regular.clone();
    let regular_font_size = 24.0;
    let font_color = Color::WHITE;
    let grey_color = Color::rgb(0.3, 0.3, 0.3);

    spawn_border_walls(
        &mut commands,
        &mut meshes,
        &mut pipelines,
        &mut shaders,
        grey_color,
        10.0,
        ColliderType::Solid,
        Option::<MenuPlugin>::None,
    )
    .insert(MenuPlugin);

    for player in players.values_mut() {
        // TODO: Randomize position?
        let pos = Vec2::new(0.0, 0.0);
        spawn_player_with_text(
            &mut commands,
            &fonts,
            player.id(),
            player.score(),
            player.color().as_bevy(),
            pos,
            PLAYER_RADIUS,
        );
    }

    let text_sections = vec![
        TextSection {
            value: HEADER_TEXT.into(),
            style: TextStyle {
                font: bold_font,
                font_size: bold_font_size,
                color: font_color,
            },
        },
        TextSection {
            value: "\nPress A to ready up".into(),
            style: TextStyle {
                font: regular_font.clone(),
                font_size: regular_font_size,
                color: font_color,
            },
        },
        TextSection {
            value: "\nPress B to change color".into(),
            style: TextStyle {
                font: regular_font,
                font_size: regular_font_size,
                color: font_color,
            },
        },
    ];

    let text_bundle = Text2dBundle {
        text: Text {
            sections: text_sections,
            alignment: TextAlignment {
                vertical: VerticalAlign::Bottom,
                horizontal: HorizontalAlign::Center,
            },
        },
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands.spawn_bundle(text_bundle).insert(MenuPlugin);
}
