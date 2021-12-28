use std::{
    cmp::Reverse,
    f32::consts::PI,
    ops::{Deref, DerefMut},
};

use bevy::{
    core::{Time, Timer},
    math::Vec2,
    prelude::{
        AppBuilder, Assets, BuildChildren, Children, Color, Commands, Entity, EventReader,
        EventWriter, HorizontalAlign, IntoSystem, Local, Mesh, MeshBundle, Or,
        ParallelSystemDescriptorCoercion, Plugin, Query, RenderPipelines, Res, ResMut, State,
        SystemSet, Transform, VerticalAlign, With,
    },
    text::{Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    na::UnitComplex,
    physics::{ColliderBundle, ColliderPositionSync, IntoEntity, RigidBodyBundle},
    prelude::{
        ActiveEvents, ColliderMassProps, ColliderMaterial, ColliderShape, ColliderType,
        IntersectionEvent, Isometry, RigidBodyActivation, RigidBodyDamping, RigidBodyMassProps,
        RigidBodyPosition, RigidBodyType, RigidBodyVelocity,
    },
};
use rand::{prelude::SliceRandom, Rng};

use skitspel::{
    ActionEvent, DisconnectedPlayers, GameState, Player, PlayerId, Players, GAME_HEIGHT,
    GAME_WIDTH, MAX_PLAYERS, RAPIER_SCALE_FACTOR,
};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, AsBevyColor, Fonts, PlayerVote,
    Shape, VoteEvent,
};
use util_rapier::{create_path_with_thickness, create_polygon_points, spawn_border_walls};

// TODO: Implement Animation for explotion.

const GAME_STATE: GameState = GameState::AchtungGame;

const SPAWN_POSITIONS: [(f32, f32); MAX_PLAYERS] = [
    (-GAME_WIDTH * 0.375, GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.375, -GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.125, -GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.125, GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.25, 0.0),
    (GAME_WIDTH * 0.375, GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.375, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.125, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.125, GAME_HEIGHT * 0.25),
];

const EXPLOTION_TEXT: &str = "Press A to create explotion (5 sec cooldown)\n";
const EXIT_TEXT: &str = "Press B to go back to main menu";

const ACHTUNG_PLAYER_HEIGHT: f32 = 20.0;

const ACHTUNG_CONSTANT_SPEED: f32 = 200.0;
const ACHTUNG_CONSTANT_TORQUE: f32 = 100.0;

/// The height and width of the explotion cooldown UI over the players.
const EXPLOTION_COOLDOWN_WIDTH: f32 = ACHTUNG_PLAYER_HEIGHT * 1.5;
const EXPLOTION_COOLDOWN_HEIGHT: f32 = 10.0;

/// Tag used on the exit text.
struct ExitText;

/// Tag used on the player tails.
struct Tail;

/// Tag used on the walls.
#[derive(Clone)]
struct Wall;

/// Component used to tag the text containing the score.
struct ScoreText;

/// Event created when player with ID `PlayerId` dashes.
struct ExplotionEvent(PlayerId);

/// Tag used on the UI under players that displays the cooldown of the dash.
struct ExplotionCooldownUI;

/// Timer used to restrict how often a player can create an explotion. This timer
/// will be restarted when a explotion is done and a player isn't allowed to
/// explode again until this timer runs out.
struct ExplotionTimer(Timer);

impl Deref for ExplotionTimer {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ExplotionTimer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for ExplotionTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(5.0, false))
    }
}

/// A struct used to keep state related to spawning of player tails.
///
/// The timer is used to not spawn them to often and the `prev_x` & `prev_y`
/// coordinates are the coordinates where the player was located the previous
/// time the timer finished. These coordinates will be used as the start
/// position for the new tail to draw.
///
/// The x & y coordinateas should NOT be scaled with the `RAPIER_SCALE_FACTOR`.
struct TailSpawn {
    timer: Timer,
    prev_x: f32,
    prev_y: f32,
}

impl TailSpawn {
    fn new(x: f32, y: f32) -> Self {
        Self {
            timer: Timer::from_seconds(1.0 / 10.0, true),
            prev_x: x,
            prev_y: y,
        }
    }
}

impl Deref for TailSpawn {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.timer
    }
}

impl DerefMut for TailSpawn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.timer
    }
}

#[derive(Clone)]
pub struct AchtungGamePlugin;

