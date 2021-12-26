use std::ops::{Deref, DerefMut};

use bevy::{
    core::{Time, Timer},
    math::{Quat, Vec2},
    prelude::{
        AppBuilder, Assets, BuildChildren, Changed, Children, Color, Commands, CoreStage, Entity,
        EventReader, EventWriter, GlobalTransform, Handle, HorizontalAlign, IntoSystem, Local,
        Mesh, MeshBundle, ParallelSystemDescriptorCoercion, Plugin, Query, QuerySet,
        RenderPipelines, Res, ResMut, Shader, State, SystemSet, SystemStage, Transform,
        VerticalAlign, With,
    },
    render::{
        pipeline::{PipelineDescriptor, RenderPipeline},
        shader::{ShaderStage, ShaderStages},
    },
    text::{Font, Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    physics::{ColliderBundle, ColliderPositionSync, IntoEntity, RigidBodyBundle},
    prelude::{
        ActiveEvents, ColliderFlags, ColliderMassProps, ColliderMaterial, ColliderShape,
        ColliderType, IntersectionEvent, RigidBodyActivation, RigidBodyDamping, RigidBodyMassProps,
        RigidBodyPosition, RigidBodyType, RigidBodyVelocity,
    },
};
use rand::{prelude::SliceRandom, Rng};

use skitspel::{
    ActionEvent, ConnectedPlayers, DisconnectedPlayers, GameState, Player, PlayerId, Players,
    ACCEL_AMOUNT, GAME_HEIGHT, GAME_WIDTH, PLAYER_RADIUS, RAPIER_SCALE_FACTOR,
};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, AsBevyColor, Fonts, PlayerVote,
    Shape, VoteEvent,
};
use util_rapier::{
    create_circle_points, create_path_with_thickness, move_players, spawn_player, FRAGMENT_SHADER,
    VERTEX_SHADER,
};

const GAME_STATE: GameState = GameState::HockeyGame;

const SPAWN_POSITIONS_LEFT: [(f32, f32); 5] = [
    (-GAME_WIDTH * 0.375, GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.375, -GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.125, -GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.125, GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.25, 0.0),
];

const SPAWN_POSITIONS_RIGHT: [(f32, f32); 5] = [
    (GAME_WIDTH * 0.375, GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.375, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.125, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.125, GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.25, 0.0),
];

/// Component used to tag the text containing the score.
struct LeftScoreText;
struct RightScoreText;

const DASH_TEXT: &str = "Press A to dash (2 sec cooldown)\n";
const EXIT_TEXT: &str = "Press B to go back to main menu";

/// The height and width of the dash cooldown UI under the players.
const DASH_COOLDOWN_WIDTH: f32 = 100.0;
const DASH_COOLDOWN_HEIGHT: f32 = 10.0;

/// Used to tag which team a player belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Team {
    Left,
    Right,
}

/// Component used to tag the text containing which players belongs to which
/// team.
struct TeamText;

/// Tag used on the exit text.
struct ExitText;

/// Tag used on the puck.
struct Puck;

/// Tag used on the goals.
struct Goal;

/// Tag used od rink walls & corners.
struct Wall;

/// Component used to keep track of the current score. The left usize is the
/// score of the left team and the right usize is the score of the right team.
struct ScoreCount(usize, usize);

/// Event created when player with ID `PlayerId` dashes.
struct DashEvent(PlayerId);

/// Timer used to restrict how often a player can dash. This timer will started
/// when a dash is done and a player isn't allowed to dash again until this
/// timer runs out.
struct DashTimer(Timer);

/// Tag used on the UI under players that displays the cooldown of the dash.
struct DashCooldownUI;

impl Deref for DashTimer {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DashTimer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for DashTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(2.0, false))
    }
}

#[derive(Clone)]
pub struct HockeyGamePlugin;

