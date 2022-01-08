use std::ops::{Deref, DerefMut};

use bevy::{
    math::{Quat, Vec2, Vec3},
    prelude::{
        Assets, BuildChildren, Changed, Children, Color, Commands, EventReader, EventWriter,
        Handle, HorizontalAlign, IntoSystem, Local, ParallelSystemDescriptorCoercion, Plugin,
        Query, Res, ResMut, SpriteBundle, State, SystemSet, Transform, VerticalAlign, With,
    },
    sprite::ColorMaterial,
    text::{Font, Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};

use skitspel::{ActionEvent, DisconnectedPlayers, GameState, Players, GAME_HEIGHT};
use util_bevy::{
    create_vote_text_sections, despawn_system, Fonts, Game, Games, PlayerVote, Shape, VoteEvent,
};

const GAME_STATE: GameState = GameState::GameSelectionMenu;

const HEADER_TEXT: &str = "Select Game";
const START_TEXT: &str = "\nPress A to start game";
const EXIT_TEXT: &str = "\nPress B to go back to main menu";

//// The horizontal distance between the selectable games on screen.
const SELECTABLE_GAMES_SPACING: f32 = 650.0;

/// Tag used on the continue text (i.e. select game).
struct StartText;

/// Tag used on the exit text.
struct ExitText;

/// Event triggered when a player wants to update its exit vote.
struct ExitVoteEvent(VoteEvent);

impl Deref for ExitVoteEvent {
    type Target = VoteEvent;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ExitVoteEvent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Event triggered when a player wants to update its start vote.
struct StartVoteEvent(VoteEvent);

impl Deref for StartVoteEvent {
    type Target = VoteEvent;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StartVoteEvent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Will be used as a tag/component added to all entities related to the menu
/// for selecting a game.
#[derive(Clone)]
pub struct GameSelectionPlugin;

/// Tag used for the currently selected game and its index in the `Games` vector.
struct SelectedGame(usize);

/// Tag used on the games displayed on screen. The index if the index of the game
/// inside `Games` (used to keep track of order).
struct SelectableGame(usize);

impl Plugin for GameSelectionPlugin {
    fn build(&self, app: &mut bevy::prelude::AppBuilder) {
        app.add_event::<ExitVoteEvent>()
            .add_event::<StartVoteEvent>()
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_menu.system())
                    .with_system(setup_selectable_games.system()),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_disconnect.system().label("vote"))
                    .with_system(handle_player_input.system().label("vote").label("change"))
                    .with_system(handle_start_event.system().after("vote"))
                    .with_system(handle_exit_event.system().after("vote"))
                    .with_system(handle_changed_game.system().after("change")),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE)
                    .with_system(despawn_system::<GameSelectionPlugin>.system()),
            );
    }
}

fn handle_disconnect(
    disconnected_players: Res<DisconnectedPlayers>,
    mut start_event_writer: EventWriter<StartVoteEvent>,
    mut exit_event_writer: EventWriter<ExitVoteEvent>,
) {
    for player_id in disconnected_players.iter() {
        start_event_writer.send(StartVoteEvent(VoteEvent::Value(*player_id, false)));
        exit_event_writer.send(ExitVoteEvent(VoteEvent::Value(*player_id, false)));
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    games: Res<Games>,
    mut selected_game_query: Query<&mut SelectedGame>,
    mut start_vote_event: EventWriter<StartVoteEvent>,
    mut exit_event_writer: EventWriter<ExitVoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                match prev_action {
                    ActionEvent::APressed => {
                        start_vote_event.send(StartVoteEvent(VoteEvent::Flip(player.id())));
                    }

                    ActionEvent::BPressed => {
                        exit_event_writer.send(ExitVoteEvent(VoteEvent::Flip(player.id())));
                    }

                    ActionEvent::LeftPressed | ActionEvent::RightPressed => {
                        let SelectedGame(ref mut idx) = *selected_game_query.single_mut().unwrap();
                        if let ActionEvent::LeftPressed = prev_action {
                            *idx = if *idx == 0 { games.len() - 1 } else { *idx - 1 };
                        } else {
                            *idx = if *idx == games.len() - 1 { 0 } else { *idx + 1 };
                        }
                    }

                    _ => (),
                }
            }
        }
    }
}

