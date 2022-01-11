use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};

use bevy::{
    core::{Time, Timer},
    math::Vec2,
    prelude::{
        AppBuilder, Assets, BuildChildren, Changed, Color, Commands, Entity, EventReader,
        EventWriter, Handle, HorizontalAlign, IntoSystem, Local, Mesh, MeshBundle,
        ParallelSystemDescriptorCoercion, Plugin, Query, QuerySet, RenderPipelines, Res, ResMut,
        State, SystemSet, Transform, VerticalAlign, With, Without,
    },
    render::mesh::VertexAttributeValues,
    text::{Font, Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    physics::{
        ColliderBundle, ColliderPositionSync, IntoEntity, RapierConfiguration, RigidBodyBundle,
        RigidBodyPositionSync,
    },
    prelude::{
        ActiveEvents, ColliderFlags, ColliderMassProps, ColliderMaterial, ColliderShape,
        ColliderType, InteractionGroups, IntersectionEvent, RigidBodyActivation, RigidBodyCcd,
        RigidBodyDamping, RigidBodyMassProps, RigidBodyMassPropsFlags, RigidBodyPosition,
        RigidBodyType, RigidBodyVelocity,
    },
    render::RapierRenderPlugin,
};
use rand::{prelude::SliceRandom, Rng};

use skitspel::{
    ActionEvent, ConnectedPlayers, DisconnectedPlayers, GameState, Player, PlayerId, Players,
    GAME_HEIGHT, GAME_WIDTH, PLAYER_RADIUS, RAPIER_SCALE_FACTOR, VERTEX_AMOUNT,
};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, handle_start_timer,
    setup_start_timer, AsBevyColor, Fonts, PlayerVote, Shape, StartTimer, VoteEvent,
};
use util_rapier::{
    create_path_with_thickness, create_polygon_points_with_angle, move_players, spawn_border_walls,
    spawn_player_with_vertex_amount,
};

// Regarding collisions between the invisible wall to prevent players from jumping
// over to the other side: The ball will be only assigned to collision group 0.
// The collider for the invisible wall over the net will then have its collision
// with group 0 disabled.

const GAME_STATE: GameState = GameState::VolleyBallGame;

const BOTTOM_SPAWN_POS_Y: f32 = -GAME_HEIGHT / 2.0 + PLAYER_RADIUS / 2.0;

const SPAWN_POSITIONS_LEFT: [(f32, f32); 5] = [
    (-GAME_WIDTH * 0.083, BOTTOM_SPAWN_POS_Y),
    (-GAME_WIDTH * 0.166, BOTTOM_SPAWN_POS_Y),
    (-GAME_WIDTH * 0.250, BOTTOM_SPAWN_POS_Y),
    (-GAME_WIDTH * 0.333, BOTTOM_SPAWN_POS_Y),
    (-GAME_WIDTH * 0.416, BOTTOM_SPAWN_POS_Y),
];

const SPAWN_POSITIONS_RIGHT: [(f32, f32); 5] = [
    (GAME_WIDTH * 0.083, BOTTOM_SPAWN_POS_Y),
    (GAME_WIDTH * 0.166, BOTTOM_SPAWN_POS_Y),
    (GAME_WIDTH * 0.250, BOTTOM_SPAWN_POS_Y),
    (GAME_WIDTH * 0.333, BOTTOM_SPAWN_POS_Y),
    (GAME_WIDTH * 0.416, BOTTOM_SPAWN_POS_Y),
];

/// Component used to tag the text containing the score.
struct LeftScoreText;
struct RightScoreText;

const PUSH_TEXT: &str = "Press A to push\n";
const EXIT_TEXT: &str = "Press B to go back to main menu";

/// How long the timer between rounds are in seconds.
const START_TIMER_TIME: usize = 3;

/// The time a push from a player is active. The player can't start another
/// push event during this period.
const PUSH_TIME: f32 = 0.5;

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

/// Tag used on the ball.
struct Ball;

/// Tag used on the goals (the floor in this case).
struct Goal;

/// Component used to keep track of the current score. The left usize is the
/// score of the left team and the right usize is the score of the right team.
struct ScoreCount(usize, usize);