impl Plugin for AchtungGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<ExplotionEvent>()
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_map.system())
                    .with_system(setup_screen_text.system()),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_disconnect.system().label("vote"))
                    .with_system(handle_player_input.system().label("vote").label("explode"))
                    .with_system(handle_exit_event.system().after("vote"))
                    .with_system(handle_winner.system())
                    .with_system(update_scoreboard.system())
                    .with_system(move_achtung_players.system())
                    .with_system(handle_spawn_tails.system().label("tail"))
                    .with_system(handle_death.system().label("death"))
                    .with_system(reset_players.system().after("tail").after("death"))
                    .with_system(
                        update_explotion_timers
                            .system()
                            .after("explode")
                            .label("timer"),
                    )
                    .with_system(update_explotion_ui.system().after("timer").before("death"))
                    .with_system(
                        handle_player_explotion
                            .system()
                            .after("explode")
                            .after("timer"),
                    ),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE)
                    .with_system(despawn_system::<AchtungGamePlugin>.system()),
            );
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

fn handle_winner(mut players: ResMut<Players>, players_alive_query: Query<&PlayerId>) {
    if players_alive_query.iter().len() == 1 {
        let player_id = players_alive_query.iter().next().unwrap();
        if let Some(winning_player) = players.get_mut(player_id) {
            winning_player.increment_score();
        }
    }
}

fn reset_players(
    mut commands: Commands,
    mut players: ResMut<Players>,
    players_alive_query: Query<Entity, With<PlayerId>>,
    tail_query: Query<Entity, With<Tail>>,
) {
    if players_alive_query.iter().count() <= 1 {
        // Remove any players that are still alive and all all tails.
        for entity in players_alive_query.iter().chain(tail_query.iter()) {
            despawn_entity(&mut commands, entity);
        }

        let mut spawn_positions = SPAWN_POSITIONS;
        spawn_positions.shuffle(&mut rand::thread_rng());

        for (idx, player) in players.values_mut().enumerate() {
            // Reset action of player before respawning. This can prevent scenarios
            // where a player held down the button just before the round ended
            // and the player continues to go in the same direction when respawned.
            player.reset_action();

            let rotation = rand::thread_rng().gen_range(0.0..(2.0 * PI));
            spawn_achtung_player(&mut commands, player, spawn_positions[idx].into(), rotation);
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

        let top_margin = 15.0;
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
            .insert(AchtungGamePlugin)
            .insert(ScoreText);
    }
}

fn update_explotion_timers(time: Res<Time>, mut explotion_timer_query: Query<&mut ExplotionTimer>) {
    for mut timer in explotion_timer_query.iter_mut() {
        timer.tick(time.delta());
    }
}

// TODO: Implement.
/// Creates an explotion around the player destroying all tails in a radius
/// around it.
fn handle_player_explotion(
    players: Res<Players>,
    mut player_query: Query<(
        &PlayerId,
        &mut ExplotionTimer,
        &mut RigidBodyVelocity,
        &RigidBodyMassProps,
    )>,
    mut explotion_event_reader: EventReader<ExplotionEvent>,
) {
    for ExplotionEvent(event_player_id) in explotion_event_reader.iter() {
        // TODO: More performant way to get player_query from player_id?
        for (player_id, mut timer, mut velocity, mass) in player_query.iter_mut() {
            if event_player_id == player_id && timer.finished() {
                if let Some(player) = players.get(player_id) {
                    // TODO:
                    timer.reset();
                }
            }
        }
    }
}

fn handle_spawn_tails(
    mut commands: Commands,
    render_pipelines: Res<RenderPipelines>,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut TailSpawn, &RigidBodyPosition)>,
) {
    for (player_id, mut tail_spawn, pos) in player_query.iter_mut() {
        if tail_spawn.timer.tick(time.delta()).just_finished() {
            let player_angle = pos.position.rotation.angle() + std::f32::consts::FRAC_PI_2;
            let player_heading_vec = Vec2::new(player_angle.cos(), player_angle.sin());
            let player_backside = player_heading_vec * -ACHTUNG_PLAYER_HEIGHT;

            let cur_x = pos.position.translation.x * RAPIER_SCALE_FACTOR + player_backside.x;
            let cur_y = pos.position.translation.y * RAPIER_SCALE_FACTOR + player_backside.y;
            let prev_x = tail_spawn.prev_x;
            let prev_y = tail_spawn.prev_y;

            if let Some(player) = players.get(player_id) {
                let (mesh, colliders) = create_path_with_thickness(
                    &[Vec2::new(cur_x, cur_y), Vec2::new(prev_x, prev_y)],
                    player.color().as_bevy(),
                    ACHTUNG_PLAYER_HEIGHT / 2.0,
                    ColliderType::Sensor,
                    ActiveEvents::INTERSECTION_EVENTS,
                    false,
                );

                let mut entity_commands = commands.spawn_bundle(MeshBundle {
                    mesh: meshes.add(mesh),
                    render_pipelines: render_pipelines.clone(),
                    ..Default::default()
                });

                colliders.into_iter().for_each(|collider| {
                    entity_commands.with_children(|parent| {
                        parent.spawn_bundle(collider).insert(Tail);
                    });
                });

                entity_commands.insert(Tail).insert(AchtungGamePlugin);
            }

            tail_spawn.prev_x = cur_x;
            tail_spawn.prev_y = cur_y;
        }
    }
}

