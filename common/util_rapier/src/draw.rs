use bevy::{
    math::Vec2,
    prelude::{Color, Mesh},
    render::{mesh::Indices, pipeline::PrimitiveTopology},
};
use bevy_rapier2d::{
    na::Point2,
    physics::ColliderBundle,
    prelude::{
        ActiveEvents, ColliderFlags, ColliderMassProps, ColliderMaterial, ColliderShape,
        ColliderType, Real,
    },
};
use skitspel::RAPIER_SCALE_FACTOR;

/// `pos` is the absolute position of the center point of the circle.
/// The `start_angle` & `end_angle` should be between 0 and 2*PI. The start angle
/// must be less than end_angle.
///
/// The functions returns `amount_of_points` points plotted around the circle
/// with its center at position `pos`. Only points found between the angles
/// `start_angle` & `end_angle` will be returned.
pub fn create_circle_points(
    radius: f32,
    pos: Vec2,
    start_angle: f32,
    end_angle: f32,
    amount_of_points: usize,
) -> Vec<Point2<Real>> {
    use std::f32::consts::PI;
    assert!(radius > 0.0, "Radius must be greater than 0.");
    assert!(
        amount_of_points > 2,
        "amount_of_points must be greater than 2."
    );
    assert!((0.0..=2.0 * PI).contains(&start_angle));
    assert!((0.0..=2.0 * PI).contains(&end_angle));

    let mut points = Vec::with_capacity(amount_of_points);
    let step = (end_angle - start_angle) / ((amount_of_points - 1) as f32);
    for i in 0..amount_of_points {
        let cur_angle = start_angle + i as f32 * step;
        let x = pos.x + radius * cur_angle.cos();
        let y = pos.y + radius * cur_angle.sin();
        points.push([x, y].into());
    }
    points
}

/// This logic is copy-pasted from:
///   https://github.com/Nilirad/bevy_prototype_lyon/blob/79cdb49888bda1455cf1ed5fee6aa3d5a955385f/src/shapes.rs#L204-L234
///
/// This function can be used to create a similar shaped polygon that can be
/// used in rapier as ex. a collider.
pub fn create_polygon_points(sides: usize, radius: f32, center: Vec2) -> Vec<Point2<Real>> {
    use std::f32::consts::PI;
    assert!(sides > 2, "Polygons must have at least 3 sides");
    let n = sides as f32;
    let internal = (n - 2.0) * PI / n;
    let offset = -internal / 2.0;

    let mut points = Vec::with_capacity(sides);
    let step = 2.0 * PI / n;
    for i in 0..sides {
        let cur_angle = (i as f32).mul_add(step, offset);
        let x = radius.mul_add(cur_angle.cos(), center.x);
        let y = radius.mul_add(cur_angle.sin(), center.y);
        points.push([x, y].into());
    }
    points
}

