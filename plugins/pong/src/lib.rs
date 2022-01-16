use std::{
    cmp::Reverse,
    collections::HashSet,
    f32::consts::{FRAC_PI_4, FRAC_PI_8, PI, TAU},
};

use bevy::{
    core::Time,
    math::{Quat, Vec2, Vec3},
    prelude::{
        AppBuilder, Assets, Color, Commands, Entity, EventReader, EventWriter, HorizontalAlign,
        IntoSystem, Local, Mesh, MeshBundle, ParallelSystemDescriptorCoercion, Plugin, Query,
        RenderPipelines, Res, ResMut, State, SystemSet, Transform, VerticalAlign, With,
    },
    render::{mesh::Indices, pipeline::PrimitiveTopology},
    text::{Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};

use rand::{prelude::SliceRandom, Rng};

use colliders::Colliders;
use skitspel::{ActionEvent, DisconnectedPlayers, GameState, PlayerId, Players, GAME_HEIGHT};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, handle_start_timer,
    setup_start_timer, AsBevyColor, Fonts, PlayerVote, Shape, StartTimer, VoteEvent,
};
use util_rapier::{create_circle_points, indices_from_vertices, vertices_with_thickness};

mod colliders;
mod util;

const GAME_STATE: GameState = GameState::PongGame;

const EXIT_TEXT: &str = "Press B to go back to main menu";

/// The radius, from the middle of the screen, of the circle that represents
/// the goals. The goals will be drawn `+- GOAL_THICKNESS / 2.0` around this
/// `GOAL_RADIUS` radius.
const GOAL_RADIUS: f32 = GAME_HEIGHT / 2.0 - GOAL_THICKNESS * 4.0;
const GOAL_RADIUS_INNER: f32 = GOAL_RADIUS - GOAL_THICKNESS / 2.0;

/// The thickness of the coal circle.
const GOAL_THICKNESS: f32 = 10.0;

/// The radius, from the middle of the screen, of the circle that represents
/// the player paddles. The paddles will be drawn `+- PLAYER_THICKNESS / 2.0`
/// around this `PLAYER_RADIUS` radius.
const PLAYER_RADIUS: f32 = GOAL_RADIUS - GOAL_THICKNESS / 2.0 - PLAYER_THICKNESS / 2.0 - 5.0;
const PLAYER_RADIUS_INNER: f32 = PLAYER_RADIUS - PLAYER_THICKNESS / 2.0;

/// The thickness/size of the player circle.
const PLAYER_THICKNESS: f32 = 15.0;

/// The length of the player in relation to the size of its goal. The player will
/// conver `PLAYER_LENGTH_FRAC` of the goal.
const PLAYER_LENGTH_FRAC: f32 = 1.0 / 6.0;

/// The radius of the ball.
const BALL_RADIUS: f32 = 10.0;

/// The speed that the ball starts with. The speed of the ball will be increased
/// `BALL_SPEED_INCREMENT` amount for every bounce that is done.
const BALL_START_SPEED: f32 = 150.0;

/// The amount of speed that is added to the ball everytime it collides with
/// a player paddle.
const BALL_SPEED_INCREMENT: f32 = 25.0;

/// The speed of the players paddle. This is written in the unit
/// `amount of radians per second`.
const PLAYER_CONSTANT_SPEED: f32 = PI / 4.0;

/// How long the timer between rounds are in seconds.
const START_TIMER_TIME: usize = 3;

/// The amount of points that will be plotted for the circle. A higher number
/// will make the circle more circular "smooth".
const AMOUNT_OF_POINTS: usize = 64;

/// Tag used on the exit text.
struct ExitText;

/// Tag used on the ball.
#[derive(Default)]
struct Ball {
    /// The current speed that the ball is traveling. This will be increased
    /// everytime a collision occurs.
    speed: f32,
    /// The angle in which the ball is currently traveling.
    angle: f32,
}

impl Ball {
    /// Moves the ball to the middle of the screen, resets the speed to the start
    /// speed and randomizes a direction in which it will start traveling.
    fn reset(&mut self, transform: &mut Transform) {
        self.speed = BALL_START_SPEED;
        self.angle = rand::thread_rng().gen_range(0.0..TAU);
        transform.translation.x = 0.0;
        transform.translation.y = 0.0;
    }
}

/// Tag used on the walls (the circle representing the goals).
struct Wall;

/// Component used to tag the text containing the score.
struct ScoreText;

/// Event created when a player dies and its character is removed.
#[derive(Debug)]
struct DeathEvent(PlayerId);

