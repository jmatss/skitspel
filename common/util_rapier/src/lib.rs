pub use draw::{
    create_circle_points, create_path_with_thickness, create_polygon_points,
    create_polygon_points_with_angle, indices_from_vertices, vertices_with_thickness,
};
pub use player::{move_players, spawn_player, spawn_player_with_vertex_amount};
pub use wall::spawn_border_walls;

mod draw;
mod player;
mod wall;
