use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

use eframe::egui::{Color32, Pos2, Rect};
use serde::Deserialize;

use crate::render::StrokeMesh;
use crate::theme;

/// The active tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Brush,
    Eraser,
    Line,
    Rectangle,
    Ellipse,
    Eyedropper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geometry {
    Freehand,
    Line,
    Rectangle,
    Ellipse,
}

/// One stroke or shape: its points, color and width.
pub struct Line {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub width: f32,
    pub geometry: Geometry,
    pub filled: bool,
    /// Area covered by the stroke, used to skip drawing when off screen.
    pub bounds: Rect,
    /// Triangles of the stroke, built the first time it is drawn. A finished stroke
    /// never changes, so they are reused by every frame.
    pub(crate) mesh: OnceLock<StrokeMesh>,
}

impl Line {
    /// Creates an empty freehand stroke.
    pub fn new(color: Color32, width: f32) -> Self {
        Self::build(Geometry::Freehand, color, width, false, Vec::new())
    }

    /// Creates a line, rectangle or ellipse between two corners.
    pub fn shape(
        geometry: Geometry,
        color: Color32,
        width: f32,
        filled: bool,
        start: Pos2,
        end: Pos2,
    ) -> Self {
        Self::build(geometry, color, width, filled, vec![start, end])
    }

    /// Creates a freehand stroke from existing points, used when the eraser splits a stroke.
    pub fn from_points(color: Color32, width: f32, points: Vec<Pos2>) -> Self {
        Self::build(Geometry::Freehand, color, width, false, points)
    }

    /// Builds any kind of stroke, whatever it comes from (drawing, eraser, project file).
    pub fn build(
        geometry: Geometry,
        color: Color32,
        width: f32,
        filled: bool,
        points: Vec<Pos2>,
    ) -> Self {
        let mut line = Self {
            points,
            color,
            width,
            geometry,
            filled,
            bounds: Rect::NOTHING,
            mesh: OnceLock::new(),
        };
        line.finish();
        line
    }

    /// Computes the bounding box once the points have changed.
    pub fn finish(&mut self) {
        self.bounds = if self.points.is_empty() {
            Rect::NOTHING
        } else {
            Rect::from_points(&self.points).expand(self.width / 2.0 + 1.0)
        };
        self.mesh = OnceLock::new();
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
                let steps = circle_steps(rx.max(ry));
                let mut points = Vec::with_capacity(steps + 1);
                for step in 0..steps {
                    let angle = TAU * step as f32 / steps as f32;
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

/// Zoom up to which a curve built once still looks smooth.
const REFERENCE_ZOOM: f32 = 4.0;
/// Largest gap between a curve and its straight pieces, in screen pixels.
const CURVE_TOLERANCE: f32 = 0.05;

/// Number of straight pieces that approximate a circle of `radius` world units
/// closely enough to look round up to `REFERENCE_ZOOM`.
pub fn circle_steps(radius: f32) -> usize {
    let radius = (radius * REFERENCE_ZOOM).max(0.5);
    let angle = (1.0 - CURVE_TOLERANCE / radius).clamp(-1.0, 1.0).acos();
    ((PI / angle).ceil() as usize).clamp(24, 720)
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
