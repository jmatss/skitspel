use std::{
    cmp::Reverse,
    collections::HashSet,
    f32::consts::{PI, TAU},
    ops::{Deref, DerefMut},
    time::Duration,
};

use bevy::{
    core::{Time, Timer},
    math::{Quat, Vec2, Vec3},
    prelude::{
        AppBuilder, Assets, BuildChildren, Children, Color, Commands, CoreStage, Entity,
        EventReader, EventWriter, GlobalTransform, HorizontalAlign, IntoSystem, Local, Mesh,
        MeshBundle, ParallelSystemDescriptorCoercion, Plugin, Query, RenderPipelines, Res, ResMut,
        State, SystemSet, SystemStage, Transform, VerticalAlign, With,
    },
    render::{mesh::Indices, pipeline::PrimitiveTopology},
    text::{Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    na::UnitComplex,
    physics::{ColliderBundle, ColliderPositionSync, RigidBodyBundle},
    prelude::{
        ColliderFlags, ColliderShape, ColliderType, InteractionGroups, Isometry,
        RigidBodyActivation, RigidBodyDamping, RigidBodyPosition, RigidBodyType, RigidBodyVelocity,
    },
};
use rand::{prelude::SliceRandom, Rng};

use colliders::Colliders;
use skitspel::{
    ActionEvent, DisconnectedPlayers, GameState, Player, PlayerId, Players, GAME_HEIGHT,
    GAME_WIDTH, RAPIER_SCALE_FACTOR,
};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, handle_start_timer,
    setup_start_timer, AsBevyColor, Fonts, PlayerVote, Shape, StartEntity, StartTimer, VoteEvent,
};
use util_rapier::{
    create_circle_points, indices_from_vertices, spawn_border_walls, vertices_with_thickness,
};

mod colliders;

const GAME_STATE: GameState = GameState::AchtungGame;

const SPAWN_POSITIONS: [(f32, f32); 12] = [
    (-GAME_WIDTH * 0.3, GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.3, 0.0),
    (-GAME_WIDTH * 0.3, -GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.1, GAME_HEIGHT * 0.25),
    (-GAME_WIDTH * 0.1, 0.0),
    (-GAME_WIDTH * 0.1, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.1, GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.1, 0.0),
    (GAME_WIDTH * 0.1, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.3, GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.3, 0.0),
    (GAME_WIDTH * 0.3, -GAME_HEIGHT * 0.25),
];

const JUMP_TEXT: &str = "Press A to jump\n";
const EXIT_TEXT: &str = "Press B to go back to main menu";

/// The thickness/size of the player character.
const PLAYER_THICKNESS: f32 = 20.0;

/// The thickness of the tails.
const TAIL_THICKNESS: f32 = 10.0;

const ACHTUNG_CONSTANT_SPEED: f32 = 400.0;
const ACHTUNG_CONSTANT_TORQUE: f32 = 150.0;

/// The height and width of the cooldown UI under the players.
const JUMP_COOLDOWN_WIDTH: f32 = PLAYER_THICKNESS * 1.5;
const JUMP_COOLDOWN_HEIGHT: f32 = 7.0;

/// How long time a jump takes (seconds).
const JUMP_TIME: f32 = 0.75;

/// The time it takes for the jump cooldown to reset.
const JUMP_COOLDOWN_TIME: f32 = 8.0 + JUMP_TIME;

/// How often a tail is spawned (seconds).
const TAIL_SPAWN_TIME: f32 = 1.0 / 5.0;

/// How long the timer between rounds are in seconds.
const START_TIMER_TIME: usize = 3;

/// Tag used on the exit text.
struct ExitText;

/// Tag used on the player tails.
struct Tail;

/// Tag used on the tail part that haven't been "created" yet, it is located
/// between the back of the player and the latest tail point.
struct CurrentTail;

/// Tag used on the walls.
#[derive(Clone)]
struct Wall;

/// Component used to tag the text containing the score.
struct ScoreText;

/// Event created when player with ID `PlayerId` jumps.
struct JumpEvent(PlayerId);

