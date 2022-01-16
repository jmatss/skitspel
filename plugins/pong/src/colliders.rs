use std::{collections::hash_map::Entry, f32::consts::TAU};

use bevy::{math::Vec2, utils::HashMap};

use skitspel::PlayerId;
use util_rapier::create_circle_points;

use crate::util;

/// Contains colliders used in the "Pong" Game.
///
/// TODO: Clean up explanation,
/// The screen will contain a circle that is divided into equal size per player.
#[derive(Debug)]
pub(crate) struct Colliders {
    /// The key is the player ID and the Vec2 is its position in the "outline"
    /// of the circle with radius `player_radius`.
    /// The position is the middle of the player paddle in the "inner" circle
    /// of the paddle.
    player_colliders: HashMap<PlayerId, Vec2>,

    /// Used to keep track of the player order. This is the order in which they
    /// are placed around the map. E.x. the player at index 0 in the vector is
    /// the player that has a goal that starts at angle 0 on the "inner" radius
    /// of the player circle.
    player_order: Vec<PlayerId>,

    /// Keeps track of how many players that have been inserted. This is needed
    /// since players might be removed from the `player_colliders` during play,
    /// which means that its len can't be used to determine the correct amount
    /// of players/goals.
    amount_of_players: usize,

    /// The radius of the circle that the players are located on. If the ball
    /// is equal or greater than this value, it might contact the player paddle.
    /// This is the radius of the "inner" circle (when considering that the
    /// players have a thickness when drawn on screen).
    player_radius_inner: f32,

    /// The (inner) radius of the circle that is used as the collider for the
    /// goals. If the balls position is equal to this value, it has hit a goal.
    /// This is the radius of the "inner" circle (when considering that the
    /// goals have a thickness when drawn on screen).
    goal_radius_inner: f32,

    /// The length of the player in relation to its goal size. The player will
    /// take up the length `player_length_frac` times the goal length.
    player_length_frac: f32,

    /// If set to true, there was a collision the previous tick. This will be
    /// used to not register collision multiple times in a short time. This can
    /// happen if the ball hits the side of the paddle. If the paddle is moving
    /// faster than the ball, it can hit it multiple times in a row.
    was_collision_prev_tick: bool,
}

impl Colliders {
    pub fn new(goal_radius_inner: f32, player_radius_inner: f32, player_length_frac: f32) -> Self {
        Self {
            player_colliders: HashMap::default(),
            player_order: Vec::default(),
            amount_of_players: 0,
            player_radius_inner,
            goal_radius_inner,
            player_length_frac,
            was_collision_prev_tick: false,
        }
    }

    /// Resets and clears the colliders stored inside this struct.
    pub fn reset(&mut self) {
        self.player_colliders.clear();
        self.player_order.clear();
        self.amount_of_players = 0;
        self.was_collision_prev_tick = false;
    }

    /// Adds the given player as a collider.
    pub fn add_player(&mut self, player_id: PlayerId, player_collider: Vec2) {
        self.player_colliders.insert(player_id, player_collider);
        self.player_order.push(player_id);
        self.amount_of_players += 1;
    }

    /// Moves the collider for the player with ID `player_id` `angle` radians
    /// relative to its current collider position.
    pub fn move_player(&mut self, player_id: PlayerId, angle: f32) {
        if let Entry::Occupied(mut old_pos) = self.player_colliders.entry(player_id) {
            let (radius, old_angle) = util::cartesian_to_polar(*old_pos.get());
            let new_angle = old_angle + angle;
            *old_pos.get_mut() = util::polar_to_cartesian(radius, new_angle);
        }
    }

    /// Removes the collider for the player with ID `player_id`.
    pub fn remove_player(&mut self, player_id: PlayerId) {
        // Let the player be left in `self.player_order` & `self.amount_of_player`.
        // Since we only want to remove the collider for the player paddle and
        // let the goal be left as it is, we don't change anyting in the other
        // variables.
        self.player_colliders.remove(&player_id);
    }