fn update_explotion_ui(
    mut commands: Commands,
    mut player_query: Query<(Entity, &mut ExplotionTimer, &Children)>,
    cooldown_ui_query: Query<(), With<ExplotionCooldownUI>>,
) {
    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let green_color = Color::rgb(0.1, 1.0, 0.1);

    for (entity, timer, children) in player_query.iter_mut() {
        if timer.just_finished() {
            // The timer just finished this tick. Create a single "finished"
            // bar in green.
            for child_entity in children.iter() {
                if cooldown_ui_query.get(*child_entity).is_ok() {
                    despawn_entity(&mut commands, *child_entity);
                }
            }

            let shape_bundle_finished = GeometryBuilder::build_as(
                &Shape::rectangle(
                    EXPLOTION_COOLDOWN_WIDTH,
                    EXPLOTION_COOLDOWN_HEIGHT,
                    Vec2::ZERO,
                ),
                ShapeColors::new(green_color),
                DrawMode::Fill(FillOptions::DEFAULT),
                Transform::from_xyz(0.0, ACHTUNG_PLAYER_HEIGHT, 0.0),
            );

            let new_child = commands
                .spawn_bundle(shape_bundle_finished)
                .insert(ExplotionCooldownUI)
                .id();

            commands.entity(entity).push_children(&[new_child]);
        } else if timer.finished() {
            // The timer was already finished before this tick which means that
            // the graphic should already be correct, do nothing.
        } else {
            // The timer is currently counting down, update the UI according
            // to the current timer.
            for child_entity in children.iter() {
                if cooldown_ui_query.get(*child_entity).is_ok() {
                    despawn_entity(&mut commands, *child_entity);
                }
            }

            let shape_bundle_left = GeometryBuilder::build_as(
                &Shape::rectangle(
                    EXPLOTION_COOLDOWN_WIDTH * timer.percent(),
                    EXPLOTION_COOLDOWN_HEIGHT,
                    Vec2::ZERO,
                ),
                ShapeColors::new(red_color),
                DrawMode::Fill(FillOptions::DEFAULT),
                Transform::from_xyz(0.0, ACHTUNG_PLAYER_HEIGHT, 0.0),
            );

            let new_children = commands
                .spawn_bundle(shape_bundle_left)
                .insert(ExplotionCooldownUI)
                .id();

            commands.entity(entity).push_children(&[new_children]);
        }
    }
}