/// Event created when a player dies and its character is removed. This is used
/// to synchronize the removal of the player character and the cooldown UI for
/// jump.
struct DeathEvent(PlayerId);

/// Tag used on the UI under players that displays the cooldown.
struct CooldownUI;

/// Timer used to restrict how often a player can jump. This timer will be
/// restarted when a jump is done and a player isn't allowed to jump again until
/// this timer runs out.
struct JumpCooldownTimer(Timer);

impl Deref for JumpCooldownTimer {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for JumpCooldownTimer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for JumpCooldownTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(JUMP_COOLDOWN_TIME, false))
    }
}

/// Timer used when the player is currently jumping. When this timer finished,
/// the jump will finish for the player.
struct JumpTimer(Timer);

impl Deref for JumpTimer {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for JumpTimer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for JumpTimer {
    fn default() -> Self {
        // The timer should start as finished.
        let mut timer = Timer::from_seconds(JUMP_TIME, false);
        timer.tick(Duration::from_secs_f32(JUMP_TIME));
        Self(timer)
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
            timer: Timer::from_seconds(TAIL_SPAWN_TIME, true),
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

#[derive(Clone, Default)]
pub struct AchtungGamePlugin;

impl Plugin for AchtungGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<JumpEvent>()
            .add_event::<DeathEvent>()
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_colliders.system())
                    .with_system(setup_start_timer::<AchtungGamePlugin, START_TIMER_TIME>.system())
                    .with_system(setup_map.system())
                    .with_system(setup_screen_text.system()),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_disconnect.system().label("vote"))
                    .with_system(handle_player_input.system().label("vote").label("jump"))
                    .with_system(handle_exit_event.system().after("vote"))
                    .with_system(handle_winner.system())
                    .with_system(update_scoreboard.system())
                    .with_system(move_achtung_players.system())
                    .with_system(handle_death.system().label("death"))
                    .with_system(reset_game.system().label("reset").after("death"))
                    .with_system(handle_start_timer.system().label("start").after("reset"))
                    .with_system(
                        update_jump_timers
                            .system()
                            .label("timer")
                            .after("jump")
                            .after("reset"),
                    )
                    .with_system(handle_player_jump.system().after("timer").before("jump_ui"))
                    .with_system(update_jump_ui.system().label("jump_ui"))
                    .with_system(
                        handle_tails
                            .system()
                            .label("tail")
                            .after("start")
                            .after("timer")
                            .before("current_tail"),
                    )
                    .with_system(handle_current_tails.system().label("current_tail")),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE)
                    .with_system(despawn_system::<AchtungGamePlugin>.system()),
            )
            .add_stage_after(
                CoreStage::PostUpdate,
                "cooldown_ui_rotation",
                SystemStage::single_threaded(),
            )
            .add_system_to_stage(
                "cooldown_ui_rotation",
                update_cooldown_ui_transform.system(),
            );
    }
}

