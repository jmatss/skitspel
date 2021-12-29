use bevy::{
    core::Time,
    math::Vec2,
    prelude::{
        AppBuilder, Assets, BuildChildren, Changed, Color, Commands, Entity, EventReader,
        EventWriter, Handle, HorizontalAlign, IntoSystem, Local, Mesh, MeshBundle,
        ParallelSystemDescriptorCoercion, Plugin, Query, QuerySet, RenderPipelines, Res, ResMut,
        State, SystemSet, Transform, VerticalAlign, With,
    },
    text::{Font, Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    physics::{
        ColliderBundle, ColliderPositionSync, IntoEntity, RapierConfiguration, RigidBodyBundle,
    },
    prelude::{
        ActiveEvents, ColliderFlags, ColliderMassProps, ColliderMaterial, ColliderShape,
        ColliderType, InteractionGroups, IntersectionEvent, RigidBodyActivation, RigidBodyDamping,
        RigidBodyMassProps, RigidBodyPosition, RigidBodyType, RigidBodyVelocity,
    },
};
use rand::{prelude::SliceRandom, Rng};

use skitspel::{
    ActionEvent, ConnectedPlayers, DisconnectedPlayers, GameState, Player, PlayerId, Players,
    GAME_HEIGHT, GAME_WIDTH, PLAYER_RADIUS, RAPIER_SCALE_FACTOR,
};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, handle_start_timer,
    setup_start_timer, AsBevyColor, Fonts, PlayerVote, Shape, StartTimer, VoteEvent,
};
use util_rapier::{create_path_with_thickness, move_players, spawn_border_walls, spawn_player};

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

const EXIT_TEXT: &str = "Press B to go back to main menu";

/// How long the timer between rounds are in seconds.
const START_TIMER_TIME: usize = 3;

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

#[derive(Clone, Default)]
pub struct VolleyBallGamePlugin;

impl Plugin for VolleyBallGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_system_set(
            SystemSet::on_enter(GAME_STATE)
                .with_system(reset_votes.system())
                .with_system(setup_gravity.system())
                .with_system(setup_map.system())
                .with_system(setup_start_timer::<VolleyBallGamePlugin, START_TIMER_TIME>.system())
                .with_system(setup_score.system())
                .with_system(setup_players.system().label("players"))
                .with_system(setup_screen_text.system().after("players")),
        )
        .add_system_set(
            SystemSet::on_update(GAME_STATE)
                .with_system(handle_disconnect.system().label("vote"))
                .with_system(handle_connect.system().label("vote"))
                .with_system(handle_player_input.system().label("vote"))
                .with_system(handle_exit_event.system().after("vote"))
                .with_system(handle_goal.system().label("goal"))
                .with_system(handle_start_timer.system().label("start").after("goal"))
                .with_system(update_scoreboard.system())
                .with_system(move_players.system())
                .with_system(handle_ball_fall.system().after("start")),
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
    players_playing: Query<(Entity, &PlayerId, &Team)>,
    mut score_count: Query<&mut ScoreCount>,
    mut ball_query: Query<(&mut RigidBodyPosition, &mut RigidBodyVelocity), With<Ball>>,
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

            // Despawn the old players and respawn them with new shapes and on
            // their side of the field.
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
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(prev_action) = player.previous_action_once() {
                if prev_action == ActionEvent::BPressed {
                    exit_event_writer.send(VoteEvent::Flip(player.id()));
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
    spawn_player(
        commands,
        player.id(),
        player.color().as_bevy(),
        spawn_pos,
        PLAYER_RADIUS,
    )
    .insert(team)
    .insert(VolleyBallGamePlugin);
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let bold_font = fonts.bold.clone();
    let bold_font_size = 128.0;
    let regular_font = fonts.regular.clone();
    let regular_font_size = 24.0;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

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

    spawn_border_walls(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        grey_color,
        wall_thickness,
        ColliderType::Solid,
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
        ActiveEvents::empty(),
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
        ActiveEvents::empty(),
        false,
    );

    let mut entity_commands = commands.spawn();

    colliders.into_iter().for_each(|mut collider| {
        entity_commands.with_children(|parent| {
            // Should not interact with group 0 (the group used for the ball).
            collider.flags.collision_groups.memberships &= !0b1;
            collider.flags.collision_groups.filter &= !0b1;
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
        position: default_pos.into(),
        activation: RigidBodyActivation::cannot_sleep(),
        ..Default::default()
    };

    let collider = ColliderBundle {
        collider_type: ColliderType::Solid,
        shape: ColliderShape::ball(radius),
        flags: ColliderFlags {
            collision_groups: InteractionGroups::new(0b1, 0b1),
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