impl Plugin for HockeyGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<DashEvent>()
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_map.system())
                    .with_system(setup_score.system())
                    .with_system(setup_players.system().label("players"))
                    .with_system(setup_screen_text.system().after("players")),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_disconnect.system().label("vote"))
                    .with_system(handle_connect.system().label("vote"))
                    .with_system(handle_player_input.system().label("vote").label("dash"))
                    .with_system(handle_exit_event.system().after("vote"))
                    .with_system(handle_goal.system().label("goal"))
                    .with_system(update_scoreboard.system())
                    .with_system(move_players.system())
                    .with_system(update_dash_timers.system().after("dash").label("timer"))
                    .with_system(handle_player_dash.system().after("dash").after("timer"))
                    .with_system(update_dash_ui.system().after("timer").before("goal")),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE)
                    .with_system(despawn_system::<HockeyGamePlugin>.system()),
            )
            .add_stage_after(
                CoreStage::PostUpdate,
                "dash_ui_rotation",
                SystemStage::single_threaded(),
            )
            .add_system_to_stage("dash_ui_rotation", update_dash_ui_transform.system());
    }
}

// There is currently no good way to handle rotation of a child entity relative
// to its parent. This is a hack to make it work (see also "dash_ui_rotation" stage).
// See: https://github.com/bevyengine/bevy/issues/1780#issuecomment-939385391
fn update_dash_ui_transform(
    player_query: Query<&Children, With<DashTimer>>,
    mut dash_ui_query: Query<&mut GlobalTransform, With<DashCooldownUI>>,
) {
    for children in player_query.iter() {
        for child_entity in children.iter() {
            if let Ok(mut transform) = dash_ui_query.get_mut(*child_entity) {
                transform.rotation = Quat::from_rotation_y(0.0);
                transform.translation.y =
                    transform.translation.y - PLAYER_RADIUS - DASH_COOLDOWN_HEIGHT;
            }
        }
    }
}

fn handle_connect(
    mut commands: Commands,
    connected_players: Res<ConnectedPlayers>,
    players_playing: Query<&Team, With<PlayerId>>,
) {
    for player in connected_players.values() {
        let mut left_team_count = 0;
        let mut right_team_count = 0;
        for team in players_playing.iter() {
            match team {
                Team::Left => left_team_count += 1,
                Team::Right => right_team_count += 1,
            }
        }

        let (team, spawn_pos) = if left_team_count < right_team_count {
            let idx = rand::thread_rng().gen_range(0..SPAWN_POSITIONS_LEFT.len());
            (Team::Left, SPAWN_POSITIONS_LEFT[idx])
        } else {
            let idx = rand::thread_rng().gen_range(0..SPAWN_POSITIONS_RIGHT.len());
            (Team::Right, SPAWN_POSITIONS_RIGHT[idx])
        };

        spawn_hockey_player(&mut commands, player, spawn_pos.into(), team);
    }
}

