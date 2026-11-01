use eframe::egui::{Color32, Pos2, Rect};
use serde::{Deserialize, Serialize};

use crate::theme;

/// The active tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Brush,
    Eraser,
    Line,
    Rectangle,
    Ellipse,
    Eyedropper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geometry {
    Freehand,
    Line,
    Rectangle,
    Ellipse,
}

/// One stroke: a list of points plus its color and width.
pub struct Line {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub width: f32,
    pub geometry: Geometry,
    pub filled: bool,
    /// Area covered by the stroke, used to skip drawing when off screen.
    pub bounds: Rect,
}

impl Line {
    /// Creates an empty stroke.
    pub fn new(color: Color32, width: f32) -> Self {
        Self {
            points: Vec::new(),
            color,
            width,
            geometry: Geometry::Freehand,
            filled: false,
            bounds: Rect::NOTHING,
        }
    }

    pub fn shape(
        geometry: Geometry,
        color: Color32,
        width: f32,
        filled: bool,
        start: Pos2,
        end: Pos2,
    ) -> Self {
        let mut line = Self {
            points: vec![start, end],
            color,
            width,
            geometry,
            filled,
            bounds: Rect::NOTHING,
        };
        line.finish();
        line
    }

    /// Creates a stroke from existing points, used when the eraser splits a stroke.
    pub fn from_points(color: Color32, width: f32, points: Vec<Pos2>) -> Self {
        let mut line = Self {
            points,
            color,
            width,
            geometry: Geometry::Freehand,
            filled: false,
            bounds: Rect::NOTHING,
        };
        line.finish();
        line
    }

    /// Adds a point, skipping points too close to the previous one.
    /// This keeps the number of points low, so long strokes stay fast.
    pub fn push(&mut self, p: Pos2) {
        let min_step = (self.width * 0.1).max(1.0);
        if let Some(last) = self.points.last() {
            if last.distance(p) < min_step {
                return;
            }
        }
        self.points.push(p);
    }

    /// Computes the bounding box once the stroke is complete.
    pub fn finish(&mut self) {
        if !self.points.is_empty() {
            self.bounds = Rect::from_points(&self.points).expand(self.width / 2.0 + 1.0);
        }
    }

    /// Points along the outline of this item, with closed shapes repeating the first point.
    pub fn outline_points(&self) -> Vec<Pos2> {
        match (self.geometry, self.points.as_slice()) {
            (_, []) => Vec::new(),
            (Geometry::Freehand, points) => points.to_vec(),
            (_, [only]) => vec![*only],
            (Geometry::Line, [start, end]) => vec![*start, *end],
            (Geometry::Rectangle, [a, b]) => {
                vec![*a, Pos2::new(b.x, a.y), *b, Pos2::new(a.x, b.y), *a]
            }
            (Geometry::Ellipse, [a, b]) => {
                let center = a.lerp(*b, 0.5);
                let rx = (b.x - a.x).abs() * 0.5;
                let ry = (b.y - a.y).abs() * 0.5;
                let mut points = Vec::with_capacity(65);
                for step in 0..64 {
                    let angle = std::f32::consts::TAU * step as f32 / 64.0;
                    points.push(Pos2::new(
                        center.x + rx * angle.cos(),
                        center.y + ry * angle.sin(),
                    ));
                }
                points.push(points[0]);
                points
            }
            (_, points) => points.to_vec(),
        }
    }
}

/// Current brush settings, edited from the toolbar.
pub struct Brush {
    pub color: Color32,
    pub width: f32,
    pub tool: Tool,
    pub fill_shapes: bool,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            color: theme::INK_LIGHT,
            width: 6.0,
            tool: Tool::Brush,
            fill_shapes: false,
        }
    }
}