/// Event created when player with ID `PlayerId` presses the push key.
struct PushEvent(PlayerId);

/// Timer used to restrict how often a player can push.
struct PushTimer(Timer);

impl Deref for PushTimer {
    type Target = Timer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PushTimer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for PushTimer {
    fn default() -> Self {
        // The timer should start as finished.
        let mut timer = Timer::from_seconds(PUSH_TIME, false);
        timer.tick(Duration::from_secs_f32(PUSH_TIME));
        Self(timer)
    }
}

/// Tag used for the push "animation" & collider. The PlayerId is the ID of the
/// player that spawned the push and the Vec2 is the direction vector in the
/// direction that this push "part" is traveling.
struct Push(PlayerId, Vec2);

/// Component used to keep track of the shape of a player. This information is
/// used when creating the "push" animation.
#[derive(Debug, Clone, Copy)]
struct PlayerShape {
    vertex_amount: usize,
    radius: f32,
}

#[derive(Clone, Default)]
pub struct VolleyBallGamePlugin;

impl Plugin for VolleyBallGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_event::<PushEvent>()
            .add_plugin(RapierRenderPlugin)
            .add_system_set(
                SystemSet::on_enter(GAME_STATE)
                    .with_system(reset_votes.system())
                    .with_system(setup_gravity.system())
                    .with_system(setup_map.system())
                    .with_system(
                        setup_start_timer::<VolleyBallGamePlugin, START_TIMER_TIME>.system(),
                    )
                    .with_system(setup_score.system())
                    .with_system(setup_players.system().label("players"))
                    .with_system(setup_screen_text.system().after("players")),
            )
            .add_system_set(
                SystemSet::on_update(GAME_STATE)
                    .with_system(handle_disconnect.system().label("vote"))
                    .with_system(handle_connect.system().label("vote"))
                    .with_system(handle_player_input.system().label("vote").label("push"))
                    .with_system(handle_exit_event.system().after("vote"))
                    .with_system(handle_goal.system().label("goal"))
                    .with_system(handle_start_timer.system().label("start").after("goal"))
                    .with_system(update_scoreboard.system())
                    .with_system(move_players.system())
                    .with_system(handle_ball_fall.system().after("start"))
                    .with_system(update_push_timers.system().label("timer").after("push"))
                    .with_system(
                        handle_spawn_push
                            .system()
                            .label("spawn_push")
                            .after("start")
                            .after("timer"),
                    )
                    .with_system(handle_push.system().after("spawn_push")),
            )
            .add_system_set(
                SystemSet::on_exit(GAME_STATE)
                    .with_system(despawn_system::<VolleyBallGamePlugin>.system())
                    .with_system(teardown_gravity.system()),
            );
    }
}

fn setup_gravity(mut configuration: ResMut<RapierConfiguration>) {
    configuration.gravity = [0.0, -9.81].into();
}