#[derive(Clone, Default)]
pub struct PongGamePlugin;

impl Plugin for PongGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<DeathEvent>()
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_colliders.system())
                    .with_system(setup_start_timer::<PongGamePlugin, START_TIMER_TIME>.system())
                    .with_system(setup_screen_text.system())
                    .with_system(setup_ball.system()),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_disconnect.system().label("vote"))
                    .with_system(handle_player_input.system().label("vote").label("jump"))
                    .with_system(handle_exit_event.system().after("vote"))
                    .with_system(update_scoreboard.system())
                    .with_system(move_players.system().label("players").after("vote"))
                    .with_system(move_ball.system().label("ball").after("players"))
                    .with_system(handle_collision.system().label("collision").after("ball"))
                    .with_system(handle_reset.system().label("reset").after("collision"))
                    .with_system(handle_start_timer.system().label("start").after("reset")),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE)
                    .with_system(despawn_system::<PongGamePlugin>.system()),
            );
    }
}

fn handle_disconnect(
    mut commands: Commands,
    disconnected_players: Res<DisconnectedPlayers>,
    players_playing: Query<(Entity, &PlayerId)>,
    mut colliders_query: Query<&mut Colliders>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if !disconnected_players.is_empty() {
        for (entity, player_id) in players_playing.iter() {
            if disconnected_players.contains(player_id) {
                exit_event_writer.send(VoteEvent::Value(*player_id, false));

                // TODO: Can there be problems with removing the graphic & collider
                //       for the player? Is there a possibility that the entity
                //       is used in the same tick that can cause a panic?
                colliders_query
                    .single_mut()
                    .unwrap()
                    .remove_player(*player_id);
                despawn_entity(&mut commands, entity);
            }
        }
    }
}