// There is currently no good way to handle rotation of a child entity relative
// to its parent. This is a hack to make it work (see also "cooldown_ui_rotation" stage).
// See: https://github.com/bevyengine/bevy/issues/1780#issuecomment-939385391
fn update_cooldown_ui_transform(
    player_query: Query<&Children, With<JumpCooldownTimer>>,
    mut cooldown_ui_query: Query<&mut GlobalTransform, With<CooldownUI>>,
) {
    for children in player_query.iter() {
        for child_entity in children.iter() {
            if let Ok(mut transform) = cooldown_ui_query.get_mut(*child_entity) {
                transform.rotation = Quat::from_rotation_y(0.0);
                transform.translation.y =
                    transform.translation.y - PLAYER_THICKNESS - JUMP_COOLDOWN_HEIGHT;
            }
        }
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

fn reset_game(
    mut commands: Commands,
    mut players: ResMut<Players>,
    players_alive_query: Query<Entity, With<PlayerId>>,
    tail_query: Query<Entity, With<Tail>>,
    mut colliders: Query<&mut Colliders>,
    mut start_timer_query: Query<&mut StartTimer>,
) {
    if players_alive_query.iter().count() <= 1 {
        // Remove any players that are still alive and all tails.
        for entity in players_alive_query.iter().chain(tail_query.iter()) {
            despawn_entity(&mut commands, entity);
        }

        colliders.single_mut().unwrap().reset();

        let mut spawn_positions = SPAWN_POSITIONS;
        spawn_positions.shuffle(&mut rand::thread_rng());

        for (idx, player) in players.values_mut().enumerate() {
            // Reset action of player before respawning. This can prevent scenarios
            // where a player held down the button just before the round ended
            // and the player continues to go in the same direction when respawned.
            player.reset_action();

            let rotation = rand::thread_rng().gen_range(0.0..TAU);
            spawn_achtung_player(&mut commands, player, spawn_positions[idx].into(), rotation);
        }

        start_timer_query.single_mut().unwrap().reset();
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

fn update_jump_timers(
    time: Res<Time>,
    mut player_timer_query: Query<(&mut JumpTimer, &mut JumpCooldownTimer), With<PlayerId>>,
    start_timer_query: Query<&StartTimer>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    for (mut jump_timer, mut cooldown_timer) in player_timer_query.iter_mut() {
        cooldown_timer.tick(time.delta());
        jump_timer.tick(time.delta());
    }
}

/// Handles jump events and sets the jumping players into a "jump-state", reseting
/// both the jump timer and jump cooldown timer.
///
/// Ensures that the graphical representation of the player is changed when a
/// jump is performed so that the user clearly sees when a jump is in progress.
/// Also turns of the rapier collisions between snakes when the snake is in
/// the air.
fn handle_player_jump(
    mut player_query: Query<(
        &PlayerId,
        &mut JumpTimer,
        &mut JumpCooldownTimer,
        &mut Transform,
        &mut ColliderFlags,
    )>,
    mut jump_event_reader: EventReader<JumpEvent>,
) {
    for JumpEvent(event_player_id) in jump_event_reader.iter() {
        // TODO: More performant way to get player_query from player_id?
        for (player_id, mut jump_timer, mut cooldown_timer, ..) in player_query.iter_mut() {
            if event_player_id == player_id && cooldown_timer.finished() {
                jump_timer.reset();
                cooldown_timer.reset();
            }
        }
    }

    for (_, jump_timer, _, mut transform, mut flags) in player_query.iter_mut() {
        if jump_timer.just_finished() {
            // The jump just finished, reset "jumping" settings.
            transform.scale = Vec3::new(1.0, 1.0, 1.0);
            transform.translation.z = 0.0;

            *flags = ColliderFlags::default();
        } else if !jump_timer.finished() {
            // A jump is in progress. Scale the size of the player to make it
            // look like it is jumping up in the z-axis (outwards from the screen)
            // and disable its collision with other snakes.

            // A value between 0 & 1. It will peek at 1 in the exact middle of
            // the jump and be 0 at the start and end of the jump.
            let percent = if jump_timer.percent() < 0.5 {
                jump_timer.percent() * 2.0
            } else {
                jump_timer.percent_left() * 2.0
            };
            // Logarithim equation which gives ~:
            // (percent == 0)  =>  ~0.0
            // (percent == 1)  =>  ~0.5
            let alg = 0.1 * (percent + 0.01).ln() + 0.5;
            let scale = 1.0 + alg;
            transform.scale = Vec3::new(scale, scale, scale);
            transform.translation.z = 100.0;

            *flags = ColliderFlags {
                collision_groups: InteractionGroups::none(),
                ..Default::default()
            };
        }
    }
}

/// Spawns a tail with a corresponding collider everytime the `TailSpawn` timer
/// have finished.
#[allow(clippy::too_many_arguments)]
fn handle_tails(
    mut commands: Commands,
    render_pipelines: Res<RenderPipelines>,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut TailSpawn, &RigidBodyPosition, &JumpTimer)>,
    mut colliders: Query<&mut Colliders>,
    start_timer_query: Query<&StartTimer>,
    mut death_event_reader: EventReader<DeathEvent>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    let players_just_died = death_event_reader
        .iter()
        .map(|e| e.0)
        .collect::<HashSet<_>>();

    for (player_id, mut tail_spawn, pos, jump_timer) in player_query.iter_mut() {
        if jump_timer.just_finished() {
            // We just finished a jump, need to update the place where the
            // tails should start spawning. This should be changed to the
            // location where we landed instead of where we jumped from.
            tail_spawn.prev_x = pos.position.translation.x * RAPIER_SCALE_FACTOR;
            tail_spawn.prev_y = pos.position.translation.y * RAPIER_SCALE_FACTOR;
        }

        // Don't spawn a tail if we are currently jumping or if the player died
        // this tick.
        if tail_spawn.timer.tick(time.delta()).just_finished()
            && jump_timer.finished()
            && !players_just_died.contains(player_id)
        {
            if let Some(player) = players.get(player_id) {
                let vertices = current_tail_vertices(&tail_spawn, pos);
                colliders.single_mut().unwrap().add(&vertices);

                let mesh = current_tail_mesh(&tail_spawn, pos, player.color().as_bevy());
                commands
                    .spawn_bundle(MeshBundle {
                        mesh: meshes.add(mesh),
                        render_pipelines: render_pipelines.clone(),
                        ..Default::default()
                    })
                    .insert(Tail)
                    .insert(AchtungGamePlugin);

                tail_spawn.prev_x = pos.position.translation.x * RAPIER_SCALE_FACTOR;
                tail_spawn.prev_y = pos.position.translation.y * RAPIER_SCALE_FACTOR;
            }
        }
    }
}

/// Handles the tail that is attached to the back of the player and haven't
/// been "spawned" yet. It doesn't have collider.
fn handle_current_tails(
    mut commands: Commands,
    render_pipelines: Res<RenderPipelines>,
    mut meshes: ResMut<Assets<Mesh>>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &TailSpawn, &RigidBodyPosition, &JumpTimer)>,
    cur_tail_query: Query<Entity, With<CurrentTail>>,
    start_timer_query: Query<&StartTimer>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    for entity in cur_tail_query.iter() {
        despawn_entity(&mut commands, entity);
    }

    for (player_id, tail_spawn, pos, jump_timer) in player_query.iter_mut() {
        // Don't spawn a tail if we are currently jumping.
        if !jump_timer.finished() {
            continue;
        }

        if let Some(player) = players.get(player_id) {
            let mesh = current_tail_mesh(tail_spawn, pos, player.color().as_bevy());
            commands
                .spawn_bundle(MeshBundle {
                    mesh: meshes.add(mesh),
                    render_pipelines: render_pipelines.clone(),
                    ..Default::default()
                })
                .insert(CurrentTail)
                .insert(Tail)
                .insert(AchtungGamePlugin);
        }
    }
}

fn current_tail_vertices(tail_spawn: &TailSpawn, pos: &RigidBodyPosition) -> Vec<Vec2> {
    let cur_x = pos.position.translation.x * RAPIER_SCALE_FACTOR;
    let cur_y = pos.position.translation.y * RAPIER_SCALE_FACTOR;
    let prev_x = tail_spawn.prev_x;
    let prev_y = tail_spawn.prev_y;
    vertices_with_thickness(
        &[Vec2::new(cur_x, cur_y), Vec2::new(prev_x, prev_y)],
        TAIL_THICKNESS,
        false,
    )
}

fn current_tail_mesh(tail_spawn: &TailSpawn, pos: &RigidBodyPosition, color: Color) -> Mesh {
    let new_vertices = current_tail_vertices(tail_spawn, pos);
    let indices = indices_from_vertices(&new_vertices);
    let colors = vec![[color.r(), color.g(), color.b(), color.a()]; new_vertices.len()];

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
    mesh.set_attribute(
        Mesh::ATTRIBUTE_POSITION,
        new_vertices.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>(),
    );
    mesh.set_indices(Some(Indices::U32(indices)));
    mesh.set_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn update_jump_ui(
    mut commands: Commands,
    mut player_query: Query<(Entity, &PlayerId, &JumpTimer, &JumpCooldownTimer, &Children)>,
    cooldown_ui_query: Query<(), With<CooldownUI>>,
    start_timer_query: Query<&StartTimer>,
    mut death_event_reader: EventReader<DeathEvent>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    let players_just_died = death_event_reader
        .iter()
        .map(|e| e.0)
        .collect::<HashSet<_>>();

    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let green_color = Color::rgb(0.1, 1.0, 0.1);

    for (entity, player_id, jump_timer, cooldown_timer, children) in player_query.iter_mut() {
        // Don't want to touch the entities of players that have just died (can
        // lead to panics).
        if players_just_died.contains(player_id) {
            continue;
        }

        if cooldown_timer.just_finished() {
            // The timer just finished this tick. Create a single "finished"
            // bar in green.
            for child_entity in children.iter() {
                if cooldown_ui_query.get(*child_entity).is_ok() {
                    despawn_entity(&mut commands, *child_entity);
                }
            }

            let shape_bundle_finished = GeometryBuilder::build_as(
                &Shape::rectangle(JUMP_COOLDOWN_WIDTH, JUMP_COOLDOWN_HEIGHT, Vec2::ZERO),
                ShapeColors::new(green_color),
                DrawMode::Fill(FillOptions::DEFAULT),
                Transform::from_xyz(0.0, 0.0, 0.0),
            );

            let new_child = commands
                .spawn_bundle(shape_bundle_finished)
                .insert(CooldownUI)
                .id();

            commands.entity(entity).push_children(&[new_child]);
        } else if cooldown_timer.finished() {
            // The timers was already finished before this tick which means that
            // the graphic should already be correct, do nothing.
        } else if jump_timer.elapsed_secs() == 0.0 {
            // The player just jumped. Remove the cooldown UI. It will be
            // re-drawn when the jump finishes.
            for child_entity in children.iter() {
                if cooldown_ui_query.get(*child_entity).is_ok() {
                    despawn_entity(&mut commands, *child_entity);
                }
            }
        } else if jump_timer.finished() {
            // No jump is currently in progress and the cooldown timer is currently
            // counting down, update the UI according to the current cooldown.
            for child_entity in children.iter() {
                if cooldown_ui_query.get(*child_entity).is_ok() {
                    despawn_entity(&mut commands, *child_entity);
                }
            }

            let shape_bundle_left = GeometryBuilder::build_as(
                &Shape::rectangle(
                    JUMP_COOLDOWN_WIDTH * cooldown_timer.percent(),
                    JUMP_COOLDOWN_HEIGHT,
                    Vec2::ZERO,
                ),
                ShapeColors::new(red_color),
                DrawMode::Fill(FillOptions::DEFAULT),
                Transform::from_xyz(0.0, 0.0, 0.0),
            );

            let new_children = commands
                .spawn_bundle(shape_bundle_left)
                .insert(CooldownUI)
                .id();

            commands.entity(entity).push_children(&[new_children]);
        }
    }
}

/// Checks collisions between players and either walls or tails. Removes the
/// player entity if a collision is found.
///
/// The player is expected to be a circle. Only points from the "front" half
/// of the circle will be used to check collision.
fn handle_death(
    mut commands: Commands,
    players_query: Query<(Entity, &PlayerId, &JumpTimer, &RigidBodyPosition)>,
    colliders: Query<&Colliders>,
    start_timer_query: Query<&StartTimer>,
    mut death_event_writer: EventWriter<DeathEvent>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    let colliders = colliders.single().unwrap();
    for (entity, player_id, jump_timer, pos) in players_query.iter() {
        let x_pos = pos.position.translation.x * RAPIER_SCALE_FACTOR;
        let y_pos = pos.position.translation.y * RAPIER_SCALE_FACTOR;
        let player_angle = pos.position.rotation.angle() + std::f32::consts::FRAC_PI_2;

        // Take `amount_of_points` points from the "front" of the player circle
        // and check collisions with those points. This will allows us to spawn
        // the tail in the middle of the player without worrying about it colliding
        // with the player directly.
        let start_angle = player_angle - std::f32::consts::FRAC_PI_2;
        let angle_amount = PI;
        let amount_of_points = 8;
        let points = create_circle_points(
            PLAYER_THICKNESS / 2.0,
            Vec2::new(x_pos, y_pos),
            start_angle,
            angle_amount,
            amount_of_points,
        );

        let player_is_jumping = !jump_timer.finished();
        for point in points {
            if colliders.is_collision(point.into(), player_is_jumping) {
                death_event_writer.send(DeathEvent(*player_id));
                despawn_entity(&mut commands, entity);
                continue;
            }
        }
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    mut jump_event_writer: EventWriter<JumpEvent>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                match prev_action {
                    ActionEvent::APressed => {
                        jump_event_writer.send(JumpEvent(player.id()));
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

fn move_achtung_players(
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut RigidBodyVelocity, &RigidBodyPosition)>,
    start_timer_query: Query<&StartTimer>,
) {
    let delta_tick = time.delta_seconds();
    for (player_id, mut velocity, pos) in player_query.iter_mut() {
        if let Some(player) = players.get(player_id) {
            // The rotation can still be changed even when the `start_timer` hasn't
            // finished yet.
            let movement_x = player.movement_x();
            if movement_x > 0.0 {
                velocity.angvel = -ACHTUNG_CONSTANT_TORQUE * delta_tick;
            } else if movement_x < 0.0 {
                velocity.angvel = ACHTUNG_CONSTANT_TORQUE * delta_tick;
            } else {
                velocity.angvel = 0.0;
            }

            if start_timer_query.single().unwrap().finished() {
                let player_angle = pos.position.rotation.angle() + std::f32::consts::FRAC_PI_2;
                let player_heading_vec = Vec2::new(player_angle.cos(), player_angle.sin());

                velocity.linvel = (player_heading_vec * ACHTUNG_CONSTANT_SPEED * delta_tick).into();
            }
        }
    }
}

fn reset_votes(mut exit_event_writer: EventWriter<VoteEvent>) {
    exit_event_writer.send(VoteEvent::Reset);
}

fn spawn_achtung_player(commands: &mut Commands, player: &Player, mut pos: Vec2, rotation: f32) {
    let white_color = Color::rgb(1.0, 1.0, 1.0);
    let center = Vec2::ZERO;
    let mut radius = PLAYER_THICKNESS / 2.0;

    let tail_spawn = TailSpawn::new(pos.x, pos.y);

    let player_shape_bundle = GeometryBuilder::build_as(
        &Shape::circle(radius, center),
        ShapeColors::new(player.color().as_bevy()),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 0.0),
    );

    let start_arrow_bundle = GeometryBuilder::build_as(
        &Shape::triangle(10.0, Vec2::ZERO),
        ShapeColors::new(white_color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(0.0, PLAYER_THICKNESS, 0.0),
    );

    pos /= RAPIER_SCALE_FACTOR;
    radius /= RAPIER_SCALE_FACTOR;

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

    let collider = ColliderBundle {
        shape: ColliderShape::ball(radius),
        ..Default::default()
    };

    let mut entity_commands = commands.spawn_bundle(rigid_body);
    entity_commands
        .insert_bundle(player_shape_bundle)
        .insert_bundle(collider)
        .insert(player.id())
        .insert(tail_spawn)
        .insert(JumpTimer::default())
        .insert(JumpCooldownTimer::default())
        .insert(AchtungGamePlugin)
        .insert(ColliderPositionSync::Discrete)
        .with_children(|parent| {
            parent.spawn_bundle(start_arrow_bundle).insert(StartEntity);
        });
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let font = fonts.regular.clone();
    let font_size = 24.0;
    let font_color = Color::WHITE;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let explotion_text = Text::with_section(
        JUMP_TEXT,
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

fn setup_colliders(mut commands: Commands) {
    commands
        .spawn()
        .insert(Colliders::new(TAIL_THICKNESS))
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
        TAIL_THICKNESS,
        ColliderType::Sensor,
        Some(Wall),
    )
    .insert(AchtungGamePlugin);
}