    /// Returns true if the player with ID `player_id` can move `angle` radians
    /// from its current position.
    ///
    /// Since the players should only be able to move infront of their goals,
    /// this will return false when the player is at the edge of its goal.
    pub fn can_move_player(&self, player_id: PlayerId, angle: f32) -> bool {
        if let Some(old_pos) = self.player_colliders.get(&player_id) {
            let (_, old_angle) = util::cartesian_to_polar(*old_pos);
            let new_angle = util::convert_angle_inv(old_angle + angle);

            let player_goal_start_angle = self.player_goal_start_angle(player_id);
            let goal_length_angle = util::angle_per_player(self.amount_of_players);
            let player_length_angle = goal_length_angle * self.player_length_frac;

            // Should be stopped when the edge of the player paddle touches the
            // edge of the goal, so need to calculate `+- layer_length_angle / 2.0`.
            let start_angle = player_goal_start_angle + player_length_angle / 2.0;
            let angle_amount = goal_length_angle - player_length_angle;

            util::is_between_angles(new_angle, start_angle, angle_amount)
        } else {
            eprintln!(
                "can_move_player -- player_id: {}, Colliders: {:#?}",
                player_id, self
            );
            false
        }
    }

    /// Returns `Some` if the ball is colliding with a player paddle. The returned
    /// tuple contains the ID of the player with the paddle that is colliding
    /// with the ball.
    ///
    /// The f32 in the tuple contains a value between 0 & 1 indicating at what
    /// poisition of the paddle the ball collided with. A `0` value indicates
    /// that the ball collided exactly at the players "start angle". A `1` value
    /// indicates that the ball collided exactly at the players "end angle".
    pub fn player_collision(
        &mut self,
        ball_pos: Vec2,
        ball_radius: f32,
    ) -> Option<(PlayerId, f32)> {
        // Plot multiple points around the ball and check collision with them.
        // This lets us detect collisions on the whole ball instead of just its
        // center points.
        let ball_points = create_circle_points(ball_radius, ball_pos, 0.0, TAU, 8);
        for ball_point in ball_points {
            let collision = self.player_collision_priv(ball_point.into());
            if collision.is_some() {
                if self.was_collision_prev_tick {
                    return None;
                } else {
                    self.was_collision_prev_tick = true;
                    return collision;
                }
            }
        }

        self.was_collision_prev_tick = false;
        None
    }

    fn player_collision_priv(&mut self, ball_point: Vec2) -> Option<(PlayerId, f32)> {
        let ball_len_from_origin = ball_point.length();
        if ball_len_from_origin.is_nan() || ball_len_from_origin < self.player_radius_inner {
            return None;
        }

        let (_, ball_angle) = util::cartesian_to_polar(ball_point);
        let goal_length_angle = util::angle_per_player(self.amount_of_players);
        let player_length_angle = goal_length_angle * self.player_length_frac;

        for (player_id, player_pos) in &mut self.player_colliders {
            let (_, player_middle_angle) = util::cartesian_to_polar(*player_pos);
            let player_start_angle = player_middle_angle - player_length_angle / 2.0;

            if util::is_between_angles(ball_angle, player_start_angle, player_length_angle) {
                let hit_dist_angle = util::distance_between_angles(player_start_angle, ball_angle);
                let hit_location = hit_dist_angle / player_length_angle;
                return Some((*player_id, hit_location));
            }
        }

        None
    }

    /// Returns `Some` if the ball is colliding with a goal. The returned PlayerId
    /// represents the player which goal the ball it is touching.
    pub fn goal_collision(&self, ball_pos: Vec2, ball_radius: f32) -> Option<PlayerId> {
        // Plot multiple points around the ball and check collision with them.
        let ball_points = create_circle_points(ball_radius, ball_pos, 0.0, TAU, 8);
        for ball_point in ball_points {
            let collision = self.goal_collision_priv(ball_point.into());
            if collision.is_some() {
                return collision;
            }
        }
        None
    }

    fn goal_collision_priv(&self, ball_point: Vec2) -> Option<PlayerId> {
        let ball_len_from_origin = ball_point.length();
        if ball_len_from_origin.is_nan() || ball_len_from_origin < self.goal_radius_inner {
            return None;
        }

        let (_, angle) = util::cartesian_to_polar(ball_point);
        let angle = util::convert_angle_inv(angle);
        let goal_length_angle = util::angle_per_player(self.amount_of_players);
        let player_idx = (angle / goal_length_angle) as usize;
        self.player_order.get(player_idx).cloned()
    }

    /// Returns the angle at which the goals starts at for player with ID `player_id`.
    fn player_goal_start_angle(&self, player_id: PlayerId) -> f32 {
        if let Some(idx) = self.player_order.iter().position(|id| *id == player_id) {
            let goal_length_angle = util::angle_per_player(self.amount_of_players);
            idx as f32 * goal_length_angle
        } else {
            unreachable!(
                "player_goal_start_angle -- player_id: {}, Colliders: {:#?}",
                player_id, self
            );
        }
    }
}