fn update_scoreboard(
    mut commands: Commands,
    fonts: Res<Fonts>,
    players: Res<Players>,
    score_text_query: Query<Entity, With<ScoreText>>,
) {
    if players.is_changed() {
        for entity in score_text_query.iter() {
            despawn_entity(&mut commands, entity);
        }

        let font = fonts.regular.clone();

        let mut sorted_players = players.values().collect::<Vec<_>>();
        sorted_players.sort_unstable_by_key(|p| Reverse(p.score()));

        let mut text_sections = Vec::default();
        text_sections.push(TextSection {
            value: "SCORE".into(),
            style: TextStyle {
                font: font.clone(),
                font_size: 32.0,
                color: Color::WHITE,
            },
        });

        for (idx, player) in sorted_players.iter().enumerate() {
            let color = player.color().as_bevy();
            text_sections.push(TextSection {
                value: format!("\n{}. {}", idx + 1, player.score()),
                style: TextStyle {
                    font: font.clone(),
                    font_size: 24.0,
                    color,
                },
            });
        }

        let top_margin = 15.0 + 3.5 * 24.0;
        let text_bundle = Text2dBundle {
            text: Text {
                sections: text_sections,
                alignment: TextAlignment {
                    vertical: VerticalAlign::Bottom,
                    horizontal: HorizontalAlign::Center,
                },
            },
            transform: Transform::from_xyz(0.0, (GAME_HEIGHT / 2.0) - top_margin, 0.0),
            ..Default::default()
        };

        commands
            .spawn_bundle(text_bundle)
            .insert(PongGamePlugin)
            .insert(ScoreText);
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                match prev_action {
                    ActionEvent::APressed => {
                        // TODO: Add action when pressing A?
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

fn move_players(
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut Transform)>,
    mut colliders_query: Query<&mut Colliders>,
) {
    let delta_tick = time.delta_seconds();
    let mut colliders = colliders_query.single_mut().unwrap();

    for (player_id, mut transform) in player_query.iter_mut() {
        if let Some(player) = players.get(player_id) {
            let movement_x = player.movement_x();
            if movement_x != 0.0 {
                let sign = if movement_x > 0.0 { 1.0 } else { -1.0 };
                let delta_angle = sign * PLAYER_CONSTANT_SPEED * delta_tick;

                // Unable to move outside its own goal.
                if colliders.can_move_player(*player_id, delta_angle) {
                    colliders.move_player(*player_id, delta_angle);
                    transform.rotate(Quat::from_rotation_z(delta_angle));
                }
            }
        }
    }
}

fn move_ball(
    time: Res<Time>,
    mut ball_query: Query<(&mut Ball, &mut Transform)>,
    start_timer_query: Query<&StartTimer>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    let delta_tick = time.delta_seconds();
    let (ball, mut ball_transform) = ball_query.single_mut().unwrap();
    let ball_movement = util::polar_to_cartesian(ball.speed * delta_tick, ball.angle);
    ball_transform.translation += Vec3::new(ball_movement.x, ball_movement.y, 0.0);
}

fn handle_collision(
    mut ball_query: Query<(&mut Ball, &Transform)>,
    mut colliders_query: Query<&mut Colliders>,
    mut death_event_writer: EventWriter<DeathEvent>,
) {
    let (mut ball, ball_transform) = ball_query.single_mut().unwrap();
    let mut colliders = colliders_query.single_mut().unwrap();

    let ball_pos = ball_transform.translation.into();
    if let Some((_, hit_p)) = colliders.player_collision(ball_pos, BALL_RADIUS) {
        // The angle of the vector from the ball to the middle of the screen.
        let (_, reverse_angle) = util::cartesian_to_polar(-1.0 * ball_pos);

        // `hit_p` is a value between 0 & 1 depending on where the ball hit the paddle.
        // Start edge of paddle:  hit_p = 0.0  =>  extra_angle = 3PI/8 (= PI/4 + PI/8)
        // Middle of paddle:      hit_p = 0.5  =>  extra_angle = 0
        // End edge of paddle:    hit_p = 1.0  =>  extra_angle = -3PI/8
        let extra_angle = if hit_p < 0.5 {
            -(3.0 * FRAC_PI_4) * hit_p + (3.0 * FRAC_PI_8)
        } else {
            (3.0 * FRAC_PI_4) * (1.0 - hit_p) - (3.0 * FRAC_PI_8)
        };

        ball.angle = (reverse_angle + extra_angle) % TAU;
        ball.speed += BALL_SPEED_INCREMENT;
    } else if let Some(player_id) = colliders.goal_collision(ball_pos, BALL_RADIUS) {
        // The ball have hit the "goal" of the player with ID `player_id`.
        death_event_writer.send(DeathEvent(player_id));
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_reset(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    render_pipelines: Res<RenderPipelines>,
    mut ball_query: Query<(&mut Ball, &mut Transform)>,
    player_query: Query<(Entity, &PlayerId)>,
    mut players: ResMut<Players>,
    wall_query: Query<Entity, With<Wall>>,
    mut colliders_query: Query<&mut Colliders>,
    mut start_timer_query: Query<&mut StartTimer>,
    mut death_event_reader: EventReader<DeathEvent>,
) {
    // Should only reset the game if there are one or fewer players alive or if a
    // death has just occured.
    let death_event = death_event_reader.iter().next();
    if player_query.iter().count() > 1 && death_event.is_none() {
        return;
    }

    for (entity, _) in player_query.iter() {
        despawn_entity(&mut commands, entity);
    }
    for entity in wall_query.iter() {
        despawn_entity(&mut commands, entity);
    }

    let (mut ball, mut transform) = ball_query.single_mut().unwrap();
    ball.reset(&mut transform);

    let mut colliders = colliders_query.single_mut().unwrap();
    colliders.reset();

    start_timer_query.single_mut().unwrap().reset();

    let mut player_ids = if let Some(DeathEvent(player_id)) = death_event {
        let mut players_alive = player_query
            .iter()
            .map(|(_, id)| *id)
            .collect::<HashSet<_>>();
        players_alive.remove(player_id);

        if players_alive.len() == 1 {
            // Only one player alive. The player should be awarded a point and
            // the game should be reset so that every player is alive again.
            let winner_id = players_alive.iter().next().unwrap();
            if let Some(player) = players.get_mut(winner_id) {
                player.increment_score();
            }

            players.keys().cloned().collect::<Vec<_>>()
        } else {
            // A player have died. Reset the game but don't include the player
            // that just died and any players that have died before.
            players_alive.iter().cloned().collect::<Vec<_>>()
        }
    } else {
        // No death occured but there are currently only 0 or 1 player alive.
        // Reset the game fully.
        players.keys().cloned().collect::<Vec<_>>()
    };

    player_ids.shuffle(&mut rand::thread_rng());
    reset_game(
        &mut commands,
        &mut meshes,
        &render_pipelines,
        &mut colliders,
        &players,
        &player_ids,
    );
}

fn reset_votes(mut exit_event_writer: EventWriter<VoteEvent>) {
    exit_event_writer.send(VoteEvent::Reset);
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let font = fonts.regular.clone();
    let font_size = 24.0;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let exit_text = Text {
        sections: create_vote_text_sections(
            EXIT_TEXT.into(),
            &players,
            &empty_player_vote,
            required_amount,
            font,
            font_size,
        ),
        alignment: TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    };

    let exit_text_bundle = Text2dBundle {
        text: exit_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0 - font_size * 3.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(exit_text_bundle)
        .insert(ExitText)
        .insert(PongGamePlugin);
}

fn setup_colliders(mut commands: Commands) {
    commands
        .spawn()
        .insert(Colliders::new(
            GOAL_RADIUS_INNER,
            PLAYER_RADIUS_INNER,
            PLAYER_LENGTH_FRAC,
        ))
        .insert(PongGamePlugin);
}

fn setup_ball(mut commands: Commands) {
    commands
        .spawn_bundle(GeometryBuilder::build_as(
            &Shape::circle(BALL_RADIUS, Vec2::ZERO),
            ShapeColors::new(Color::rgb(1.0, 1.0, 1.0)),
            DrawMode::Fill(FillOptions::DEFAULT),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ))
        .insert(Ball::default())
        .insert(PongGamePlugin);
}

/// Sets up the whole game. Spawns the player paddle graphics, the goal graphics
/// and the colliders. Only the players specified in `player_ids` will be added
/// to the game.
fn reset_game(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    render_pipelines: &RenderPipelines,
    colliders: &mut Colliders,
    players: &Players,
    player_ids: &[PlayerId],
) {
    let amount_of_players = player_ids.len();
    let amount_of_points = AMOUNT_OF_POINTS / amount_of_players;
    let angle_per_player = util::angle_per_player(amount_of_players);

    for (idx, player_id) in player_ids.iter().enumerate() {
        let color = if let Some(player) = players.get(player_id) {
            player.color().as_bevy()
        } else {
            eprintln!(
                "Unable to find player with ID {} when creating Pong wall.",
                player_id
            );
            continue;
        };

        let angle_middle = util::angle_middle(amount_of_players, idx);
        let pos = util::polar_to_cartesian(PLAYER_RADIUS_INNER, angle_middle);
        colliders.add_player(*player_id, pos);

        /* PLAYER GRAPHIC */
        // The length of the player paddle in radians.
        let player_len_angle = angle_per_player * PLAYER_LENGTH_FRAC;
        let start_angle = util::angle_middle(amount_of_players, idx) - player_len_angle / 2.0;
        let mesh = create_circle_part(
            PLAYER_RADIUS,
            start_angle,
            player_len_angle,
            PLAYER_THICKNESS,
            amount_of_points,
            color,
        );

        commands
            .spawn_bundle(MeshBundle {
                mesh: meshes.add(mesh),
                render_pipelines: render_pipelines.clone(),
                ..Default::default()
            })
            .insert(*player_id)
            .insert(PongGamePlugin);

        /* GOAL GRAPHIC */
        let start_angle = util::angle_start(amount_of_players, idx);
        let mesh = create_circle_part(
            GOAL_RADIUS,
            start_angle,
            angle_per_player,
            GOAL_THICKNESS,
            amount_of_points,
            color,
        );

        commands
            .spawn_bundle(MeshBundle {
                mesh: meshes.add(mesh),
                render_pipelines: render_pipelines.clone(),
                ..Default::default()
            })
            .insert(Wall)
            .insert(PongGamePlugin);
    }
}

/// If a `fade_color` is given, the circle will have the color `color` at the
/// outer part of the cirlce and the color `fade_color` at the inner part with
/// a fade between.
fn create_circle_part(
    middle_radius: f32,
    start_angle: f32,
    angle_amount: f32,
    thickness: f32,
    amount_of_points: usize,
    color: Color,
) -> Mesh {
    let center = Vec2::ZERO;
    let points = create_circle_points(
        middle_radius,
        center,
        start_angle,
        angle_amount,
        amount_of_points,
    );

    let vertices = vertices_with_thickness(
        &points
            .iter()
            .map(|p| Vec2::new(p.x, p.y))
            .collect::<Vec<_>>(),
        thickness,
        false,
    );
    let indices = indices_from_vertices(&vertices);
    let colors = vec![[color.r(), color.g(), color.b(), color.a()]; vertices.len()];

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
    mesh.set_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vertices.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>(),
    );
    mesh.set_indices(Some(Indices::U32(indices)));
    mesh.set_attribute(Mesh::ATTRIBUTE_COLOR, colors);

    mesh
}
