use eframe::egui::{Color32, Pos2, Rect};

use crate::theme;

/// The active tool.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Brush,
    Eraser,
}

/// One stroke: a list of points plus its color and width.
pub struct Line {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub width: f32,
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
            bounds: Rect::NOTHING,
        }
    }

    /// Creates a stroke from existing points, used when the eraser splits a stroke.
    pub fn from_points(color: Color32, width: f32, points: Vec<Pos2>) -> Self {
        let mut line = Self {
            points,
            color,
            width,
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
}

/// Current brush settings, edited from the toolbar.
pub struct Brush {
    pub color: Color32,
    pub width: f32,
    pub tool: Tool,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            color: theme::INK_LIGHT,
            width: 6.0,
            tool: Tool::Brush,
        }
    }
}
