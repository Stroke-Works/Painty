use std::collections::VecDeque;
use std::sync::Arc;

use eframe::egui::{
    Color32, Key, Painter, PointerButton, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2,
};

use crate::model::{Brush, Geometry, Line, Tool};

/// Maximum number of undo steps kept in memory.
const HISTORY_LIMIT: usize = 100;
/// Fraction of the remaining distance to the pointer kept per frame at 60 fps.
/// Smoothing is time-based, so strokes feel the same at any refresh rate.
/// Lower it for a more direct feel, raise it for smoother strokes.
const SMOOTHING: f32 = 0.85;
/// Eraser step, as a fraction of the eraser radius. Smaller = no gaps, slower.
const ERASE_STEP: f32 = 0.25;
/// A bend gets a round disc only when the flat joint would leave a notch deeper
/// than this, in points. Below it the difference cannot be seen.
const JOINT_DEPTH: f32 = 0.15;

/// Strokes are shared with `Arc`, so a history snapshot only copies pointers.
type Lines = Vec<Arc<Line>>;

/// Holds the finished strokes, the stroke being drawn, and the undo/redo history.
pub struct Canvas {
    lines: Lines,
    current: Option<Line>,
    undo: VecDeque<Lines>,
    redo: Vec<Lines>,
    /// Smoothed pointer position used while drawing or erasing.
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
        self.current = None;
        self.undo.clear();
        self.redo.clear();
        self.smooth_pos = None;
        self.last_erase = None;
        self.eraser_changed = false;
    }

    pub fn replace_lines(&mut self, lines: Vec<Arc<Line>>) {
        self.lines = lines;
        self.current = None;
        self.undo.clear();
        self.redo.clear();
        self.smooth_pos = None;
        self.last_erase = None;
        self.eraser_changed = false;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.pan_active = false;
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
        // Frame duration, capped so a stalled frame does not make the stroke jump.
        let dt = ui.input(|i| i.stable_dt).clamp(1.0 / 1000.0, 0.1);

        if self.is_panning(ui) {
            self.current = None;
            self.smooth_pos = None;
        } else if erasing {
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
            self.handle_brush(&response, brush.color, brush.width, dt);
        }

        // Only strokes that touch the visible area are drawn.
        let clip = painter.clip_rect();
        for line in &self.lines {
            if self.screen_bounds(line, response.rect).intersects(clip) {
                draw_line(&painter, line, response.rect.min + self.pan, self.zoom);
            }
        }
        if let Some(line) = &self.current {
            draw_line(&painter, line, response.rect.min + self.pan, self.zoom);
        }

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
        let panning = self.pan_active;
        if panning {
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

    /// Moves the smoothed position toward the pointer and returns it.
    /// `dt` is the frame duration in seconds; the factor is normalized to 60 fps
    /// so the stroke behaves the same at any refresh rate.
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

    /// Brush: draws a smooth stroke while dragging, or a single dot on click.
    fn handle_brush(&mut self, response: &Response, color: Color32, width: f32, dt: f32) {
        if response.drag_started() {
            self.current = Some(Line::new(color, width));
            self.smooth_pos = response
                .interact_pointer_pos()
                .map(|p| self.to_world(p, response.rect));
        }

        if response.dragged() {
            if let Some(target) = response.interact_pointer_pos() {
                let p = self.smooth(self.to_world(target, response.rect), dt);
                if let Some(line) = self.current.as_mut() {
                    line.push(p);
                }
            }
        }

        if response.drag_stopped() {
            if let Some(mut line) = self.current.take() {
                // Finish at the actual pointer, not the lagging smoothed point.
                if let Some(end) = response.interact_pointer_pos() {
                    line.push(self.to_world(end, response.rect));
                }
                if !line.points.is_empty() {
                    line.finish();
                    self.commit(line);
                }
            }
            self.smooth_pos = None;
        }

        // Single click without dragging: place one point.
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let mut line = Line::new(color, width);
                line.push(self.to_world(pos, response.rect));
                line.finish();
                self.commit(line);
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
            .find(|line| item_contains(line, point, line.width * 0.5))
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

/// True when the eraser disc of the given `radius` at `p` overlaps `line`.
fn touches(line: &Line, p: Pos2, radius: f32) -> bool {
    let reach = radius + line.width * 0.5;
    if !line.bounds.expand(reach).contains(p) {
        return false;
    }
    if line.geometry != Geometry::Freehand {
        return item_contains(line, p, reach);
    }
    match line.points.as_slice() {
        [] => false,
        [only] => only.distance(p) <= reach,
        pts => pts
            .windows(2)
            .any(|w| disc_interval(w[0], w[1], p, reach).is_some()),
    }
}

fn item_contains(line: &Line, point: Pos2, reach: f32) -> bool {
    let Some((a, b)) = line.points.as_slice().first().zip(line.points.last()) else {
        return false;
    };
    if line.filled {
        match line.geometry {
            Geometry::Rectangle => return Rect::from_two_pos(*a, *b).contains(point),
            Geometry::Ellipse => {
                let center = a.lerp(*b, 0.5);
                let rx = (b.x - a.x).abs() * 0.5;
                let ry = (b.y - a.y).abs() * 0.5;
                if rx > 0.0 && ry > 0.0 {
                    let x = (point.x - center.x) / rx;
                    let y = (point.y - center.y) / ry;
                    if x * x + y * y <= 1.0 {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    let points = line.outline_points();
    points
        .windows(2)
        .any(|pair| distance_to_segment(point, pair[0], pair[1]) <= reach)
}

fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length_sq = ab.length_sq();
    if length_sq <= f32::EPSILON {
        point.distance(a)
    } else {
        point.distance(a + ab * ((point - a).dot(ab) / length_sq).clamp(0.0, 1.0))
    }
}

/// Cuts `line` where it crosses the disc of radius `reach` centred on `p`.
/// Returns the pieces that remain, in order (possibly none).
fn split_around(line: &Line, p: Pos2, reach: f32) -> Vec<Line> {
    let mut runs: Vec<Vec<Pos2>> = Vec::new();

    match line.points.as_slice() {
        [] => {}
        [only] => {
            if only.distance(p) > reach {
                runs.push(vec![*only]);
            }
        }
        pts => {
            let mut run: Vec<Pos2> = Vec::new();
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                match disc_interval(a, b, p, reach) {
                    // Segment untouched: it joins the current run.
                    None => {
                        if run.is_empty() {
                            run.push(a);
                        }
                        run.push(b);
                    }
                    // Segment crosses the disc between t0 and t1: keep what is outside.
                    Some((t0, t1)) => {
                        if t0 > 0.0 {
                            if run.is_empty() {
                                run.push(a);
                            }
                            run.push(a.lerp(b, t0));
                        }
                        if !run.is_empty() {
                            runs.push(std::mem::take(&mut run));
                        }
                        if t1 < 1.0 {
                            run.push(a.lerp(b, t1));
                            run.push(b);
                        }
                    }
                }
            }
            if !run.is_empty() {
                runs.push(run);
            }
        }
    }

    runs.into_iter()
        .map(|pts| Line::from_points(line.color, line.width, pts))
        .collect()
}

/// Part of the segment `a`-`b` inside the disc of radius `r` centred on `center`,
/// as parameters from 0.0 (at `a`) to 1.0 (at `b`). `None` if it never enters it.
fn disc_interval(a: Pos2, b: Pos2, center: Pos2, r: f32) -> Option<(f32, f32)> {
    let d = b - a;
    let f = a - center;
    let qa = d.dot(d);
    let qc = f.dot(f) - r * r;

    // Degenerate segment: a single point.
    if qa <= f32::EPSILON {
        return (qc <= 0.0).then_some((0.0, 1.0));
    }

    let qb = 2.0 * f.dot(d);
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let t0 = (-qb - root) / (2.0 * qa);
    let t1 = (-qb + root) / (2.0 * qa);
    if t1 < 0.0 || t0 > 1.0 {
        return None;
    }
    Some((t0.max(0.0), t1.min(1.0)))
}

/// True when the bend at `p` is sharp enough for a flat joint to show a notch.
/// The notch depth is `radius * (1 - cos(angle / 2))`, `angle` being the turn.
fn needs_round_joint(prev: Pos2, p: Pos2, next: Pos2, radius: f32) -> bool {
    let a = p - prev;
    let b = next - p;
    let (la, lb) = (a.length(), b.length());
    if la <= f32::EPSILON || lb <= f32::EPSILON {
        return false;
    }
    let cos = (a.dot(b) / (la * lb)).clamp(-1.0, 1.0);
    radius * (1.0 - ((1.0 + cos) * 0.5).sqrt()) > JOINT_DEPTH
}

fn draw_line(painter: &Painter, line: &Line, origin: Pos2, zoom: f32) {
    let color = line.color;
    let width = line.width * zoom;
    let stroke = Stroke::new(width, color);
    match (line.geometry, line.points.as_slice()) {
        (_, []) => {}
        (Geometry::Rectangle, [a, b]) => {
            let a = origin + a.to_vec2() * zoom;
            let b = origin + b.to_vec2() * zoom;
            let rect = Rect::from_two_pos(a, b);
            if line.filled {
                painter.rect_filled(rect, 0.0, color);
            }
            painter.rect_stroke(rect, 0.0, stroke);
        }
        (Geometry::Ellipse, [a, b]) => {
            let a = origin + a.to_vec2() * zoom;
            let b = origin + b.to_vec2() * zoom;
            let rect = Rect::from_two_pos(a, b);
            let center = rect.center();
            let radius = rect.size() * 0.5;
            if line.filled {
                painter.add(Shape::ellipse_filled(center, radius, color));
            }
            painter.add(Shape::ellipse_stroke(center, radius, stroke));
        }
        _ => {
            let points = line
                .outline_points()
                .into_iter()
                .map(|point| origin + point.to_vec2() * zoom)
                .collect::<Vec<_>>();
            if points.len() == 1 {
                painter.circle_filled(points[0], width * 0.5, color);
            } else {
                painter.add(connected_stroke(&points, width, color));
                if line.geometry == Geometry::Freehand || line.geometry == Geometry::Line {
                    painter.circle_filled(points[0], width * 0.5, color);
                    painter.circle_filled(*points.last().unwrap(), width * 0.5, color);
                }
                for w in points.windows(3) {
                    if needs_round_joint(w[0], w[1], w[2], width * 0.5) {
                        painter.circle_filled(w[1], width * 0.5, color);
                    }
                }
            }
        }
    }
}

fn connected_stroke(points: &[Pos2], width: f32, color: Color32) -> Shape {
    Shape::line(points.to_vec(), Stroke::new(width, color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn dot(at: Pos2) -> Line {
        let mut line = Line::new(Color32::BLACK, 4.0);
        line.push(at);
        line.finish();
        line
    }

    fn segment(a: Pos2, b: Pos2) -> Line {
        Line::from_points(Color32::BLACK, 4.0, vec![a, b])
    }

    #[test]
    fn stroke_is_rendered_as_one_connected_path() {
        let line = Line::from_points(
            Color32::BLACK,
            4.0,
            vec![pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0)],
        );

        match connected_stroke(&line.points, line.width, line.color) {
            Shape::Path(path) => assert_eq!(path.points, line.points),
            _ => panic!("a stroke must be tessellated as a connected path"),
        }
    }

    #[test]
    fn filled_shape_is_sampled_inside_its_bounds_and_erasable() {
        let shape = Line::shape(
            Geometry::Rectangle,
            Color32::RED,
            4.0,
            true,
            pos2(10.0, 10.0),
            pos2(40.0, 40.0),
        );
        assert!(item_contains(&shape, pos2(20.0, 20.0), 0.0));
        assert!(touches(&shape, pos2(20.0, 20.0), 2.0));
    }

    #[test]
    fn erasing_empty_space_keeps_redo() {
        let mut canvas = Canvas::default();
        canvas.commit(dot(pos2(10.0, 10.0)));
        canvas.undo();
        assert_eq!(canvas.redo.len(), 1);

        canvas.erase_at(pos2(500.0, 500.0), 10.0);

        assert!(canvas.undo.is_empty());
        assert_eq!(canvas.redo.len(), 1);
    }

    #[test]
    fn erasing_a_stroke_adds_one_undo_step() {
        let mut canvas = Canvas::default();
        canvas.commit(dot(pos2(10.0, 10.0)));
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
        canvas.commit(segment(pos2(0.0, 0.0), pos2(100.0, 0.0)));

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
        canvas.commit(segment(pos2(0.0, 0.0), pos2(100.0, 0.0)));

        canvas.erase_at(pos2(50.0, 30.0), 5.0);

        assert_eq!(canvas.lines().len(), 1);
        assert_eq!(canvas.undo.len(), 1);
    }

    #[test]
    fn eraser_removes_the_end_of_a_stroke() {
        let mut canvas = Canvas::default();
        canvas.commit(segment(pos2(0.0, 0.0), pos2(100.0, 0.0)));

        canvas.erase_at(pos2(100.0, 0.0), 5.0);

        assert_eq!(canvas.lines().len(), 1);
        let end = canvas.lines()[0].points.last().unwrap().x;
        assert!((end - 93.0).abs() < 0.01, "{end}");
    }

    #[test]
    fn history_is_limited() {
        let mut canvas = Canvas::default();
        for i in 0..(HISTORY_LIMIT + 20) {
            canvas.commit(dot(pos2(i as f32 * 10.0, 0.0)));
        }
        assert_eq!(canvas.undo.len(), HISTORY_LIMIT);
    }

    #[test]
    fn only_sharp_bends_get_a_round_joint() {
        let radius = 3.0;
        // Straight line and a tiny bend: no disc needed.
        assert!(!needs_round_joint(
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(20.0, 0.0),
            radius
        ));
        assert!(!needs_round_joint(
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(20.0, 1.0),
            radius
        ));
        // Right angle: a flat joint would show a notch.
        assert!(needs_round_joint(
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            radius
        ));
    }
}