fn teardown_gravity(mut configuration: ResMut<RapierConfiguration>) {
    configuration.gravity = [0.0, 0.0].into();
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

        spawn_volleyball_player(&mut commands, player, spawn_pos.into(), team);
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

/// Slows down the speed in which the ball is falling. A force will be applied
/// at every tick to make sure that the ball falls slower.
///
/// If the game is paused or in the "StartTimer", this system will make sure
/// that the ball is stuck in the air.
///
/// When we exit the "StartTimer", the ball will be given a push upwards and
/// to an arbitrary side to start them game.
fn handle_ball_fall(
    time: Res<Time>,
    mut ball_query: Query<
        (
            &mut RigidBodyVelocity,
            &mut RigidBodyPosition,
            &RigidBodyMassProps,
        ),
        With<Ball>,
    >,
    start_timer_query: Query<&StartTimer>,
) {
    let delta_tick = time.delta_seconds();
    for (mut velocity, mut pos, mass) in ball_query.iter_mut() {
        let start_timer = start_timer_query.single().unwrap();
        if start_timer.just_finished() {
            // The timer just finished and the game is starting.
            let force_x = rand::thread_rng().gen_range(-1.0..=1.0);
            let heading_vec = Vec2::new(force_x, 0.5).normalize();
            velocity.apply_impulse(mass, (heading_vec * 15.0).into())
        } else if start_timer.finished() {
            // "Normal" operation, slow down the ball so it doesn't fall so fast.
            velocity.apply_impulse(mass, (Vec2::Y * delta_tick * 5.0).into());
        } else {
            // StartTimer counting down, so keep the ball still in the middle.
            reset_ball(&mut pos, &mut velocity);
        }
    }
}

/// Checks collisions between ball and goal (floor). Updates scores and resets ball
/// & player positions if a collision is found.
#[allow(clippy::too_many_arguments)]
fn handle_goal(
    mut commands: Commands,
    mut intersection_event: EventReader<IntersectionEvent>,
    mut players: ResMut<Players>,
    players_playing: Query<(Entity, &PlayerId, &Team), Without<Push>>,
    mut score_count: Query<&mut ScoreCount>,
    mut ball_query: Query<(&mut RigidBodyPosition, &mut RigidBodyVelocity), With<Ball>>,
    push_query: Query<Entity, With<Push>>,
    goal_query: Query<&Team, With<Goal>>,
    mut start_timer_query: Query<&mut StartTimer>,
) {
    for intersection in intersection_event.iter() {
        if intersection.intersecting {
            let entity_a = intersection.collider1.entity();
            let entity_b = intersection.collider2.entity();
            let goal_entity =
                if ball_query.get_mut(entity_a).is_ok() && goal_query.get(entity_b).is_ok() {
                    entity_b
                } else if ball_query.get_mut(entity_b).is_ok() && goal_query.get(entity_a).is_ok() {
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

            // TODO: Can there be a panic if we remove it here and in the same
            //       frame the PushTimer finishes and the `handle_push`
            //       despawn the entity as well?
            // Remove the currently active pushes.
            for entity in push_query.iter() {
                despawn_entity(&mut commands, entity);
            }

            // Despawn the old players and respawn them with new shapes.
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
                    spawn_volleyball_player(&mut commands, player, spawn_pos.into(), *team);
                }
            }

            let (mut ball_pos, mut ball_velocity) = ball_query.single_mut().unwrap();
            reset_ball(&mut ball_pos, &mut ball_velocity);

            start_timer_query.single_mut().unwrap().reset();
        }
    }
}

