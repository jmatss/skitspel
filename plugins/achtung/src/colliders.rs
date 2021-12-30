use std::collections::HashSet;

use bevy::{math::Vec2, utils::HashMap};

use skitspel::{GAME_HEIGHT, GAME_WIDTH};

const ZONES_HORIZONTAL: usize = 8;
const ZONES_VERTICAL: usize = 16;

const ZONE_WIDTH: f32 = GAME_WIDTH / ZONES_HORIZONTAL as f32;
const ZONE_HEIGHT: f32 = GAME_HEIGHT / ZONES_VERTICAL as f32;

/// Contains colliders used in the "Achtung Die Kurve" Game.
///
/// The screen is divided into zones. Any collider is only inserted into the zones
/// in which it is located. When we check collision for the players, we know
/// in which the zone the players are and only need to check collisions with the
/// colliders in the same zone.
///
/// For an overview of the algorithm used to find collisions, see:
///   https://stackoverflow.com/a/218081
///
/// We first check if the point is found inside a `Collision`s "bounding box".
/// If that is the case, we check if a horizontal line drawn from the point
/// intersects the lines forming the `Collision` an odd numer of times.
/// If both of these are true, we have found a collision.
pub(crate) struct Colliders {
    /// The key of the map is the zone index. The values are the colliders that
    /// are stored in the specific zone.
    colliders: HashMap<usize, Vec<Collider>>,

    /// Cooliders and coordinates for the border colliders. The assumption is
    /// that the screen is surrounded by a border that is `thickness` wide.
    /// These coordinates represents where these borders starts.
    collider_x_left: f32,
    collider_x_right: f32,
    collider_y_top: f32,
    collider_y_bottom: f32,
}

impl Colliders {
    pub fn new(thickness: f32) -> Self {
        Self {
            colliders: HashMap::default(),
            collider_x_left: -GAME_WIDTH / 2.0 + thickness,
            collider_x_right: GAME_WIDTH / 2.0 - thickness,
            collider_y_top: GAME_HEIGHT / 2.0 - thickness,
            collider_y_bottom: -GAME_HEIGHT / 2.0 + thickness,
        }
    }

    /// Resets and clears the colliders stored inside this struct.
    pub fn reset(&mut self) {
        self.colliders.clear();
    }

    /// Adds the polygon formed by the given `vertices` as a collider. The
    /// `vertices` MUST have a length of 4.
    pub fn add(&mut self, vertices: &[Vec2]) {
        assert!(vertices.len() == 4);
        let collider = Collider::new([vertices[0], vertices[1], vertices[2], vertices[3]]);

        let mut prev_zone_indices = HashSet::with_capacity(4);
        for vertex in collider.points.iter() {
            let zone_idx = self.zone_idx(*vertex);
            if !prev_zone_indices.contains(&zone_idx) {
                prev_zone_indices.insert(zone_idx);
                self.colliders
                    .entry(zone_idx)
                    .or_default()
                    .push(collider.clone());
            }
        }
    }

    /// Returns true if the given point `p` is located inside one of the colliders.
    pub fn is_collision(&self, p: Vec2) -> bool {
        if self.screen_border_collision(p) {
            return true;
        }

        let zone_idx = self.zone_idx(p);
        let zone_colliders = if let Some(colliders) = self.colliders.get(&zone_idx) {
            colliders
        } else {
            // No colliders in this zone, so nothing to collide with.
            return false;
        };

        for collider in zone_colliders {
            if self.bounding_box_collision(p, collider)
                && self.horizontal_line_collision(p, collider)
            {
                return true;
            }
        }
        false
    }

    /// Calculates in which zone the give point `p` is located.
    fn zone_idx(&self, p: Vec2) -> usize {
        let zone_horizontal_idx = (p.x / ZONE_WIDTH) as usize;
        let zone_vertical_idx = (p.y / ZONE_HEIGHT) as usize;
        let zone_idx = zone_horizontal_idx + ZONES_HORIZONTAL * zone_vertical_idx;
        assert!(zone_idx < ZONES_HORIZONTAL * ZONES_VERTICAL);
        zone_idx
    }

    /// Returns true if the point `p` is located outside the screen border.
    fn screen_border_collision(&self, p: Vec2) -> bool {
        p.x < self.collider_x_left
            || p.x > self.collider_x_right
            || p.y > self.collider_y_top
            || p.y < self.collider_y_bottom
    }

    /// Returns true if the given point `p` is inside the "bounding box" of the
    /// collider. This is used as a quick check to see if there is a possiblity
    /// for collision before doing a more thorough check.
    fn bounding_box_collision(&self, p: Vec2, collider: &Collider) -> bool {
        p.x > collider.min_x && p.x < collider.max_x && p.y > collider.min_y && p.y < collider.max_y
    }

    /// Returns true if a horizontal line drawn from the point `p` intersects the
    /// lines forming the `collider` an odd number of times. If this is true,
    /// it indicates that point `p` is located inside the `collider`.
    fn horizontal_line_collision(&self, p: Vec2, collider: &Collider) -> bool {
        let mut intersect_count = 0;
        for i in 0..collider.points.len() {
            let p1 = collider.points[i];
            let p2 = collider.points[(i + 1) % collider.points.len()];
            if p.x > f32::min(p1.x, p2.x)
                && p.x < f32::max(p1.x, p2.x)
                && p.y > f32::min(p1.y, p2.y)
                && p.y < f32::max(p1.y, p2.y)
            {
                intersect_count += 1;
            }
        }
        intersect_count % 2 != 0
    }
}

/// Contains pre-calculated min & max values for the coordinate values found in
/// `points`. This is done so that we don't have to re-calculate the values
/// every time we do a "bounding box" collision check.
#[derive(Clone)]
struct Collider {
    points: [Vec2; 4],
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl Collider {
    pub fn new(points: [Vec2; 4]) -> Self {
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for p in points {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        Self {
            points,
            min_x,
            max_x,
            min_y,
            max_y,
        }
    }
}
