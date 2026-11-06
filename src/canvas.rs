use std::collections::VecDeque;
use std::sync::Arc;

use eframe::egui::{Color32, Key, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::geometry::{contains, split_around, touches};
use crate::model::{Brush, Geometry, Line, Tool};
use crate::render::Pen;
use crate::smooth::Smoother;

/// Maximum number of undo steps kept in memory.
const HISTORY_LIMIT: usize = 100;
/// Fraction of the remaining distance to the pointer kept per frame at 60 fps (eraser).
/// Smoothing is time-based, so it feels the same at any refresh rate.
const SMOOTHING: f32 = 0.85;
/// Eraser step, as a fraction of the eraser radius. Smaller = no gaps, slower.
const ERASE_STEP: f32 = 0.25;

/// Strokes are shared with `Arc`, so a history snapshot only copies pointers.
type Lines = Vec<Arc<Line>>;

/// Holds the finished strokes, the stroke being drawn, and the undo/redo history.
pub struct Canvas {
    lines: Lines,
    current: Option<Line>,
    /// Builds the stroke being drawn from the raw pointer positions.
    smoother: Option<Smoother>,
    undo: VecDeque<Lines>,
    redo: Vec<Lines>,
    /// Smoothed pointer position used while erasing.
    smooth_pos: Option<Pos2>,
    /// Last erased position during a drag, so fast drags leave no gaps.
    last_erase: Option<Pos2>,
    /// True once the current eraser gesture has modified the drawing, so the
    /// gesture creates at most one undo step.
    eraser_changed: bool,
    /// Area of the drawing area on screen, used when exporting.
    rect: Option<Rect>,
    zoom: f32,
    pan: Vec2,
    pan_active: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            current: None,
            smoother: None,
            undo: VecDeque::new(),
            redo: Vec::new(),
            smooth_pos: None,
            last_erase: None,
            eraser_changed: false,
            rect: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            pan_active: false,
        }
    }
}

