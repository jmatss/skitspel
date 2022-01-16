use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::math::Vec2;

/// Given the polar coordinate of a point, returns it corresponding position
/// in the cartesian coordinate system.
pub fn polar_to_cartesian(radius: f32, angle: f32) -> Vec2 {
    let x = radius * angle.cos();
    let y = radius * angle.sin();
    Vec2::new(x, y)
}

/// Given the polar coordinate of a point, returns it corresponding position
/// in the cartesian coordinate system.
///
/// The first f32 in the returned tuple is the length of the vector and the
/// second f32 is the angle.
pub fn cartesian_to_polar(coord: Vec2) -> (f32, f32) {
    let r = (coord.x.powf(2.0) + coord.y.powf(2.0)).sqrt();
    let angle = coord.y.atan2(coord.x);
    (r, angle)
}

/// Returns the angle in the middle of the goal for the player at index `player_idx`.
///
/// The angles are always calculated `+ (PI / 2.0)`. This is done since Bevy
/// starts the angle 0 to the right and the counts counter-clockwise. We want
/// our angels to start at the top, so we add a quarter of a circle.
pub fn angle_middle(amount_of_players: usize, player_idx: usize) -> f32 {
    angle_start(amount_of_players, player_idx) + angle_per_player(amount_of_players) / 2.0
}

/// Returns the start angle for the player at index `player_idx`.
///
/// The angles are always calculated `+ (PI / 2.0)`. This is done since Bevy
/// starts the angle 0 to the right and the counts counter-clockwise. We want
/// our angels to start at the top, so we add a quarter of a circle.
pub fn angle_start(amount_of_players: usize, player_idx: usize) -> f32 {
    convert_angle(angle_per_player(amount_of_players) * player_idx as f32)
}

/// How "much" angle every player will have. This is the angle for the whole
/// circle divided by the amount of players.
pub fn angle_per_player(amount_of_players: usize) -> f32 {
    TAU / amount_of_players as f32
}

/// The angles are always calculated `+ (PI / 2.0)`. This is done since Bevy
/// starts the angle 0 to the right and the counts counter-clockwise. We want
/// our angels to start at the top, so we add a quarter of a circle.
pub fn convert_angle(angle: f32) -> f32 {
    (angle + FRAC_PI_2).rem_euclid(TAU)
}

/// The angles are always calculated `+ (PI / 2.0)`. This is done since Bevy
/// starts the angle 0 to the right and the counts counter-clockwise. We want
/// our angels to start at the top, so we add a quarter of a circle.
///
/// This function reverts the effect of a call to `convert_angle`. I.e. it
/// caluculates `angle - PI / 2.0`.
pub fn convert_angle_inv(angle: f32) -> f32 {
    (angle - FRAC_PI_2).rem_euclid(TAU)
}

/// Returns true if the given angle `angle` is located somewhere between the
/// angles `start_angle` & `start_angle + angle_amount`. The circle is traversed
/// counter-clockwise.
pub fn is_between_angles(mut angle: f32, mut start_angle: f32, angle_amount: f32) -> bool {
    assert!(angle_amount > 0.0 && angle_amount <= TAU);
    angle = angle.rem_euclid(TAU);
    start_angle = start_angle.rem_euclid(TAU);

    let end_angle = start_angle + angle_amount;
    if start_angle <= angle {
        // True if `end_angle` is greater that `angle`. It does not matter
        // if `end_angle` is greater than TAU since it must cross the `angle`
        // to get to that "length".
        end_angle >= angle
    } else {
        // Since `start_angle` is greater than `angle` at this point, the
        // `end_angle` must "roll-around" modulu to be able to pass the `angle`.
        end_angle >= TAU && end_angle.rem_euclid(TAU) >= angle
    }
}

/// Returns the length between the two angles `to_angle` & `from_angle`.
/// Returns a value between 0 & TAU.
pub fn distance_between_angles(mut from_angle: f32, mut to_angle: f32) -> f32 {
    from_angle = from_angle.rem_euclid(TAU);
    to_angle = to_angle.rem_euclid(TAU);

    if from_angle <= to_angle {
        to_angle - from_angle
    } else {
        // `to_angle` is greater, need to wrap around the circle.
        TAU - (from_angle - to_angle)
    }
}
