use std::collections::VecDeque;
use std::sync::Arc;

use eframe::egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui};

use crate::model::{Brush, Line, Tool};

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
#[derive(Default)]
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

    /// Handles mouse input on the drawing area and paints all strokes.
    /// `ring` is the color of the eraser outline.
    pub fn show(&mut self, ui: &mut Ui, brush: &Brush, ring: Color32) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        self.rect = Some(response.rect);
        let radius = brush.width / 2.0;
        let erasing = brush.tool == Tool::Eraser;
        // Frame duration, capped so a stalled frame does not make the stroke jump.
        let dt = ui.input(|i| i.stable_dt).clamp(1.0 / 1000.0, 0.1);

        if erasing {
            self.handle_eraser(&response, radius, dt);
        } else {
            self.handle_brush(&response, brush, dt);
        }

        // Only strokes that touch the visible area are drawn.
        let clip = painter.clip_rect();
        for line in &self.lines {
            if line.bounds.intersects(clip) {
                draw_line(&painter, line);
            }
        }
        if let Some(line) = &self.current {
            draw_line(&painter, line);
        }

        // Outline of the eraser under the pointer.
        if erasing {
            if let Some(pos) = response.hover_pos() {
                painter.circle_stroke(pos, radius, Stroke::new(1.0_f32, ring));
            }
        }
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
    fn handle_brush(&mut self, response: &Response, brush: &Brush, dt: f32) {
        if response.drag_started() {
            self.current = Some(Line::new(brush.color, brush.width));
            self.smooth_pos = response.interact_pointer_pos();
        }

        if response.dragged() {
            if let Some(target) = response.interact_pointer_pos() {
                let p = self.smooth(target, dt);
                if let Some(line) = self.current.as_mut() {
                    line.push(p);
                }
            }
        }

        if response.drag_stopped() {
            if let Some(mut line) = self.current.take() {
                // Finish at the actual pointer, not the lagging smoothed point.
                if let Some(end) = response.interact_pointer_pos() {
                    line.push(end);
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
                let mut line = Line::new(brush.color, brush.width);
                line.push(pos);
                line.finish();
                self.commit(line);
            }
        }
    }

    /// Eraser: removes the parts of existing strokes under the pointer, with smoothing.
    fn handle_eraser(&mut self, response: &Response, radius: f32, dt: f32) {
        if response.drag_started() || response.clicked() {
            self.last_erase = None;
            self.smooth_pos = response.interact_pointer_pos();
            self.eraser_changed = false;
        }

        if response.dragged() {
            if let Some(target) = response.interact_pointer_pos() {
                let p = self.smooth(target, dt);
                let from = self.last_erase.unwrap_or(p);
                self.erase_between(from, p, radius);
                self.last_erase = Some(p);
            }
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.erase_at(pos, radius);
            }
        }

        if response.drag_stopped() {
            // Catch up to the actual release point: the smoothed point lags behind it.
            if let Some(pos) = response.interact_pointer_pos() {
                let from = self.last_erase.unwrap_or(pos);
                self.erase_between(from, pos, radius);
            }
            self.last_erase = None;
            self.smooth_pos = None;
            self.eraser_changed = false;
        }
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
    match line.points.as_slice() {
        [] => false,
        [only] => only.distance(p) <= reach,
        pts => pts
            .windows(2)
            .any(|w| disc_interval(w[0], w[1], p, reach).is_some()),
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

/// Draws one stroke: flat segments, round caps at both ends, and a disc only at
/// the bends where a flat joint would be visible.
fn draw_line(painter: &Painter, line: &Line) {
    let color = line.color;
    let radius = line.width / 2.0;
    let pts = &line.points;

    match pts.len() {
        0 => {}
        1 => {
            painter.circle_filled(pts[0], radius, color);
        }
        n => {
            painter.add(connected_stroke(line));
            painter.circle_filled(pts[0], radius, color);
            painter.circle_filled(pts[n - 1], radius, color);
            for w in pts.windows(3) {
                if needs_round_joint(w[0], w[1], w[2], radius) {
                    painter.circle_filled(w[1], radius, color);
                }
            }
        }
    }
}

fn connected_stroke(line: &Line) -> Shape {
    Shape::line(line.points.clone(), Stroke::new(line.width, line.color))
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

        match connected_stroke(&line) {
            Shape::Path(path) => assert_eq!(path.points, line.points),
            _ => panic!("a stroke must be tessellated as a connected path"),
        }
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