impl Canvas {
    /// Goes back one step.
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop_back() {
            let current = std::mem::replace(&mut self.lines, previous);
            self.redo.push(current);
            self.eraser_changed = false;
        }
    }

    /// Goes forward one step, after an undo.
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let current = std::mem::replace(&mut self.lines, next);
            self.undo.push_back(current);
            self.eraser_changed = false;
        }
    }

    /// Removes everything. This cannot be undone.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.reset_gesture();
        self.undo.clear();
        self.redo.clear();
    }

    pub fn replace_lines(&mut self, lines: Lines) {
        self.lines = lines;
        self.reset_gesture();
        self.undo.clear();
        self.redo.clear();
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.pan_active = false;
    }

    /// Forgets the stroke and the eraser gesture in progress.
    fn reset_gesture(&mut self) {
        self.current = None;
        self.smoother = None;
        self.smooth_pos = None;
        self.last_erase = None;
        self.eraser_changed = false;
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// All finished strokes, for export.
    pub fn lines(&self) -> &[Arc<Line>] {
        &self.lines
    }

    /// Area of the drawing area on screen, once it has been drawn.
    pub fn rect(&self) -> Option<Rect> {
        self.rect
    }

    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.pan_active = false;
    }

    /// Handles mouse input on the drawing area and paints all strokes.
    /// `ring` is the color of the eraser outline.
    pub fn show(&mut self, ui: &mut Ui, brush: &mut Brush, ring: Color32) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        self.rect = Some(response.rect);
        self.handle_view(&response, ui);
        let radius = brush.width / 2.0;
        let erasing = brush.tool == Tool::Eraser;

        if self.is_panning(ui) {
            self.current = None;
            self.smoother = None;
        } else if erasing {
            // Frame duration, capped so a stalled frame does not make the eraser jump.
            let dt = ui.input(|i| i.stable_dt).clamp(1.0 / 1000.0, 0.1);
            self.handle_eraser(&response, radius, dt);
        } else if matches!(brush.tool, Tool::Line | Tool::Rectangle | Tool::Ellipse) {
            self.handle_shape(&response, brush);
        } else if brush.tool == Tool::Eyedropper {
            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(color) = self.sample_color(self.to_world(pos, response.rect)) {
                        brush.color = color;
                        brush.tool = Tool::Brush;
                    }
                }
            }
        } else {
            self.handle_brush(&response, brush.color, brush.width);
        }

        // Only strokes that touch the visible area are drawn.
        let clip = painter.clip_rect();
        let mut pen = Pen::new(&painter, response.rect.min + self.pan, self.zoom);
        for line in &self.lines {
            if self.screen_bounds(line, response.rect).intersects(clip) {
                pen.draw(line);
            }
        }
        if let Some(line) = &self.current {
            pen.draw_fresh(line);
        }
        pen.finish();

        // Outline of the eraser under the pointer.
        if erasing {
            if let Some(pos) = response.hover_pos() {
                painter.circle_stroke(pos, radius * self.zoom, Stroke::new(1.0_f32, ring));
            }
        }
    }

    fn handle_view(&mut self, response: &Response, ui: &Ui) {
        let (zoom_delta, pointer, scroll) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.pointer.hover_pos(),
                i.smooth_scroll_delta,
            )
        });
        if response.hovered() && zoom_delta != 0.0 {
            if let Some(pointer) = pointer {
                let world = self.to_world(pointer, response.rect);
                self.zoom = (self.zoom * (zoom_delta * 0.002).exp()).clamp(0.1, 16.0);
                self.pan = pointer - response.rect.min - world.to_vec2() * self.zoom;
                ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }
        } else if response.hovered() && scroll != Vec2::ZERO {
            ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }

        let (middle_down, primary_down, space_down, pointer_delta) = ui.input(|i| {
            (
                i.pointer.button_down(PointerButton::Middle),
                i.pointer.button_down(PointerButton::Primary),
                i.key_down(Key::Space),
                i.pointer.delta(),
            )
        });
        let pan_input = middle_down || (space_down && primary_down);
        if response.hovered() && pan_input {
            self.pan_active = true;
        }
        if self.pan_active {
            self.pan += pointer_delta;
        }
        if !pan_input {
            self.pan_active = false;
        }
    }

    fn is_panning(&self, ui: &Ui) -> bool {
        self.pan_active
            || ui.input(|i| {
                i.pointer.button_down(PointerButton::Middle)
                    || (i.key_down(Key::Space) && i.pointer.button_down(PointerButton::Primary))
            })
    }

    fn to_world(&self, screen: Pos2, rect: Rect) -> Pos2 {
        Pos2::new(
            (screen.x - rect.min.x - self.pan.x) / self.zoom,
            (screen.y - rect.min.y - self.pan.y) / self.zoom,
        )
    }

    fn screen_bounds(&self, line: &Line, rect: Rect) -> Rect {
        let min = rect.min + self.pan + line.bounds.min.to_vec2() * self.zoom;
        let max = rect.min + self.pan + line.bounds.max.to_vec2() * self.zoom;
        Rect::from_min_max(min, max)
    }

    /// Moves the smoothed eraser position toward the pointer and returns it.
    /// `dt` is the frame duration in seconds; the factor is normalized to 60 fps.
    fn smooth(&mut self, target: Pos2, dt: f32) -> Pos2 {
        let kept = SMOOTHING.powf(dt * 60.0);
        let next = match self.smooth_pos {
            Some(prev) => prev + (target - prev) * (1.0 - kept),
            None => target,
        };
        self.smooth_pos = Some(next);
        next
    }

    /// Saves the current state so it can be restored with undo.
    fn checkpoint(&mut self) {
        self.undo.push_back(self.lines.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    /// Adds a finished stroke, as one undoable step.
    fn commit(&mut self, line: Line) {
        self.checkpoint();
        self.lines.push(Arc::new(line));
    }

    /// Brush: the stroke follows the pointer while dragging, or a single dot is placed on click.
    fn handle_brush(&mut self, response: &Response, color: Color32, width: f32) {
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let p = self.to_world(pos, response.rect);
                self.smoother = Some(Smoother::new(p, brush_step(width)));
                self.current = Some(Line::new(color, width));
            }
        }

        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let p = self.to_world(pos, response.rect);
                if let Some(smoother) = self.smoother.as_mut() {
                    smoother.add(p);
                }
            }
            if let (Some(smoother), Some(line)) = (&self.smoother, self.current.as_mut()) {
                smoother.write_to(&mut line.points);
                line.finish();
            }
        }

        if response.drag_stopped() {
            if let Some(mut line) = self.current.take() {
                let end = response
                    .interact_pointer_pos()
                    .map(|pos| self.to_world(pos, response.rect))
                    .or_else(|| line.points.last().copied());
                if let (Some(end), Some(smoother)) = (end, self.smoother.take()) {
                    line.points = smoother.finish(end);
                    line.finish();
                    self.commit(line);
                }
            }
            self.smoother = None;
        }

        // Single click without dragging: place one dot.
        if response.clicked() && self.current.is_none() {
            if let Some(pos) = response.interact_pointer_pos() {
                let dot = Line::build(
                    Geometry::Freehand,
                    color,
                    width,
                    false,
                    vec![self.to_world(pos, response.rect)],
                );
                self.commit(dot);
            }
        }
    }

    fn handle_shape(&mut self, response: &Response, brush: &Brush) {
        let geometry = match brush.tool {
            Tool::Line => Geometry::Line,
            Tool::Rectangle => Geometry::Rectangle,
            Tool::Ellipse => Geometry::Ellipse,
            _ => return,
        };
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                let point = self.to_world(pos, response.rect);
                self.current = Some(Line::shape(
                    geometry,
                    brush.color,
                    brush.width,
                    brush.fill_shapes,
                    point,
                    point,
                ));
            }
        }
        if response.dragged() {
            let point = response
                .interact_pointer_pos()
                .map(|pos| self.to_world(pos, response.rect));
            if let (Some(point), Some(line)) = (point, self.current.as_mut()) {
                line.points[1] = point;
                line.finish();
            }
        }
        if response.drag_stopped() {
            if let (Some(pos), Some(mut line)) =
                (response.interact_pointer_pos(), self.current.take())
            {
                line.points[1] = self.to_world(pos, response.rect);
                line.finish();
                if line.points[0].distance(line.points[1]) > 1.0 {
                    self.commit(line);
                }
            }
        }
    }

    /// Eraser: removes the parts of existing strokes under the pointer, with smoothing.
    fn handle_eraser(&mut self, response: &Response, radius: f32, dt: f32) {
        if response.drag_started() || response.clicked() {
            self.last_erase = None;
            self.smooth_pos = response
                .interact_pointer_pos()
                .map(|p| self.to_world(p, response.rect));
            self.eraser_changed = false;
        }

        if response.dragged() {
            if let Some(target) = response.interact_pointer_pos() {
                let p = self.smooth(self.to_world(target, response.rect), dt);
                let from = self.last_erase.unwrap_or(p);
                self.erase_between(from, p, radius);
                self.last_erase = Some(p);
            }
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.erase_at(self.to_world(pos, response.rect), radius);
            }
        }

        if response.drag_stopped() {
            // Catch up to the actual release point: the smoothed point lags behind it.
            if let Some(pos) = response.interact_pointer_pos() {
                let pos = self.to_world(pos, response.rect);
                let from = self.last_erase.unwrap_or(pos);
                self.erase_between(from, pos, radius);
            }
            self.last_erase = None;
            self.smooth_pos = None;
            self.eraser_changed = false;
        }
    }

    fn sample_color(&self, point: Pos2) -> Option<Color32> {
        self.lines
            .iter()
            .rev()
            .find(|line| contains(line, point, line.width * 0.5))
            .map(|line| line.color)
    }

    /// Erases along the segment from `from` to `to`, in small steps.
    fn erase_between(&mut self, from: Pos2, to: Pos2, radius: f32) {
        let step = (radius * ERASE_STEP).max(0.5);
        let steps = (from.distance(to) / step).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.erase_at(from + (to - from) * t, radius);
        }
    }

    /// Erases everything within `radius` of `p`. Strokes are cut exactly where the
    /// eraser crosses them, even between two distant points of a fast stroke.
    fn erase_at(&mut self, p: Pos2, radius: f32) {
        // Cheap test first: most calls touch nothing, and then nothing is allocated.
        let Some(first) = self.lines.iter().position(|l| touches(l, p, radius)) else {
            return;
        };

        // Save the state before the first actual change of this gesture, so a
        // click or drag in empty space adds no undo step and keeps redo.
        if !self.eraser_changed {
            self.checkpoint();
            self.eraser_changed = true;
        }

        let mut next: Lines = Vec::with_capacity(self.lines.len() + 2);
        next.extend(self.lines[..first].iter().cloned());
        for line in &self.lines[first..] {
            if touches(line, p, radius) {
                if line.geometry != Geometry::Freehand {
                    continue;
                }
                let reach = radius + line.width * 0.5;
                next.extend(split_around(line, p, reach).into_iter().map(Arc::new));
            } else {
                next.push(line.clone());
            }
        }
        self.lines = next;
    }
}

