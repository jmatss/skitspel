use std::cmp::Reverse;

use bevy::{
    core::Time,
    math::Vec2,
    prelude::{
        AppBuilder, Assets, Color, Commands, Entity, EventReader, EventWriter, HorizontalAlign,
        IntoSystem, Local, Mesh, ParallelSystemDescriptorCoercion, Plugin, Query, RenderPipelines,
        Res, ResMut, State, SystemSet, Transform, VerticalAlign, With,
    },
    text::{Text, Text2dBundle, TextAlignment, TextSection, TextStyle},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    physics::{ColliderBundle, IntoEntity, RigidBodyBundle},
    prelude::{
        ActiveEvents, ColliderFlags, ColliderMassProps, ColliderMaterial, ColliderShape,
        ColliderType, IntersectionEvent, RigidBodyActivation, RigidBodyDamping, RigidBodyMassProps,
        RigidBodyType, RigidBodyVelocity,
    },
};
use rand::prelude::SliceRandom;

use skitspel::{
    ActionEvent, DisconnectedPlayers, GameState, PlayerId, Players, GAME_HEIGHT, GAME_WIDTH,
    MAX_PLAYERS, PLAYER_RADIUS, RAPIER_SCALE_FACTOR, TORQUE_ACCEL_AMOUNT,
};
use util_bevy::{
    create_vote_text_sections, despawn_entity, despawn_system, handle_start_timer,
    setup_start_timer, AsBevyColor, Fonts, PlayerVote, Shape, StartTimer, VoteEvent,
};
use util_rapier::{move_players, spawn_border_walls, spawn_player};

const GAME_STATE: GameState = GameState::PushGame;

const SPAWN_POSITIONS: [(f32, f32); MAX_PLAYERS] = [
    (-GAME_WIDTH * 0.375, GAME_HEIGHT * 0.375),
    (GAME_WIDTH * 0.375, -GAME_HEIGHT * 0.375),
    (GAME_WIDTH * 0.375, GAME_HEIGHT * 0.375),
    (-GAME_WIDTH * 0.375, -GAME_HEIGHT * 0.375),
    (GAME_WIDTH * 0.00, -GAME_HEIGHT * 0.375),
    (-GAME_WIDTH * 0.25, GAME_HEIGHT * 0.00),
    (GAME_WIDTH * 0.25, GAME_HEIGHT * 0.00),
    (GAME_WIDTH * 0.00, -GAME_HEIGHT * 0.25),
    (GAME_WIDTH * 0.00, GAME_HEIGHT * 0.25),
];

/// Component used to tag the entities that causes death if a player touches it.
#[derive(Debug, Clone)]
struct DeathCollider;

/// Component used to tag the text containing the score.
struct ScoreText;

const SPIN_TEXT: &str = "Press A to spin\n";
const EXIT_TEXT: &str = "Press B to go back to main menu";

/// How long the timer between rounds are in seconds.
const START_TIMER_TIME: usize = 3;

/// Tag used on the exit text.
struct ExitText;

/// Tag used on the pillar in the middle. It seems to be some problems with the
/// z-ordering in bevy, so the countdown StartText isn't being displayed on top
/// of the pillar.
///
/// A temporary hack is implemented to remove the `DeathPillar` during the countdown
/// and then spawn it in when the game starts.
struct DeathPillar;

#[derive(Debug, Default)]
pub struct PushGamePlugin;

impl Plugin for PushGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_system_set(
            SystemSet::on_enter(GAME_STATE)
                .with_system(reset_votes.system())
                .with_system(setup_map.system())
                .with_system(setup_start_timer::<PushGamePlugin, START_TIMER_TIME>.system())
                .with_system(setup_screen_text.system()),
        )
        .add_system_set(
            SystemSet::on_update(GAME_STATE)
                .with_system(handle_disconnect.system().label("vote"))
                .with_system(handle_player_input.system().label("vote"))
                .with_system(handle_exit_event.system().after("vote"))
                .with_system(handle_winner.system())
                .with_system(update_scoreboard.system())
                .with_system(reset_players.system().label("reset"))
                .with_system(handle_death_pillar.system().label("pillar").after("reset"))
                .with_system(handle_start_timer.system().after("pillar"))
                .with_system(move_players.system())
                .with_system(spin_players.system())
                .with_system(handle_death.system()),
        )
        .add_system_set(
            SystemSet::on_exit(GAME_STATE).with_system(despawn_system::<PushGamePlugin>.system()),
        );
    }
}