/// Checks collisions between players and either walls or tails. Removes the
/// player entity if a collision is found.
#[allow(clippy::type_complexity)]
fn handle_death(
    mut commands: Commands,
    mut intersection_event: EventReader<IntersectionEvent>,
    players_query: Query<Entity, With<PlayerId>>,
    death_query: Query<Entity, Or<(With<Wall>, With<Tail>)>>,
) {
    for intersection in intersection_event.iter() {
        if intersection.intersecting {
            let entity_a = intersection.collider1.entity();
            let entity_b = intersection.collider2.entity();
            if players_query.get(entity_a).is_ok() && death_query.get(entity_b).is_ok() {
                despawn_entity(&mut commands, entity_a);
            } else if players_query.get(entity_b).is_ok() && death_query.get(entity_a).is_ok() {
                despawn_entity(&mut commands, entity_b);
            }
        }
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    mut dash_event_writer: EventWriter<ExplotionEvent>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                match prev_action {
                    ActionEvent::APressed => {
                        dash_event_writer.send(ExplotionEvent(player.id()));
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

pub fn move_achtung_players(
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut RigidBodyVelocity, &RigidBodyPosition)>,
) {
    let delta_tick = time.delta_seconds();
    for (player_id, mut velocity, pos) in player_query.iter_mut() {
        if let Some(player) = players.get(player_id) {
            let movement_x = player.movement_x();
            if movement_x > 0.0 {
                velocity.angvel = -ACHTUNG_CONSTANT_TORQUE * delta_tick;
            } else if movement_x < 0.0 {
                velocity.angvel = ACHTUNG_CONSTANT_TORQUE * delta_tick;
            } else {
                velocity.angvel = 0.0;
            }

            let player_angle = pos.position.rotation.angle() + std::f32::consts::FRAC_PI_2;
            let player_heading_vec = Vec2::new(player_angle.cos(), player_angle.sin());

            velocity.linvel = (player_heading_vec * ACHTUNG_CONSTANT_SPEED * delta_tick).into();
        }
    }
}

fn reset_votes(mut exit_event_writer: EventWriter<VoteEvent>) {
    exit_event_writer.send(VoteEvent::Reset);
}

fn spawn_achtung_player(commands: &mut Commands, player: &Player, mut pos: Vec2, rotation: f32) {
    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let vertex_amount = 3;
    let center = Vec2::ZERO;
    let mut height = ACHTUNG_PLAYER_HEIGHT;

    let player_angle = rotation + std::f32::consts::FRAC_PI_2;
    let player_heading_vec = Vec2::new(player_angle.cos(), player_angle.sin());
    let player_backside_relative = player_heading_vec * -height;
    let player_backside = pos + player_backside_relative;

    let tail_spawn = TailSpawn::new(player_backside.x, player_backside.y);

    let head_shape_bundle = GeometryBuilder::build_as(
        &Shape::new(height, center, vertex_amount),
        ShapeColors::new(player.color().as_bevy()),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 0.0),
    );

    let bottom_shape_bundle = GeometryBuilder::build_as(
        &Shape::circle(height / 2.0, center),
        ShapeColors::new(player.color().as_bevy()),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(0.0, -height / 2.0, 0.0),
    );

    let cooldown_bundle = GeometryBuilder::build_as(
        &Shape::rectangle(
            EXPLOTION_COOLDOWN_WIDTH,
            EXPLOTION_COOLDOWN_HEIGHT,
            Vec2::ZERO,
        ),
        ShapeColors::new(red_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(0.0, height, 0.0),
    );

    pos /= RAPIER_SCALE_FACTOR;
    height /= RAPIER_SCALE_FACTOR;

    let rigid_body = RigidBodyBundle {
        body_type: RigidBodyType::Dynamic,
        damping: RigidBodyDamping {
            linear_damping: 0.5,
            angular_damping: 0.5,
        },
        position: RigidBodyPosition {
            position: Isometry {
                rotation: UnitComplex::new(rotation),
                translation: pos.into(),
            },
            ..Default::default()
        },
        activation: RigidBodyActivation::cannot_sleep(),
        ..Default::default()
    };

    let collider_shape =
        ColliderShape::convex_hull(&create_polygon_points(vertex_amount, height, center))
            .unwrap_or_else(|| {
                panic!("Unable to create convex_hull with sides: {}", vertex_amount)
            });

    let collider = ColliderBundle {
        collider_type: ColliderType::Sensor,
        shape: collider_shape,
        flags: ActiveEvents::INTERSECTION_EVENTS.into(),
        material: ColliderMaterial {
            friction: 0.7,
            restitution: 0.8,
            ..Default::default()
        },
        mass_properties: ColliderMassProps::Density(1.0),
        ..Default::default()
    };

    let mut entity_commands = commands.spawn_bundle(rigid_body);
    entity_commands
        .insert_bundle(head_shape_bundle)
        .insert_bundle(collider)
        .insert(player.id())
        .insert(tail_spawn)
        .insert(ExplotionTimer::default())
        .insert(AchtungGamePlugin)
        .insert(ColliderPositionSync::Discrete);

    entity_commands
        .with_children(|parent| {
            parent
                .spawn_bundle(cooldown_bundle)
                .insert(ExplotionCooldownUI);
        })
        .with_children(|parent| {
            parent.spawn_bundle(bottom_shape_bundle);
        });
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let font = fonts.regular.clone();
    let font_size = 24.0;
    let font_color = Color::WHITE;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let explotion_text = Text::with_section(
        EXPLOTION_TEXT,
        TextStyle {
            font: font.clone(),
            font_size,
            color: font_color,
        },
        TextAlignment {
            vertical: VerticalAlign::Bottom,
            horizontal: HorizontalAlign::Center,
        },
    );

    let explotion_text_bundle = Text2dBundle {
        text: explotion_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(explotion_text_bundle)
        .insert(AchtungGamePlugin);

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
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0 - font_size, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(exit_text_bundle)
        .insert(ExitText)
        .insert(AchtungGamePlugin);
}

fn setup_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    render_pipelines: Res<RenderPipelines>,
) {
    let red_color = Color::rgb(1.0, 0.1, 0.1);
    spawn_border_walls(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        red_color,
        10.0,
        ColliderType::Sensor,
        Some(Wall),
    )
    .insert(AchtungGamePlugin);
}
