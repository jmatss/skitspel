use bevy::math::Vec2;
use bevy_prototype_lyon::{
    prelude::Geometry,
    shapes::{self, RectangleOrigin, RegularPolygonFeature},
};
use lyon_path::path::Builder;

#[derive(Debug, Clone)]
pub enum Shape {
    Circle(shapes::Circle),
    Polygon(shapes::RegularPolygon),
    Rectangle(shapes::Rectangle),
}

impl Geometry for Shape {
    fn add_geometry(&self, b: &mut Builder) {
        match self {
            Shape::Circle(c) => c.add_geometry(b),
            Shape::Polygon(p) => p.add_geometry(b),
            Shape::Rectangle(r) => r.add_geometry(b),
        }
    }
}

impl Shape {
    /// If the amount of sides is less than 3, a circle will be created.
    /// Otherwise a polygon is created with the amount of sides.
    pub fn new(radius: f32, center: Vec2, sides: usize) -> Self {
        if sides < 3 {
            Self::Circle(shapes::Circle { radius, center })
        } else {
            Self::Polygon(shapes::RegularPolygon {
                sides,
                center,
                feature: RegularPolygonFeature::Radius(radius),
            })
        }
    }

    pub fn circle(radius: f32, center: Vec2) -> Self {
        Self::new(radius, center, 0)
    }

    pub fn polygon(radius: f32, center: Vec2, sides: usize) -> Self {
        Self::new(radius, center, sides)
    }

    pub fn square(width: f32, center: Vec2) -> Self {
        Self::rectangle(width, width, center)
    }

    pub fn rectangle(width: f32, height: f32, center: Vec2) -> Self {
        Self::Rectangle(shapes::Rectangle {
            width,
            height,
            origin: RectangleOrigin::CustomCenter(center),
        })
    }
}