/// If a majority of the players wants to start, we should start the selected game.
#[allow(clippy::too_many_arguments)]
fn handle_start_event(
    players: Res<Players>,
    fonts: Res<Fonts>,
    games: Res<Games>,
    mut game_state: ResMut<State<GameState>>,
    selected_game_query: Query<&SelectedGame>,
    mut start_text: Query<&mut Text, With<StartText>>,
    mut player_start_vote: Local<PlayerVote>,
    mut start_event_reader: EventReader<StartVoteEvent>,
) {
    let voted_amount_before = player_start_vote.voted_amount();
    let total_amount_before = player_start_vote.total_amount();

    start_event_reader
        .iter()
        .for_each(|vote| player_start_vote.register_vote(vote));

    let voted_amount_after = player_start_vote.len();
    let total_amount_after = players.len();

    if voted_amount_before != voted_amount_after || total_amount_before != total_amount_after {
        player_start_vote.set_total_amount(total_amount_after);

        let required_amount = (player_start_vote.total_amount() / 2) + 1;
        if voted_amount_after >= required_amount {
            let SelectedGame(idx) = *selected_game_query.single().unwrap();
            let new_game_state = games.get(idx).unwrap().game_state;
            game_state.set(new_game_state).unwrap();
        } else {
            let font = fonts.regular.clone();
            let font_size = 24.0;
            start_text.single_mut().unwrap().sections = create_vote_text_sections(
                START_TEXT.into(),
                &players,
                &player_start_vote,
                required_amount,
                font,
                font_size,
            );
        }
    }
}

/// If a majority of the players wants to exit, we should return back to the menu.
fn handle_exit_event(
    players: Res<Players>,
    fonts: Res<Fonts>,
    mut game_state: ResMut<State<GameState>>,
    mut exit_text: Query<&mut Text, With<ExitText>>,
    mut player_exit_vote: Local<PlayerVote>,
    mut exit_event_reader: EventReader<ExitVoteEvent>,
) {
    let voted_amount_before = player_exit_vote.voted_amount();
    let total_amount_before = player_exit_vote.total_amount();

    exit_event_reader
        .iter()
        .for_each(|vote| player_exit_vote.register_vote(vote));

    let voted_amount_after = player_exit_vote.len();
    let total_amount_after = players.len();

    if voted_amount_before != voted_amount_after || total_amount_before != total_amount_after {
        player_exit_vote.set_total_amount(total_amount_after);

        let required_amount = (player_exit_vote.total_amount() / 2) + 1;
        if voted_amount_after >= required_amount && total_amount_after >= 2 {
            game_state.set(GameState::StartMenu).unwrap();
        } else {
            let font = fonts.regular.clone();
            let font_size = 24.0;
            exit_text.single_mut().unwrap().sections = create_vote_text_sections(
                EXIT_TEXT.into(),
                &players,
                &player_exit_vote,
                required_amount,
                font,
                font_size,
            );
        }
    }
}

// TODO: Move selected game to middle.
fn handle_changed_game(
    mut materials: ResMut<Assets<ColorMaterial>>,
    selected_game: Query<&SelectedGame, Changed<SelectedGame>>,
    mut selectable_games: Query<(
        &SelectableGame,
        &Handle<ColorMaterial>,
        &mut Transform,
        &Children,
    )>,
    mut text_query: Query<&mut Text>,
) {
    let new_selected_idx = if let Ok(SelectedGame(idx)) = selected_game.single() {
        *idx
    } else {
        // True if no changed done to `SelectedGame`.
        return;
    };

    for (selectable_game, color_handle, mut transform, children) in selectable_games.iter_mut() {
        let color = &mut materials.get_mut(color_handle).unwrap().color;
        let SelectableGame(cur_selected_idx) = selectable_game;
        color.set_a(if new_selected_idx == *cur_selected_idx {
            1.0
        } else {
            0.2
        });

        for child_entity in children.iter() {
            if let Ok(mut selectable_game_text) = text_query.get_mut(*child_entity) {
                for section in &mut selectable_game_text.sections {
                    let color = &mut section.style.color;
                    color.set_a(if new_selected_idx == *cur_selected_idx {
                        1.0
                    } else {
                        0.2
                    });
                }
            }
        }

        let pos_idx = *cur_selected_idx as isize - new_selected_idx as isize;
        transform.translation = Vec3::new(SELECTABLE_GAMES_SPACING * pos_idx as f32, 0.0, 0.0);
    }
}

fn reset_votes(
    mut start_event_writer: EventWriter<StartVoteEvent>,
    mut exit_event_writer: EventWriter<ExitVoteEvent>,
) {
    start_event_writer.send(StartVoteEvent(VoteEvent::Reset));
    exit_event_writer.send(ExitVoteEvent(VoteEvent::Reset));
}

