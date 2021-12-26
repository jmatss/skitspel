use bevy::{
    core::Time,
    ecs::system::EntityCommands,
    math::Vec2,
    prelude::{Color, Commands, Query, Res, Transform},
};
use bevy_prototype_lyon::prelude::{DrawMode, FillOptions, GeometryBuilder, ShapeColors};
use bevy_rapier2d::{
    physics::{ColliderBundle, ColliderPositionSync, RigidBodyBundle},
    prelude::{
        ColliderMassProps, ColliderMaterial, ColliderShape, ColliderType, RigidBodyActivation,
        RigidBodyDamping, RigidBodyMassProps, RigidBodyType, RigidBodyVelocity,
    },
};
use rand::Rng;

use skitspel::{PlayerId, Players, ACCEL_AMOUNT, RAPIER_SCALE_FACTOR, VERTEX_AMOUNT};
use util_bevy::Shape;

use crate::create_polygon_points;

/// System that moves players according to the current inputs inside `Players`.
/// The default `ACCEL_AMOUNT` is used.
pub fn move_players(
    time: Res<Time>,
    players: Res<Players>,
    mut player_query: Query<(&PlayerId, &mut RigidBodyVelocity, &RigidBodyMassProps)>,
) {
    let delta_tick = time.delta_seconds();
    for (player_id, mut velocity, mass) in player_query.iter_mut() {
        if let Some(player) = players.get(player_id) {
            let movement_x = player.movement_x();
            let movement_y = player.movement_y();
            let movement_vec = Vec2::new(movement_x, movement_y) * ACCEL_AMOUNT * delta_tick;
            velocity.apply_impulse(mass, movement_vec.into());
        }
    }
}

/// Spawns a player conssisting of:
///  - ShapeBundle
///  - Collider
///  - RigidBody
/// The spawned entity will be tagged with the `player_id`.
pub fn spawn_player<'a, 'b>(
    commands: &'b mut Commands<'a>,
    player_id: PlayerId,
    color: Color,
    mut pos: Vec2,
    mut radius: f32,
) -> EntityCommands<'a, 'b> {
    let vertex_idx = rand::thread_rng().gen_range(0..VERTEX_AMOUNT.len());
    let vertex_amount = VERTEX_AMOUNT[vertex_idx];
    let center = Vec2::ZERO;

    let shape_bundle = GeometryBuilder::build_as(
        &Shape::new(radius, center, vertex_amount),
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

    let collider_shape = if vertex_amount == 0 {
        ColliderShape::ball(radius)
    } else {
        ColliderShape::convex_hull(&create_polygon_points(vertex_amount, radius, center))
            .unwrap_or_else(|| {
                panic!(
                    "Unable to create convex_hull with sides: {}",
                    vertex_amount + 1
                )
            })
    };

    let collider = ColliderBundle {
        collider_type: ColliderType::Solid,
        shape: collider_shape,
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
        .insert_bundle(shape_bundle)
        .insert_bundle(collider)
        .insert(player_id)
        .insert(ColliderPositionSync::Discrete);

    entity_commands
}