fn handle_disconnect(
    mut commands: Commands,
    disconnected_players: Res<DisconnectedPlayers>,
    players_alive_query: Query<(Entity, &PlayerId)>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if !disconnected_players.is_empty() {
        for (entity, player_id) in players_alive_query.iter() {
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
    mut start_timer_query: Query<&mut StartTimer>,
) {
    if players_alive_query.iter().count() <= 1 {
        // Remove any players that are still alive.
        for entity in players_alive_query.iter() {
            despawn_entity(&mut commands, entity);
        }

        let mut spawn_positions = SPAWN_POSITIONS;
        spawn_positions.shuffle(&mut rand::thread_rng());

        for (idx, player) in players.values_mut().enumerate() {
            // Reset action of player before respawning. This can prevent scenarios
            // where a player held down the button just before the round ended
            // and the player continues to go in the same direction when respawned.
            player.reset_action();

            let color = player.color().as_bevy();
            spawn_player(
                &mut commands,
                player.id(),
                color,
                spawn_positions[idx].into(),
                PLAYER_RADIUS,
            )
            .insert(PushGamePlugin);
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
            .insert(PushGamePlugin)
            .insert(ScoreText);
    }
}

fn spin_players(
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut RigidBodyVelocity, &RigidBodyMassProps)>,
) {
    let delta_tick = time.delta_seconds();
    for (player_id, mut velocity, mass) in player_query.iter_mut() {
        if let Some(player) = players.get(player_id) {
            if player.a_is_pressed() {
                velocity.apply_torque_impulse(mass, TORQUE_ACCEL_AMOUNT * delta_tick);
            }
        }
    }
}

/// Checks collisions between players and "red walls". Removes the player
/// entity if a collision is found.
fn handle_death(
    mut commands: Commands,
    mut intersection_event: EventReader<IntersectionEvent>,
    players_query: Query<Entity, With<PlayerId>>,
    death_walls_query: Query<Entity, With<DeathCollider>>,
    start_timer_query: Query<&StartTimer>,
) {
    if !start_timer_query.single().unwrap().finished() {
        return;
    }

    for intersection in intersection_event.iter() {
        if intersection.intersecting {
            let entity_a = intersection.collider1.entity();
            let entity_b = intersection.collider2.entity();
            if players_query.get(entity_a).is_ok() && death_walls_query.get(entity_b).is_ok() {
                despawn_entity(&mut commands, entity_a);
            } else if players_query.get(entity_b).is_ok() && death_walls_query.get(entity_a).is_ok()
            {
                despawn_entity(&mut commands, entity_b);
            }
        }
    }
}

/// It seems to be some problems with the z-ordering in bevy, so the countdown
/// StartText isn't being displayed on top of the pillar in the middle.
///
/// A temporary hack is implemented to remove the `DeathPillar` during the countdown
/// and then spawn it in when the game starts.
fn handle_death_pillar(
    mut commands: Commands,
    death_walls_query: Query<(Entity, &DeathPillar)>,
    start_timer_query: Query<&StartTimer>,
) {
    let start_timer = start_timer_query.single().unwrap();
    if start_timer.just_finished() {
        let pos = Vec2::ZERO;
        let radius = 120.0;
        let red_color = Color::rgb(1.0, 0.1, 0.1);
        spawn_pillar_death(&mut commands, pos, radius, red_color);
    } else if start_timer.elapsed_secs() == 0.0 {
        // True if just reset, remove pillar.
        for (entity, _) in death_walls_query.iter() {
            despawn_entity(&mut commands, entity);
        }
    }
}

/// Updates the components inside the `Players` according to which buttons the
/// players have pushed.
fn handle_player_input(
    mut players: ResMut<Players>,
    mut exit_event_writer: EventWriter<VoteEvent>,
) {
    if players.is_changed() {
        for player in players.values_mut() {
            if let Some(ActionEvent::BPressed) = player.previous_action_once() {
                exit_event_writer.send(VoteEvent::Flip(player.id()));
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

fn reset_votes(mut exit_event_writer: EventWriter<VoteEvent>) {
    exit_event_writer.send(VoteEvent::Reset);
}

fn setup_screen_text(mut commands: Commands, players: Res<Players>, fonts: Res<Fonts>) {
    let font = fonts.regular.clone();
    let font_size = 24.0;
    let font_color = Color::WHITE;

    let empty_player_vote = PlayerVote::default();
    let required_amount = (players.len() / 2) + 1;

    let spin_text = Text::with_section(
        SPIN_TEXT,
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

    let spin_text_bundle = Text2dBundle {
        text: spin_text,
        transform: Transform::from_xyz(0.0, GAME_HEIGHT / 4.0, 0.0),
        ..Default::default()
    };

    commands
        .spawn_bundle(spin_text_bundle)
        .insert(PushGamePlugin);

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
        .insert(PushGamePlugin)
        .insert(ExitText);
}

fn setup_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    render_pipelines: Res<RenderPipelines>,
) {
    let red_color = Color::rgb(1.0, 0.1, 0.1);
    let grey_color = Color::rgb(0.3, 0.3, 0.3);

    spawn_border_walls(
        &mut commands,
        &mut meshes,
        render_pipelines.clone(),
        red_color,
        10.0,
        ColliderType::Sensor,
        ActiveEvents::INTERSECTION_EVENTS.into(),
        Some(DeathCollider),
    )
    .insert(PushGamePlugin);

    let positions = [
        Vec2::new(-GAME_WIDTH * 0.25, GAME_HEIGHT * 0.20),
        Vec2::new(GAME_WIDTH * 0.25, GAME_HEIGHT * 0.20),
        Vec2::new(-GAME_WIDTH * 0.25, -GAME_HEIGHT * 0.20),
        Vec2::new(GAME_WIDTH * 0.25, -GAME_HEIGHT * 0.20),
    ];
    for pos in positions {
        spawn_pillar_wall(&mut commands, pos, 120.0, grey_color);
    }
}

fn spawn_pillar_wall(commands: &mut Commands, mut pos: Vec2, mut radius: f32, color: Color) {
    let shape = GeometryBuilder::build_as(
        &Shape::circle(radius, Vec2::ZERO),
        ShapeColors::new(color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 0.0),
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
        .insert(PushGamePlugin);
}

fn spawn_pillar_death(commands: &mut Commands, mut pos: Vec2, mut radius: f32, color: Color) {
    let shape = GeometryBuilder::build_as(
        &Shape::circle(radius, Vec2::ZERO),
        ShapeColors::new(color),
        DrawMode::Fill(FillOptions::DEFAULT),
        Transform::from_xyz(pos.x, pos.y, 0.0),
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
        collider_type: ColliderType::Sensor,
        flags: ColliderFlags {
            active_events: ActiveEvents::INTERSECTION_EVENTS,
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
        .insert(DeathCollider)
        .insert(DeathPillar)
        .insert(PushGamePlugin);
}