/// Creates a path following the vertices given in `vertices` with thickness
/// `line_thickness` and color `color`. The path will have a collider which means
/// that the physics object will hit it.
///
/// If `closed` is set to true, a path will be drawn between the start and end
/// vertices.
pub fn create_path_with_thickness(
    vertices: &[Vec2],
    color: Color,
    line_thickness: f32,
    collider_type: ColliderType,
    active_events: ActiveEvents,
    closed: bool,
) -> (Mesh, Vec<ColliderBundle>) {
    let new_vertices = vertices_with_thickness(vertices, line_thickness, closed);
    let indices = indices_from_vertices(&new_vertices);
    let colors = vec![[color.r(), color.g(), color.b(), color.a()]; new_vertices.len()];

    let mut colliders = Vec::with_capacity(indices.len() / 3);
    for window in indices.windows(3).step_by(3) {
        let (a, b, c) = if let [i_a, i_b, i_c] = *window {
            (
                *new_vertices.get(i_a as usize).unwrap(),
                *new_vertices.get(i_b as usize).unwrap(),
                *new_vertices.get(i_c as usize).unwrap(),
            )
        } else {
            unreachable!("{:#?}", window);
        };

        colliders.push(ColliderBundle {
            collider_type,
            flags: ColliderFlags {
                active_events,
                ..Default::default()
            },
            shape: ColliderShape::triangle(
                [a.x / RAPIER_SCALE_FACTOR, a.y / RAPIER_SCALE_FACTOR].into(),
                [b.x / RAPIER_SCALE_FACTOR, b.y / RAPIER_SCALE_FACTOR].into(),
                [c.x / RAPIER_SCALE_FACTOR, c.y / RAPIER_SCALE_FACTOR].into(),
            ),
            material: ColliderMaterial {
                friction: 0.3,
                restitution: 0.5,
                ..Default::default()
            },
            mass_properties: ColliderMassProps::Density(1.0),
            ..Default::default()
        });
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
    mesh.set_attribute(
        Mesh::ATTRIBUTE_POSITION,
        new_vertices.iter().map(|v| [v.x, v.y]).collect::<Vec<_>>(),
    );
    mesh.set_indices(Some(Indices::U32(indices)));
    mesh.set_attribute(Mesh::ATTRIBUTE_COLOR, colors);

    (mesh, colliders)
}

/// Thickness is added by making the lines of two triangles. Two new vertices
/// will be created for every vertex. These new vertices are moved slightly
/// to the left and right (~perpendicularity to previous and next vertices
/// be drawn to) according to the specified thickness.
/// These new vertices will be connected using triangles (two triangles per
/// line segment).
///
/// For example given two vertices `a` and `b`. We would split them up into
/// four vertices: `a1`, `a2`, `b1` & `b2`. The line would then be represented
/// by the two triangles between `a1`, `a2`, `b1` and `b1`, `b2`, `a2`.
fn vertices_with_thickness(vertices: &[Vec2], line_thickness: f32, closed: bool) -> Vec<Vec2> {
    assert!(vertices.len() > 2 || (vertices.len() == 2 && !closed));

    let mut new_vertices: Vec<Vec2> = Vec::with_capacity(vertices.len() * 2);

    let width = line_thickness / 2.0;
    for window in vertices.windows(3) {
        if let [prev, cur, next] = *window {
            new_vertices.extend(&create_new_vertices(prev, cur, next, width));
        } else {
            unreachable!("{:#?}", window);
        }
    }

    // Handle the start and end points. It makes no sense to do `closed`
    // on only two vertices so it will be handled as a non-`closed` path.
    if closed && vertices.len() >= 3 {
        let prev = *vertices.get(vertices.len() - 2).unwrap();
        let cur = *vertices.get(vertices.len() - 1).unwrap();
        let next = *vertices.get(0).unwrap();
        new_vertices.extend(&create_new_vertices(prev, cur, next, width));

        let prev = *vertices.get(vertices.len() - 1).unwrap();
        let cur = *vertices.get(0).unwrap();
        let next = *vertices.get(1).unwrap();
        let new = create_new_vertices(prev, cur, next, width);
        // Inserted both at the start to `close` the path to end up at the start.
        new_vertices.insert(0, new[0]);
        new_vertices.insert(1, new[1]);
        new_vertices.extend(&[new[0], new[1]]);
    } else {
        let first = *vertices.get(0).unwrap();
        let next = *vertices.get(1).unwrap();
        let norm_vec = (next - first).normalize();
        new_vertices.insert(0, first + Vec2::new(-norm_vec.y, norm_vec.x) * width);
        new_vertices.insert(1, first + Vec2::new(norm_vec.y, -norm_vec.x) * width);

        let prev = *vertices.get(vertices.len() - 2).unwrap();
        let last = *vertices.get(vertices.len() - 1).unwrap();
        let norm_vec = (last - prev).normalize();
        new_vertices.push(last + Vec2::new(-norm_vec.y, norm_vec.x) * width);
        new_vertices.push(last + Vec2::new(norm_vec.y, -norm_vec.x) * width);
    }

    new_vertices
}

/// Creates the two new vertices that will represent the old vertex `cur`.
/// The given `width` should be half of the `line_thickness`.
/// https://proofwiki.org/wiki/Angle_Bisector_Vector
fn create_new_vertices(prev: Vec2, cur: Vec2, next: Vec2, width: f32) -> [Vec2; 2] {
    let prev_line = prev - cur;
    let next_line = next - cur;
    let angle_bisector = prev_line.length() * next_line + next_line.length() * prev_line;

    let angle_between = prev_line.angle_between(angle_bisector);
    // The length that is needed for the vector starting at the original `cur`
    // vertex in the direction of `angle_bisctor` to be `width` units perpendicular
    // to the imaginary lines from the `prev` & `next` vertices.
    let len = (width / angle_between.sin()).abs();

    let heading_vec = angle_bisector.normalize();
    [cur + (heading_vec * len), cur - (heading_vec * len)]
}

/// Creates a list of indices that will be used to create triangles for the
/// given `vertices`. The triangles must be constructed counter clockwise so
/// that they are facing forward (requirement by the bevy engine).
fn indices_from_vertices(vertices: &[Vec2]) -> Vec<u32> {
    let mut indices = Vec::default();
    let mut i = 0;
    for window in vertices.windows(4).step_by(2) {
        if let [a1, a2, b1, b2] = *window {
            // We want to draw two triangles between the points `a1`, `a2`,
            // `b1` & `b2`. We therefore need to figure out which points we
            // should draw the triangles from to cover the whole area between
            // the points.
            let (i_a1_or_a2, a1_or_a2) = if intersects(a1, b1, a2, b2) {
                (i, a1)
            } else {
                (i + 1, a2)
            };
            if ccw(a1_or_a2, b1, b2) {
                indices.extend(&[i_a1_or_a2, i + 2, i + 3]);
            } else {
                indices.extend(&[i_a1_or_a2, i + 3, i + 2]);
            }
            if ccw(a1, a2, b1) {
                indices.extend(&[i, i + 1, i + 2]);
            } else {
                indices.extend(&[i, i + 2, i + 1]);
            }
            i += 2;
        } else {
            unreachable!("{:#?}", window);
        }
    }
    indices
}

/// Returns true if the lines `a`-`b` and `c`-`d` intersects.
fn intersects(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    ccw(a, c, d) != ccw(b, c, d) && ccw(a, b, c) != ccw(a, b, d)
}

/// Checks if the three points `a`, `b` & `c` are listed in a counter clock wise
/// order. This doesn't handle colinearity (i.e. cases when all three points
/// are located on the same line), but it will never happen in our use-case.
/// https://bryceboe.com/2006/10/23/line-segment-intersection-algorithm/
fn ccw(a: Vec2, b: Vec2, c: Vec2) -> bool {
    (c.y - a.y) * (b.x - a.x) > (b.y - a.y) * (c.x - a.x)
}