fn setup_menu(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let bold_font = fonts.bold.clone();
    let bold_font_size = 64.0;
    let regular_font = fonts.regular.clone();
    let regular_font_size = 24.0;
    let white_color = Color::WHITE;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let header_text = Text::with_section(
        HEADER_TEXT,
        TextStyle {
            font: bold_font,
            font_size: bold_font_size,
            color: white_color,
        },
        TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    );

    let header_text_bundle = Text2dBundle {
        text: header_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(header_text_bundle)
        .insert(GameSelectionPlugin);

    let start_text = Text {
        sections: create_vote_text_sections(
            START_TEXT.into(),
            &players,
            &empty_player_vote,
            required_amount,
            regular_font.clone(),
            regular_font_size,
        ),
        alignment: TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    };

    let start_text_bundle = Text2dBundle {
        text: start_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0 - bold_font_size, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(start_text_bundle)
        .insert(StartText)
        .insert(GameSelectionPlugin);

    let exit_text = Text {
        sections: create_vote_text_sections(
            EXIT_TEXT.into(),
            &players,
            &empty_player_vote,
            required_amount,
            regular_font,
            regular_font_size,
        ),
        alignment: TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    };

    let exit_text_bundle = Text2dBundle {
        text: exit_text,
        transform: Transform::from_xyz(
            0.0,
            GAME_HEIGHT / 4.0 - bold_font_size - regular_font_size,
            0.0,
        ),
        ..Default::default()
    };

    commands
        .spawn_bundle(exit_text_bundle)
        .insert(ExitText)
        .insert(GameSelectionPlugin);

    let arrow_shape = Shape::polygon(30.0, Vec2::ZERO, 3);

    let left_arrow_bundle = GeometryBuilder::build_as(
        &arrow_shape,
        ShapeColors::new(white_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform {
            translation: Vec3::new(-300.0, 0.0, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    );

    let right_arrow_bundle = GeometryBuilder::build_as(
        &arrow_shape,
        ShapeColors::new(white_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform {
            translation: Vec3::new(300.0, 0.0, 0.0),
            rotation: Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    );

    commands
        .spawn_bundle(left_arrow_bundle)
        .insert(GameSelectionPlugin);
    commands
        .spawn_bundle(right_arrow_bundle)
        .insert(GameSelectionPlugin);
}

fn setup_selectable_games(mut commands: Commands, fonts: Res<Fonts>, games: Res<Games>) {
    let selected_idx = 0;
    commands
        .spawn()
        .insert(SelectedGame(selected_idx))
        .insert(GameSelectionPlugin);

    let bold_font = fonts.bold.clone();
    let white_color = Color::WHITE;

    let arrow_shape = Shape::polygon(30.0, Vec2::ZERO, 3);

    let left_arrow_bundle = GeometryBuilder::build_as(
        &arrow_shape,
        ShapeColors::new(white_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform {
            translation: Vec3::new(-300.0, 0.0, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    );

    let right_arrow_bundle = GeometryBuilder::build_as(
        &arrow_shape,
        ShapeColors::new(white_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform {
            translation: Vec3::new(300.0, 0.0, 0.0),
            rotation: Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
            ..Default::default()
        },
    );

    commands
        .spawn_bundle(left_arrow_bundle)
        .insert(GameSelectionPlugin);
    commands
        .spawn_bundle(right_arrow_bundle)
        .insert(GameSelectionPlugin);

    for (idx, game) in games.iter().enumerate() {
        let pos = Vec2::ZERO;
        spawn_selected_game(&mut commands, game, idx, pos, bold_font.clone());
    }
}

fn spawn_selected_game(
    commands: &mut Commands,
    game: &Game,
    idx: usize,
    pos: Vec2,
    font: Handle<Font>,
) {
    let white_color = Color::WHITE;

    let sprite_bundle = SpriteBundle {
        material: game.screenshot.clone(),
        transform: Transform {
            translation: Vec3::new(pos.x, pos.y, 0.0),
            scale: Vec3::new(0.25, 0.25, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };

    let text_bundle = Text2dBundle {
        text: Text {
            sections: vec![
                TextSection {
                    // Transform doesn't work for text when it is a child bundle.
                    value: " \n\n\n\n\n\n".into(),
                    style: TextStyle {
                        font: font.clone(),
                        font_size: 24.0,
                        color: white_color,
                    },
                },
                TextSection {
                    value: game.name.into(),
                    style: TextStyle {
                        font,
                        font_size: 64.0,
                        color: white_color,
                    },
                },
            ],
            alignment: TextAlignment {
                vertical: VerticalAlign::Bottom,
                horizontal: HorizontalAlign::Center,
            },
        },
        ..Default::default()
    };

    commands
        .spawn_bundle(sprite_bundle)
        .with_children(|parent| {
            parent.spawn_bundle(text_bundle);
        })
        .insert(SelectableGame(idx))
        .insert(GameSelectionPlugin);
}