fn handle_player_input(
    mut players: ResMut<Players>,
    mut push_event_writer: EventWriter<PushEvent>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                match prev_action {
                    ActionEvent::APressed => {
                        push_event_writer.send(PushEvent(player.id()));
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

fn update_push_timers(
    time: Res<Time>,
    mut push_timer_query: Query<&mut PushTimer>,
    start_timer_query: Query<&StartTimer>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    for mut timer in push_timer_query.iter_mut() {
        timer.tick(time.delta());
    }
}

/// Handles the spawning of pushes from players.
#[allow(clippy::too_many_arguments)]
fn handle_spawn_push(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    render_pipelines: Res<RenderPipelines>,
    players: Res<Players>,
    mut player_query: Query<(
        &PlayerId,
        &mut PushTimer,
        &RigidBodyVelocity,
        &RigidBodyPosition,
        &PlayerShape,
    )>,
    start_timer_query: Query<&StartTimer>,
    mut push_event_reader: EventReader<PushEvent>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    for PushEvent(event_player_id) in push_event_reader.iter() {
        // TODO: More performant way to get player_query from player_id?
        for (player_id, mut push_timer, player_vel, player_pos, player_shape) in
            player_query.iter_mut()
        {
            // Don't spawn a new one if the timer just finished. The old one needs
            // one tick to be despawned.
            if event_player_id == player_id && push_timer.finished() && !push_timer.just_finished()
            {
                push_timer.reset();

                if let Some(player) = players.get(player_id) {
                    spawn_push(
                        &mut commands,
                        &mut meshes,
                        &render_pipelines,
                        player,
                        *player_shape,
                        *player_vel,
                        *player_pos,
                    );
                }
            }
        }
    }
}

/// Handles the movement and animation of pushes. Also despawn the pushes after
/// the `PushTimer` finishes.
fn handle_push(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    player_query: Query<(&PlayerId, &PushTimer)>,
    mut push_query: Query<(Entity, &Push, &mut RigidBodyVelocity, &Handle<Mesh>)>,
    start_timer_query: Query<&StartTimer>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    let delta_tick = time.delta_seconds();
    for (player_id, push_timer) in player_query.iter() {
        if push_timer.just_finished() {
            // The push just finished, remove the push "animation".
            remove_push(&mut commands, *player_id, &mut push_query);
        } else if !push_timer.finished() {
            // Value between 0 & 1.
            let elapsed = push_timer.percent();
            let alg = (-2.0 * elapsed.powf(2.0) + 2.0) * 2000.0;
            for (_, Push(push_player_id, heading_vec), mut push_vel, mesh) in push_query.iter_mut()
            {
                if player_id == push_player_id {
                    push_vel.linvel = (*heading_vec * alg * delta_tick).into();

                    // Decrease opacity of the push animation.
                    if let Some(VertexAttributeValues::Float4(colors)) = meshes
                        .get_mut(mesh)
                        .map(|m| m.attribute_mut(Mesh::ATTRIBUTE_COLOR))
                        .flatten()
                    {
                        for color in colors {
                            color[3] = push_timer.percent_left();
                        }
                    }
                }
            }
        }
    }
}

fn remove_push(
    commands: &mut Commands,
    player_id: PlayerId,
    push_query: &mut Query<(Entity, &Push, &mut RigidBodyVelocity, &Handle<Mesh>)>,
) {
    for (entity, expand_tag, ..) in push_query.iter_mut() {
        if expand_tag.0 == player_id {
            despawn_entity(commands, entity);
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
        .insert(VolleyBallGamePlugin);

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
        .insert(VolleyBallGamePlugin);
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

fn spawn_volleyball_player(commands: &mut Commands, player: &Player, spawn_pos: Vec2, team: Team) {
    // Prevent round shape.
    let vertex_idx = rand::thread_rng().gen_range(0..VERTEX_AMOUNT.len() - 1);
    let vertex_amount = VERTEX_AMOUNT[vertex_idx + 1];

    // The second group will be used by the "push" colliders. The player shouldn't
    // be able to collide with them.
    let collider_flags = ColliderFlags {
        collision_groups: InteractionGroups::new(!0b10, !0b10),
        ..Default::default()
    };

    spawn_player_with_vertex_amount(
        commands,
        player.id(),
        player.color().as_bevy(),
        spawn_pos,
        PLAYER_RADIUS,
        vertex_amount,
        collider_flags,
    )
    .insert(PlayerShape {
        vertex_amount,
        radius: PLAYER_RADIUS,
    })
    .insert(team)
    .insert(PushTimer::default())
    .insert(VolleyBallGamePlugin);
}

/// There is no way to scale a collider in bevy-rapier. We therefore have to
/// remove the old entity and create a copy with changed size.
fn spawn_push(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    render_pipelines: &RenderPipelines,
    player: &Player,
    player_shape: PlayerShape,
    player_vel: RigidBodyVelocity,
    player_pos: RigidBodyPosition,
) {
    let thickness = 3.0;
    let angle = player_pos.position.rotation.angle();
    let pos = Vec2::new(
        player_pos.position.translation.x * RAPIER_SCALE_FACTOR,
        player_pos.position.translation.y * RAPIER_SCALE_FACTOR,
    );

    let points = create_polygon_points_with_angle(
        player_shape.vertex_amount,
        player_shape.radius,
        pos,
        angle,
    );

    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];
        let mut half_p = Vec2::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
        let heading_vec = Vec2::new(half_p.x - pos.x, half_p.y - pos.y).normalize();

        let (mesh, colliders) = create_path_with_thickness(
            &[Vec2::new(p1.x, p1.y), Vec2::new(p2.x, p2.y)],
            player.color().as_bevy(),
            thickness,
            ColliderType::Solid,
            // The second group will be used by the "push" colliders. Should only be able
            // to interact with the ball (group 1).
            ColliderFlags {
                collision_groups: InteractionGroups::new(0b10, 0b1),
                ..Default::default()
            },
            false,
        );

        half_p /= RAPIER_SCALE_FACTOR;

        let rigid_body = RigidBodyBundle {
            body_type: RigidBodyType::Dynamic,
            damping: RigidBodyDamping {
                linear_damping: 0.0,
                angular_damping: 1.0,
            },
            ccd: RigidBodyCcd {
                ccd_enabled: true,
                ..Default::default()
            },
            // Spawn the push with the velocity of the player to make the movement
            // more "fluid" and look more "realistic".
            velocity: player_vel,
            mass_properties: RigidBodyMassPropsFlags::ROTATION_LOCKED.into(),
            activation: RigidBodyActivation::cannot_sleep(),
            ..Default::default()
        };

        let mut entity_commands = commands.spawn_bundle(rigid_body);

        entity_commands
            .insert_bundle(MeshBundle {
                mesh: meshes.add(mesh),
                render_pipelines: render_pipelines.clone(),
                ..Default::default()
            })
            .insert(Push(player.id(), heading_vec))
            .insert(VolleyBallGamePlugin)
            .insert(RigidBodyPositionSync::Discrete);

        for collider in colliders.into_iter() {
            entity_commands.with_children(|parent| {
                parent.spawn_bundle(collider);
            });
        }
    }
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let bold_font = fonts.bold.clone();
    let bold_font_size = 128.0;
    let regular_font = fonts.regular.clone();
    let regular_font_size = 24.0;
    let font_color = Color::WHITE;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let expand_text = Text::with_section(
        PUSH_TEXT,
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

    let expand_text_bundle = Text2dBundle {
        text: expand_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0 - regular_font_size, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(expand_text_bundle)
        .insert(VolleyBallGamePlugin);

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
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(exit_text_bundle)
        .insert(ExitText)
        .insert(VolleyBallGamePlugin);

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
        .insert(VolleyBallGamePlugin);

    let right_score_text_bundle = Text2dBundle {
        text: zero_score_text,
        transform: Transform::from_xyz(GAME_WIDTH / 4.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(right_score_text_bundle)
        .insert(RightScoreText)
        .insert(VolleyBallGamePlugin);
}

fn setup_score(mut commands: Commands) {
    commands
        .spawn()
        .insert(ScoreCount(0, 0))
        .insert(VolleyBallGamePlugin);
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
        spawn_volleyball_player(&mut commands, player, spawn_pos.into(), team);
    }
}

fn setup_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    render_pipelines: Res<RenderPipelines>,
) {
    let wall_thickness = 10.0;
    let net_thickness = 20.0;
    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let grey_color = Color::rgb(0.3, 0.3, 0.3);
    let white_color = Color::rgb(1.0, 1.0, 1.0);

    // No interaction with the push colliders.
    let collider_flags = ColliderFlags {
        collision_groups: InteractionGroups::new(!0b0, !0b10),
        ..Default::default()
    };

    spawn_border_walls(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        grey_color,
        wall_thickness,
        ColliderType::Solid,
        collider_flags,
        Option::<VolleyBallGamePlugin>::None,
    )
    .insert(VolleyBallGamePlugin);

    // Put a post/circle at the top of the net.
    let post_pos = Vec2::new(0.0, -GAME_HEIGHT / 4.0);
    spawn_post(&mut commands, post_pos, net_thickness / 2.0, grey_color);

    let net_vertices = [
        Vec2::new(0.0, -GAME_HEIGHT / 2.0),
        Vec2::new(0.0, -GAME_HEIGHT / 4.0),
    ];
    let (mesh, colliders) = create_path_with_thickness(
        &net_vertices,
        grey_color,
        net_thickness,
        ColliderType::Solid,
        collider_flags,
        false,
    );

    let mut entity_commands = commands.spawn_bundle(MeshBundle {
        mesh: meshes.add(mesh),
        render_pipelines: render_pipelines.clone(),
        ..Default::default()
    });

    colliders.into_iter().for_each(|collider| {
        entity_commands.with_children(|parent| {
            parent.spawn_bundle(collider);
        });
    });

    entity_commands.insert(VolleyBallGamePlugin);

    let blocking_over_net_vertices = [
        Vec2::new(0.0, -GAME_HEIGHT / 4.0),
        Vec2::new(0.0, GAME_HEIGHT / 2.0),
    ];
    let (_, colliders) = create_path_with_thickness(
        &blocking_over_net_vertices,
        white_color,
        net_thickness,
        ColliderType::Solid,
        // Should not interact with group 0 or 1 (the ball group and the push
        // group respectively).
        ColliderFlags {
            collision_groups: InteractionGroups::new(!0b11, !0b11),
            ..Default::default()
        },
        false,
    );

    let mut entity_commands = commands.spawn();

    colliders.into_iter().for_each(|collider| {
        entity_commands.with_children(|parent| {
            parent.spawn_bundle(collider);
        });
    });

    entity_commands.insert(VolleyBallGamePlugin);

    let goal_width = GAME_WIDTH / 2.0;
    let goal_height = wall_thickness + 1.0;

    let left_floor_goal = Vec2::new(-GAME_WIDTH / 4.0, -GAME_HEIGHT / 2.0 + goal_height / 2.);
    spawn_goal(
        &mut commands,
        left_floor_goal,
        goal_width,
        goal_height,
        red_color,
        Team::Left,
    );

    let right_floor_goal = Vec2::new(GAME_WIDTH / 4.0, -GAME_HEIGHT / 2.0 + goal_height / 2.0);
    spawn_goal(
        &mut commands,
        right_floor_goal,
        goal_width,
        goal_height,
        red_color,
        Team::Right,
    );

    spawn_ball(&mut commands, 50.0, white_color);
}

fn spawn_goal(
    commands: &mut Commands,
    mut pos: Vec2,
    mut width: f32,
    mut height: f32,
    color: Color,
    team: Team,
) {
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
        .insert(VolleyBallGamePlugin);
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
        flags: ColliderFlags {
            collision_groups: InteractionGroups::new(!0b0, !0b10),
            ..Default::default()
        },
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
        .insert(VolleyBallGamePlugin);
}

fn spawn_ball(commands: &mut Commands, mut radius: f32, color: Color) {
    let mut default_pos = Vec2::ZERO;

    let shape = GeometryBuilder::build_as(
        &Shape::circle(radius, Vec2::ZERO),
        ShapeColors::new(color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(default_pos.x, default_pos.y, 0.0),
    );

    default_pos /= RAPIER_SCALE_FACTOR;
    radius /= RAPIER_SCALE_FACTOR;

    let mut rigid_body = RigidBodyBundle {
        body_type: RigidBodyType::Dynamic,
        damping: RigidBodyDamping {
            linear_damping: 0.5,
            angular_damping: 0.5,
        },
        ccd: RigidBodyCcd {
            ccd_enabled: true,
            ..Default::default()
        },
        position: default_pos.into(),
        activation: RigidBodyActivation::cannot_sleep(),
        ..Default::default()
    };

    let collider = ColliderBundle {
        collider_type: ColliderType::Solid,
        shape: ColliderShape::ball(radius),
        flags: ColliderFlags {
            collision_groups: InteractionGroups::new(0b1, 0b11),
            ..Default::default()
        },
        material: ColliderMaterial {
            friction: 0.05,
            restitution: 0.9,
            ..Default::default()
        },
        mass_properties: ColliderMassProps::Density(0.1),
        ..Default::default()
    };

    reset_ball(&mut rigid_body.position, &mut rigid_body.velocity);

    commands
        .spawn_bundle(rigid_body)
        .insert_bundle(shape)
        .insert_bundle(collider)
        .insert(Ball)
        .insert(VolleyBallGamePlugin)
        .insert(ColliderPositionSync::Discrete);
}

// Reset ball to ~middle of field.
fn reset_ball(pos: &mut RigidBodyPosition, velocity: &mut RigidBodyVelocity) {
    pos.position = Vec2::new(0.0, (GAME_HEIGHT / 8.0) / RAPIER_SCALE_FACTOR).into();
    velocity.linvel = Vec2::ZERO.into();
    velocity.angvel = 0.0;
}