/// Smallest move of the pointer, in drawing units, that changes a brush stroke.
fn brush_step(width: f32) -> f32 {
    (width * 0.05).max(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn dot(at: Pos2) -> Line {
        Line::build(Geometry::Freehand, Color32::BLACK, 4.0, false, vec![at])
    }

    fn segment(a: Pos2, b: Pos2) -> Line {
        Line::from_points(Color32::BLACK, 4.0, vec![a, b])
    }

    fn commit(canvas: &mut Canvas, line: Line) {
        canvas.commit(line);
    }

    #[test]
    fn erasing_empty_space_keeps_redo() {
        let mut canvas = Canvas::default();
        commit(&mut canvas, dot(pos2(10.0, 10.0)));
        canvas.undo();
        assert_eq!(canvas.redo.len(), 1);

        canvas.erase_at(pos2(500.0, 500.0), 10.0);

        assert!(canvas.undo.is_empty());
        assert_eq!(canvas.redo.len(), 1);
    }

    #[test]
    fn erasing_a_stroke_adds_one_undo_step() {
        let mut canvas = Canvas::default();
        commit(&mut canvas, dot(pos2(10.0, 10.0)));
        let steps = canvas.undo.len();

        canvas.erase_at(pos2(10.0, 10.0), 5.0);
        canvas.erase_at(pos2(10.0, 10.0), 5.0);

        assert!(canvas.is_empty());
        assert_eq!(canvas.undo.len(), steps + 1);
    }

    #[test]
    fn smoothing_is_frame_rate_independent() {
        fn advance(frames: u32, dt: f32) -> f32 {
            let mut canvas = Canvas {
                smooth_pos: Some(pos2(0.0, 0.0)),
                ..Canvas::default()
            };
            for _ in 0..frames {
                canvas.smooth(pos2(100.0, 0.0), dt);
            }
            canvas.smooth_pos.unwrap().x
        }

        // Same duration (0.1 s) at 30 fps and at 120 fps.
        let slow = advance(3, 1.0 / 30.0);
        let fast = advance(12, 1.0 / 120.0);
        assert!((slow - fast).abs() < 0.01, "{slow} vs {fast}");
    }

    #[test]
    fn eraser_cuts_a_stroke_between_two_distant_points() {
        let mut canvas = Canvas::default();
        // Only two points, 100 px apart: no point is near the eraser.
        commit(&mut canvas, segment(pos2(0.0, 0.0), pos2(100.0, 0.0)));

        canvas.erase_at(pos2(50.0, 0.0), 5.0);

        // Reach is 5 + 2 = 7, so the stroke is cut between x = 43 and x = 57.
        assert_eq!(canvas.lines().len(), 2);
        let left_end = canvas.lines()[0].points.last().unwrap().x;
        let right_start = canvas.lines()[1].points[0].x;
        assert!((left_end - 43.0).abs() < 0.01, "{left_end}");
        assert!((right_start - 57.0).abs() < 0.01, "{right_start}");
    }

    #[test]
    fn eraser_near_but_outside_leaves_the_stroke_alone() {
        let mut canvas = Canvas::default();
        commit(&mut canvas, segment(pos2(0.0, 0.0), pos2(100.0, 0.0)));

        canvas.erase_at(pos2(50.0, 30.0), 5.0);

        assert_eq!(canvas.lines().len(), 1);
        assert_eq!(canvas.undo.len(), 1);
    }

    #[test]
    fn eraser_removes_the_end_of_a_stroke() {
        let mut canvas = Canvas::default();
        commit(&mut canvas, segment(pos2(0.0, 0.0), pos2(100.0, 0.0)));

        canvas.erase_at(pos2(100.0, 0.0), 5.0);

        assert_eq!(canvas.lines().len(), 1);
        let end = canvas.lines()[0].points.last().unwrap().x;
        assert!((end - 93.0).abs() < 0.01, "{end}");
    }

    #[test]
    fn history_is_limited() {
        let mut canvas = Canvas::default();
        for i in 0..(HISTORY_LIMIT + 20) {
            commit(&mut canvas, dot(pos2(i as f32 * 10.0, 0.0)));
        }
        assert_eq!(canvas.undo.len(), HISTORY_LIMIT);
    }

    #[test]
    fn undo_then_redo_restores_the_stroke() {
        let mut canvas = Canvas::default();
        commit(&mut canvas, dot(pos2(1.0, 1.0)));
        canvas.undo();
        assert!(canvas.is_empty());
        canvas.redo();
        assert_eq!(canvas.lines().len(), 1);
    }
}