fn handle_disconnect(
    mut commands: Commands,
    disconnected_players: Res<DisconnectedPlayers>,
    players_playing: Query<(Entity, &PlayerId)>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if !disconnected_players.is_empty() {
        for (entity, player_id) in players_playing.iter() {
            if disconnected_players.contains(player_id) {
                exit_event_writer.send(VoteEvent::Value(*player_id, false));
                despawn_entity(&mut commands, entity);
            }
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_scoreboard(
    mut commands: Commands,
    players: Res<Players>,
    fonts: Res<Fonts>,
    score_change: Query<&ScoreCount, Changed<ScoreCount>>,
    mut score_text_query: QuerySet<(
        Query<&mut Text, With<LeftScoreText>>,
        Query<&mut Text, With<RightScoreText>>,
    )>,
    players_playing: Query<(&PlayerId, &Team)>,
    old_team_text: Query<Entity, With<TeamText>>,
    mut players_playing_count: Local<usize>,
) {
    for new_score_count in score_change.iter() {
        let ScoreCount(left_score, right_score) = new_score_count;

        let mut left_score_text = score_text_query.q0_mut().single_mut().unwrap();
        if let Some(text_section) = left_score_text.sections.first_mut() {
            text_section.value = left_score.to_string();
        }

        let mut right_score_text = score_text_query.q1_mut().single_mut().unwrap();
        if let Some(text_section) = right_score_text.sections.first_mut() {
            text_section.value = right_score.to_string();
        }
    }

    let cur_players_playing_count = players_playing.iter().count();
    if *players_playing_count != cur_players_playing_count || score_change.iter().count() > 0 {
        *players_playing_count = cur_players_playing_count;

        let font = fonts.bold.clone();
        let font_size = 128.0;
        update_team_text(
            &mut commands,
            &players,
            players_playing,
            old_team_text,
            font,
            font_size,
        );
    }
}

fn update_dash_timers(time: Res<Time>, mut dash_timer_query: Query<&mut DashTimer>) {
    for mut timer in dash_timer_query.iter_mut() {
        timer.tick(time.delta());
    }
}

/// Dashes the player in a direction. There are two cases for directions:
///  1. If no movement button is held in at the moment, the player will dash
///     in the direction that it is traveling.
///  2. If at leasy one direction key is pressed at the moment, the player will
///     dash in that direction.
fn handle_player_dash(
    players: Res<Players>,
    mut player_query: Query<(
        &PlayerId,
        &mut DashTimer,
        &mut RigidBodyVelocity,
        &RigidBodyMassProps,
    )>,
    mut dash_event_reader: EventReader<DashEvent>,
) {
    for DashEvent(event_player_id) in dash_event_reader.iter() {
        // TODO: More performant way to get player_query from player_id?
        for (player_id, mut timer, mut velocity, mass) in player_query.iter_mut() {
            if event_player_id == player_id && timer.finished() {
                if let Some(player) = players.get(player_id) {
                    let heading_vec = if player.movement_x() != 0.0 || player.movement_y() != 0.0 {
                        Vec2::new(player.movement_x(), player.movement_y()).normalize()
                    } else {
                        Vec2::from(velocity.linvel).normalize()
                    };
                    velocity.apply_impulse(mass, (heading_vec * ACCEL_AMOUNT).into());
                    timer.reset();
                }
            }
        }
    }
}

fn update_dash_ui(
    mut commands: Commands,
    mut player_query: Query<(Entity, &mut DashTimer, &Children)>,
) {
    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let green_color = Color::rgb(0.1, 1.0, 0.1);

    for (entity, timer, children) in player_query.iter_mut() {
        if timer.just_finished() {
            // The timer just finished this tick. Create a single "finished"
            // bar in green.
            for child_entity in children.iter() {
                despawn_entity(&mut commands, *child_entity);
            }

            let shape_bundle_finished = GeometryBuilder::build_as(
                &Shape::rectangle(DASH_COOLDOWN_WIDTH, DASH_COOLDOWN_HEIGHT, Vec2::ZERO),
                ShapeColors::new(green_color),
                DrawMode::Fill(FillOptions::DEFAULT),
                Transform::from_xyz(0.0, 0.0, 0.0),
            );

            let new_child = commands
                .spawn_bundle(shape_bundle_finished)
                .insert(DashCooldownUI)
                .id();

            commands.entity(entity).push_children(&[new_child]);
        } else if timer.finished() {
            // The timer was already finished before this tick which means that
            // the graphic should already be correct, do nothing.
        } else {
            // The timer is currently counting down, update the UI according
            // to the current timer.
            for child_entity in children.iter() {
                despawn_entity(&mut commands, *child_entity);
            }

            let shape_bundle_left = GeometryBuilder::build_as(
                &Shape::rectangle(
                    DASH_COOLDOWN_WIDTH * timer.percent(),
                    DASH_COOLDOWN_HEIGHT,
                    Vec2::ZERO,
                ),
                ShapeColors::new(red_color),
                DrawMode::Fill(FillOptions::DEFAULT),
                Transform::from_xyz(0.0, 0.0, 0.0),
            );

            let new_children = commands
                .spawn_bundle(shape_bundle_left)
                .insert(DashCooldownUI)
                .id();

            commands.entity(entity).push_children(&[new_children]);
        }
    }
}

/// Checks collisions between ball and goal. Updates scores and resets ball
/// & player positions if a collision is found.
fn handle_goal(
    mut commands: Commands,
    mut intersection_event: EventReader<IntersectionEvent>,
    mut players: ResMut<Players>,
    players_playing: Query<(Entity, &PlayerId, &Team)>,
    mut score_count: Query<&mut ScoreCount>,
    mut puck_query: Query<(&mut RigidBodyPosition, &mut RigidBodyVelocity), With<Puck>>,
    goal_query: Query<&Team, With<Goal>>,
) {
    for intersection in intersection_event.iter() {
        if intersection.intersecting {
            let entity_a = intersection.collider1.entity();
            let entity_b = intersection.collider2.entity();
            let goal_entity =
                if puck_query.get_mut(entity_a).is_ok() && goal_query.get(entity_b).is_ok() {
                    entity_b
                } else if puck_query.get_mut(entity_b).is_ok() && goal_query.get(entity_a).is_ok() {
                    entity_a
                } else {
                    continue;
                };

            let ScoreCount(ref mut left_score, ref mut right_score) =
                *score_count.single_mut().unwrap();

            let goal_belonging_to_team = goal_query.get(goal_entity).unwrap();
            let scoring_team = match goal_belonging_to_team {
                Team::Left => {
                    *right_score += 1;
                    Team::Right
                }
                Team::Right => {
                    *left_score += 1;
                    Team::Left
                }
            };

            // Despawn the old players and respawn them with new shapes and on
            // their side of the rink.
            for (entity, ..) in players_playing.iter() {
                despawn_entity(&mut commands, entity);
            }

            // Used to get randomized spawns.
            let mut left_spawn_positions = SPAWN_POSITIONS_LEFT.to_vec();
            left_spawn_positions.shuffle(&mut rand::thread_rng());
            let mut right_spawn_positions = SPAWN_POSITIONS_RIGHT.to_vec();
            right_spawn_positions.shuffle(&mut rand::thread_rng());

            for (_, player_id, team) in players_playing.iter() {
                if let Some(player) = players.get_mut(player_id) {
                    if *team == scoring_team {
                        player.increment_score();
                    }

                    let spawn_pos = match team {
                        Team::Left => left_spawn_positions.pop().unwrap(),
                        Team::Right => right_spawn_positions.pop().unwrap(),
                    };

                    player.reset_action();
                    spawn_hockey_player(&mut commands, player, spawn_pos.into(), *team);
                }
            }

            // Reset puck to middle of rink.
            let (mut pos, mut velocity) = puck_query.single_mut().unwrap();
            pos.position = Vec2::ZERO.into();
            velocity.linvel = Vec2::ZERO.into();
            velocity.angvel = 0.0;
        }
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    mut dash_event_writer: EventWriter<DashEvent>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                match prev_action {
                    ActionEvent::APressed => {
                        dash_event_writer.send(DashEvent(player.id()));
                    }

                    ActionEvent::BPressed => {
                        exit_event_writer.send(VoteEvent::Flip(player.id()));
                    }

                    _ => (),
                }
            }
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
    mut exit_event_reader: EventReader<VoteEvent>,
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
        if voted_amount_after >= required_amount {
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

fn update_team_text(
    commands: &mut Commands,
    players: &Players,
    players_playing: Query<(&PlayerId, &Team)>,
    old_team_text: Query<Entity, With<TeamText>>,
    font: Handle<Font>,
    font_size: f32,
) {
    for entity in old_team_text.iter() {
        despawn_entity(commands, entity);
    }

    let mut left_team_player_ids = players_playing
        .iter()
        .filter(|(_, team)| matches!(team, Team::Left))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    left_team_player_ids.sort_unstable();

    let left_team_text_sections =
        create_team_text_sections(&left_team_player_ids, players, font.clone(), font_size);

    let left_team_text_bundle = Text2dBundle {
        text: Text {
            sections: left_team_text_sections,
            alignment: TextAlignment {
                vertical: VerticalAlign::Bottom,
                horizontal: HorizontalAlign::Center,
            },
        },
        transform: Transform::from_xyz(-GAME_WIDTH / 4.0, GAME_HEIGHT / 4.0 - font_size / 2.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(left_team_text_bundle)
        .insert(TeamText)
        .insert(HockeyGamePlugin);

    let right_team_player_ids = players_playing
        .iter()
        .filter(|(_, team)| matches!(team, Team::Right))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    left_team_player_ids.sort_unstable();

    let right_team_text_sections =
        create_team_text_sections(&right_team_player_ids, players, font, font_size);

    let right_team_text_bundle = Text2dBundle {
        text: Text {
            sections: right_team_text_sections,
            alignment: TextAlignment {
                vertical: VerticalAlign::Bottom,
                horizontal: HorizontalAlign::Center,
            },
        },
        transform: Transform::from_xyz(GAME_WIDTH / 4.0, GAME_HEIGHT / 4.0 - font_size / 2.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(right_team_text_bundle)
        .insert(TeamText)
        .insert(HockeyGamePlugin);
}

fn create_team_text_sections(
    team_player_ids: &[PlayerId],
    players: &Players,
    font: Handle<Font>,
    font_size: f32,
) -> Vec<TextSection> {
    let mut text_sections = Vec::with_capacity(team_player_ids.len());
    for player_id in team_player_ids {
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
    text_sections
}

fn reset_votes(mut exit_event_writer: EventWriter<VoteEvent>) {
    exit_event_writer.send(VoteEvent::Reset);
}

fn spawn_hockey_player(commands: &mut Commands, player: &Player, spawn_pos: Vec2, team: Team) {
    let red_color = Color::rgb(1.0, 0.1, 0.1);

    let mut entity_commands = spawn_player(
        commands,
        player.id(),
        player.color().as_bevy(),
        spawn_pos,
        PLAYER_RADIUS,
    );

    let shape_bundle = GeometryBuilder::build_as(
        &Shape::rectangle(DASH_COOLDOWN_WIDTH, DASH_COOLDOWN_HEIGHT, Vec2::ZERO),
        ShapeColors::new(red_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(0.0, -PLAYER_RADIUS, 0.0),
    );

    entity_commands.with_children(|parent| {
        parent.spawn_bundle(shape_bundle).insert(DashCooldownUI);
    });

    entity_commands
        .insert(DashTimer::default())
        .insert(team)
        .insert(HockeyGamePlugin);
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let bold_font = fonts.bold.clone();
    let bold_font_size = 128.0;
    let regular_font = fonts.regular.clone();
    let regular_font_size = 24.0;
    let font_color = Color::WHITE;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let dash_text = Text::with_section(
        DASH_TEXT,
        TextStyle {
            font: regular_font.clone(),
            font_size: regular_font_size,
            color: font_color,
        },
        TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    );

    let dash_text_bundle = Text2dBundle {
        text: dash_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(dash_text_bundle)
        .insert(HockeyGamePlugin);

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
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0 - regular_font_size, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(exit_text_bundle)
        .insert(ExitText)
        .insert(HockeyGamePlugin);

    let zero_score_text = Text::with_section(
        "0",
        TextStyle {
            font: bold_font,
            font_size: bold_font_size,
            color: Color::WHITE,
        },
        TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    );

    let left_score_text_bundle = Text2dBundle {
        text: zero_score_text.clone(),
        transform: Transform::from_xyz(-GAME_WIDTH / 4.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(left_score_text_bundle)
        .insert(LeftScoreText)
        .insert(HockeyGamePlugin);

    let right_score_text_bundle = Text2dBundle {
        text: zero_score_text,
        transform: Transform::from_xyz(GAME_WIDTH / 4.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(right_score_text_bundle)
        .insert(RightScoreText)
        .insert(HockeyGamePlugin);
}

fn setup_score(mut commands: Commands) {
    commands
        .spawn()
        .insert(ScoreCount(0, 0))
        .insert(HockeyGamePlugin);
}

fn setup_players(mut commands: Commands, mut players: ResMut<Players>) {
    let mut left_spawn_positions = SPAWN_POSITIONS_LEFT.to_vec();
    left_spawn_positions.shuffle(&mut rand::thread_rng());
    let mut right_spawn_positions = SPAWN_POSITIONS_RIGHT.to_vec();
    right_spawn_positions.shuffle(&mut rand::thread_rng());

    let mut players_shuffled = players.values_mut().collect::<Vec<_>>();
    players_shuffled.shuffle(&mut rand::thread_rng());

    let half_idx = players_shuffled.len() / 2;
    for (i, player) in players_shuffled.into_iter().enumerate() {
        let (team, spawn_pos) = if i < half_idx {
            (Team::Left, left_spawn_positions.pop().unwrap())
        } else {
            (Team::Right, right_spawn_positions.pop().unwrap())
        };

        player.reset_action();
        spawn_hockey_player(&mut commands, player, spawn_pos.into(), team);
    }
}

fn setup_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut pipelines: ResMut<Assets<PipelineDescriptor>>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    let thickness = 10.0;
    let ht = thickness / 2.0;
    let post_radius = thickness * 2.0;
    let corner_radius = 200.0;
    let amount_of_corner_points = 16;

    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let grey_color = Color::rgb(0.3, 0.3, 0.3);
    let white_color = Color::rgb(1.0, 1.0, 1.0);

    let pipeline_handle = pipelines.add(PipelineDescriptor::default_config(ShaderStages {
        vertex: shaders.add(Shader::from_glsl(ShaderStage::Vertex, VERTEX_SHADER)),
        fragment: Some(shaders.add(Shader::from_glsl(ShaderStage::Fragment, FRAGMENT_SHADER))),
    }));

    let render_pipelines =
        RenderPipelines::from_pipelines(vec![RenderPipeline::new(pipeline_handle)]);

    let top_wall_vertices = [
        Vec2::new(
            (-GAME_WIDTH / 2.0) + ht + corner_radius,
            (GAME_HEIGHT / 2.0) - ht,
        ),
        Vec2::new(
            (GAME_WIDTH / 2.0) - ht - corner_radius,
            (GAME_HEIGHT / 2.0) - ht,
        ),
    ];
    spawn_rink_wall(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        &top_wall_vertices,
        grey_color,
        thickness,
    );

    let right_wall_vertices = [
        Vec2::new(
            (GAME_WIDTH / 2.0) - ht,
            (GAME_HEIGHT / 2.0) - ht - corner_radius,
        ),
        Vec2::new(
            (GAME_WIDTH / 2.0) - ht,
            (-GAME_HEIGHT / 2.0) + ht + corner_radius,
        ),
    ];
    spawn_rink_wall(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        &right_wall_vertices,
        grey_color,
        thickness,
    );

    let bottom_wall_vertices = [
        Vec2::new(
            (-GAME_WIDTH / 2.0) + ht + corner_radius,
            (-GAME_HEIGHT / 2.0) + ht,
        ),
        Vec2::new(
            (GAME_WIDTH / 2.0) - ht - corner_radius,
            (-GAME_HEIGHT / 2.0) + ht,
        ),
    ];
    spawn_rink_wall(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        &bottom_wall_vertices,
        grey_color,
        thickness,
    );

    let left_wall_vertices = [
        Vec2::new(
            (-GAME_WIDTH / 2.0) + ht,
            (GAME_HEIGHT / 2.0) - ht - corner_radius,
        ),
        Vec2::new(
            (-GAME_WIDTH / 2.0) + ht,
            (-GAME_HEIGHT / 2.0) + ht + corner_radius,
        ),
    ];
    spawn_rink_wall(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        &left_wall_vertices,
        grey_color,
        thickness,
    );

    let top_left_pos = Vec2::new(
        -GAME_WIDTH / 2.0 + thickness / 2.0 + corner_radius,
        GAME_HEIGHT / 2.0 - thickness / 2.0 - corner_radius,
    );
    let start_angle = std::f32::consts::FRAC_PI_2;
    let end_angle = std::f32::consts::PI;
    spawn_rink_corner(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        corner_radius,
        top_left_pos,
        start_angle,
        end_angle,
        amount_of_corner_points,
        grey_color,
        thickness,
    );

    let top_right_pos = Vec2::new(
        GAME_WIDTH / 2.0 - thickness / 2.0 - corner_radius,
        GAME_HEIGHT / 2.0 - thickness / 2.0 - corner_radius,
    );
    let start_angle = 0.0;
    let end_angle = std::f32::consts::FRAC_PI_2;
    spawn_rink_corner(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        corner_radius,
        top_right_pos,
        start_angle,
        end_angle,
        amount_of_corner_points,
        grey_color,
        thickness,
    );

    let bottom_right_pos = Vec2::new(
        GAME_WIDTH / 2.0 - thickness / 2.0 - corner_radius,
        -GAME_HEIGHT / 2.0 + thickness / 2.0 + corner_radius,
    );
    let start_angle = std::f32::consts::PI * 1.5;
    let end_angle = std::f32::consts::PI * 2.0;
    spawn_rink_corner(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        corner_radius,
        bottom_right_pos,
        start_angle,
        end_angle,
        amount_of_corner_points,
        grey_color,
        thickness,
    );

    let bottom_left_pos = Vec2::new(
        -GAME_WIDTH / 2.0 + thickness / 2.0 + corner_radius,
        -GAME_HEIGHT / 2.0 + thickness / 2.0 + corner_radius,
    );
    let start_angle = std::f32::consts::PI;
    let end_angle = std::f32::consts::PI * 1.5;
    spawn_rink_corner(
        &mut commands,
        &mut meshes,
        render_pipelines,
        corner_radius,
        bottom_left_pos,
        start_angle,
        end_angle,
        amount_of_corner_points,
        grey_color,
        thickness,
    );

    let top_left_pos = Vec2::new(-GAME_WIDTH / 4.0, GAME_HEIGHT / 2.0 - thickness / 2.0);
    spawn_post(&mut commands, top_left_pos, post_radius, grey_color);

    let top_right_pos = Vec2::new(GAME_WIDTH / 4.0, GAME_HEIGHT / 2.0 - thickness / 2.0);
    spawn_post(&mut commands, top_right_pos, post_radius, grey_color);

    let bottom_left_pos = Vec2::new(-GAME_WIDTH / 4.0, -GAME_HEIGHT / 2.0 + thickness / 2.0);
    spawn_post(&mut commands, bottom_left_pos, post_radius, grey_color);

    let bottom_right_pos = Vec2::new(GAME_WIDTH / 4.0, -GAME_HEIGHT / 2.0 + thickness / 2.0);
    spawn_post(&mut commands, bottom_right_pos, post_radius, grey_color);

    let goal_width = thickness + 1.0;
    let goal_height = 400.0;

    let left_goal_pos = Vec2::new(-GAME_WIDTH / 2.0 + goal_width / 2.0, 0.0);
    spawn_goal(
        &mut commands,
        left_goal_pos,
        goal_width,
        goal_height,
        red_color,
        grey_color,
        Team::Left,
    );

    let right_goal_pos = Vec2::new(GAME_WIDTH / 2.0 - goal_width / 2.0, 0.0);
    spawn_goal(
        &mut commands,
        right_goal_pos,
        goal_width,
        goal_height,
        red_color,
        grey_color,
        Team::Right,
    );

    spawn_puck(&mut commands, Vec2::ZERO, 20.0, white_color);
}

fn spawn_rink_wall(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    render_pipelines: RenderPipelines,
    vertices: &[Vec2],
    color: Color,
    thickness: f32,
) {
    let (mesh, colliders) = create_path_with_thickness(
        vertices,
        color,
        thickness,
        ColliderType::Solid,
        ActiveEvents::empty(),
        false,
    );

    let mut entity_commands = commands.spawn_bundle(MeshBundle {
        mesh: meshes.add(mesh),
        render_pipelines,
        ..Default::default()
    });

    colliders.into_iter().for_each(|collider| {
        entity_commands.with_children(|parent| {
            parent.spawn_bundle(collider).insert(Wall);
        });
    });

    entity_commands.insert(HockeyGamePlugin);
}

#[allow(clippy::too_many_arguments)]
fn spawn_rink_corner(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    render_pipelines: RenderPipelines,
    radius: f32,
    pos: Vec2,
    start_angle: f32,
    end_angle: f32,
    amount_of_points: usize,
    color: Color,
    thickness: f32,
) {
    let points = create_circle_points(radius, pos, start_angle, end_angle, amount_of_points);
    let (mesh, colliders) = create_path_with_thickness(
        &points
            .iter()
            .map(|p| Vec2::new(p.coords.x, p.coords.y))
            .collect::<Vec<_>>(),
        color,
        thickness,
        ColliderType::Solid,
        ActiveEvents::empty(),
        false,
    );

    let mut entity_commands = commands.spawn_bundle(MeshBundle {
        mesh: meshes.add(mesh),
        render_pipelines,
        ..Default::default()
    });

    colliders.into_iter().for_each(|collider| {
        entity_commands.with_children(|parent| {
            parent.spawn_bundle(collider).insert(Wall);
        });
    });

    entity_commands.insert(Wall).insert(HockeyGamePlugin);
}

fn spawn_goal(
    commands: &mut Commands,
    mut pos: Vec2,
    mut width: f32,
    mut height: f32,
    color: Color,
    post_color: Color,
    team: Team,
) {
    let post_radius = width * 1.5;

    let top_post_pos = Vec2::new(pos.x, pos.y + height / 2.0);
    spawn_post(commands, top_post_pos, post_radius, post_color);

    let bottom_post_pos = Vec2::new(pos.x, pos.y - height / 2.0);
    spawn_post(commands, bottom_post_pos, post_radius, post_color);

    let shape = GeometryBuilder::build_as(
        &Shape::rectangle(width, height, Vec2::ZERO),
        ShapeColors::new(color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 0.0),
    );

    pos /= RAPIER_SCALE_FACTOR;
    width /= RAPIER_SCALE_FACTOR;
    height /= RAPIER_SCALE_FACTOR;

    let rigid_body = RigidBodyBundle {
        body_type: RigidBodyType::Static,
        damping: RigidBodyDamping {
            linear_damping: 0.5,
            angular_damping: 0.5,
        },
        position: pos.into(),
        activation: RigidBodyActivation::cannot_sleep(),
        ..Default::default()
    };

    let collider = ColliderBundle {
        collider_type: ColliderType::Sensor,
        flags: ColliderFlags {
            active_events: ActiveEvents::INTERSECTION_EVENTS,
            ..Default::default()
        },
        shape: ColliderShape::cuboid(width / 2.0, height / 2.0),
        material: ColliderMaterial {
            friction: 0.3,
            restitution: 0.5,
            ..Default::default()
        },
        mass_properties: ColliderMassProps::Density(1.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(rigid_body)
        .insert_bundle(shape)
        .insert_bundle(collider)
        .insert(team)
        .insert(Goal)
        .insert(HockeyGamePlugin);
}

fn spawn_post(commands: &mut Commands, mut pos: Vec2, mut radius: f32, color: Color) {
    let shape = GeometryBuilder::build_as(
        &Shape::circle(radius, Vec2::ZERO),
        ShapeColors::new(color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 1.0),
    );

    pos /= RAPIER_SCALE_FACTOR;
    radius /= RAPIER_SCALE_FACTOR;

    let rigid_body = RigidBodyBundle {
        body_type: RigidBodyType::Static,
        damping: RigidBodyDamping {
            linear_damping: 0.5,
            angular_damping: 0.5,
        },
        position: pos.into(),
        activation: RigidBodyActivation::cannot_sleep(),
        ..Default::default()
    };

    let collider = ColliderBundle {
        collider_type: ColliderType::Solid,
        shape: ColliderShape::ball(radius),
        material: ColliderMaterial {
            friction: 0.3,
            restitution: 0.5,
            ..Default::default()
        },
        mass_properties: ColliderMassProps::Density(1.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(rigid_body)
        .insert_bundle(shape)
        .insert_bundle(collider)
        .insert(HockeyGamePlugin);
}

fn spawn_puck(commands: &mut Commands, mut pos: Vec2, mut radius: f32, color: Color) {
    let shape = GeometryBuilder::build_as(
        &Shape::circle(radius, Vec2::ZERO),
        ShapeColors::new(color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 0.0),
    );

    pos /= RAPIER_SCALE_FACTOR;
    radius /= RAPIER_SCALE_FACTOR;

    let rigid_body = RigidBodyBundle {
        body_type: RigidBodyType::Dynamic,
        damping: RigidBodyDamping {
            linear_damping: 0.5,
            angular_damping: 0.5,
        },
        position: pos.into(),
        activation: RigidBodyActivation::cannot_sleep(),
        ..Default::default()
    };

    let collider = ColliderBundle {
        collider_type: ColliderType::Solid,

        shape: ColliderShape::ball(radius),
        material: ColliderMaterial {
            friction: 0.05,
            restitution: 0.9,
            ..Default::default()
        },
        mass_properties: ColliderMassProps::Density(1.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(rigid_body)
        .insert_bundle(shape)
        .insert_bundle(collider)
        .insert(Puck)
        .insert(HockeyGamePlugin)
        .insert(ColliderPositionSync::Discrete);
}
