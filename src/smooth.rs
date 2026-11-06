//! Turns the raw pointer positions of a brush stroke into a smooth polyline.
//!
//! The accepted positions are the control points of a chain of quadratic curves
//! that run between the midpoints of consecutive positions. The curve has no
//! corners and never lags behind the pointer: only the last half step is still
//! provisional, and it is drawn straight to the pointer.

use eframe::egui::Pos2;

/// Largest gap between a curve and the straight pieces that replace it, in world units.
const TOLERANCE: f32 = 0.05;
/// Most straight pieces used for one curve.
const MAX_PIECES: usize = 64;
/// Points closer than this are the same point.
const EPSILON: f32 = 1e-3;

/// Builds a stroke point by point as the pointer moves.
pub struct Smoother {
    /// Positions closer than this to the last accepted one only move the tip.
    min_step: f32,
    /// Smoothed points that will not change any more.
    fixed: Vec<Pos2>,
    /// Last accepted pointer position, the control point of the next curve.
    last: Pos2,
    /// End of the smoothed part: a midpoint, or the start of the stroke.
    anchor: Pos2,
    /// Latest pointer position, drawn straight from the last accepted one.
    tip: Pos2,
    /// Number of accepted positions.
    count: usize,
}

impl Smoother {
    /// Starts a stroke at `start`. `min_step` is the smallest move that counts.
    pub fn new(start: Pos2, min_step: f32) -> Self {
        Self {
            min_step,
            fixed: vec![start],
            last: start,
            anchor: start,
            tip: start,
            count: 1,
        }
    }

    /// Follows the pointer to `p`.
    pub fn add(&mut self, p: Pos2) {
        self.tip = p;
        if p.distance(self.last) >= self.min_step {
            self.advance(p);
        }
    }

    /// Ends the stroke exactly at `end` and returns its points.
    pub fn finish(mut self, end: Pos2) -> Vec<Pos2> {
        self.tip = end;
        if end.distance(self.last) > EPSILON {
            self.advance(end);
        }
        let mut points = Vec::new();
        self.write_to(&mut points);
        points
    }

    /// Replaces the content of `out` with the stroke as it is now.
    pub fn write_to(&self, out: &mut Vec<Pos2>) {
        out.clear();
        out.extend_from_slice(&self.fixed);
        for p in [self.last, self.tip] {
            if out.last().is_none_or(|q| q.distance(p) > EPSILON) {
                out.push(p);
            }
        }
    }

    /// Accepts `p`, and smooths the part of the stroke that it completes.
    fn advance(&mut self, p: Pos2) {
        let end = self.last.lerp(p, 0.5);
        if self.count == 1 {
            // The stroke starts with a straight piece up to the first midpoint.
            push_unique(&mut self.fixed, end);
        } else {
            curve(self.anchor, self.last, end, &mut self.fixed);
        }
        self.anchor = end;
        self.last = p;
        self.count += 1;
    }
}

fn push_unique(out: &mut Vec<Pos2>, p: Pos2) {
    if out.last().is_none_or(|q| q.distance(p) > EPSILON) {
        out.push(p);
    }
}

/// Appends the quadratic curve from `a` to `b` with control point `c`, as straight
/// pieces. `a` itself is not appended. A bend gets more pieces than a straight run.
fn curve(a: Pos2, c: Pos2, b: Pos2, out: &mut Vec<Pos2>) {
    // The gap between a piece and the curve is |a - 2c + b| / (4 n²) for n pieces.
    let bend = (a.to_vec2() - c.to_vec2() * 2.0 + b.to_vec2()).length();
    let pieces = ((bend / (4.0 * TOLERANCE)).sqrt().ceil() as usize).clamp(1, MAX_PIECES);
    for i in 1..=pieces {
        let t = i as f32 / pieces as f32;
        let u = 1.0 - t;
        let point = if i == pieces {
            b
        } else {
            Pos2::new(
                u * u * a.x + 2.0 * u * t * c.x + t * t * b.x,
                u * u * a.y + 2.0 * u * t * c.y + t * t * b.y,
            )
        };
        push_unique(out, point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn smooth(points: &[Pos2], min_step: f32) -> Vec<Pos2> {
        let mut smoother = Smoother::new(points[0], min_step);
        for p in &points[1..points.len() - 1] {
            smoother.add(*p);
        }
        smoother.finish(*points.last().unwrap())
    }

    #[test]
    fn a_click_stays_a_single_point() {
        let smoother = Smoother::new(pos2(5.0, 5.0), 1.0);
        assert_eq!(smoother.finish(pos2(5.0, 5.0)), vec![pos2(5.0, 5.0)]);
    }

    #[test]
    fn the_stroke_starts_and_ends_exactly_where_the_pointer_did() {
        let raw = [
            pos2(0.0, 0.0),
            pos2(10.0, 4.0),
            pos2(20.0, 0.0),
            pos2(30.0, 6.0),
        ];
        let out = smooth(&raw, 1.0);
        assert_eq!(out.first(), Some(&raw[0]));
        assert_eq!(out.last(), Some(&raw[3]));
    }

    #[test]
    fn a_straight_run_needs_very_few_points() {
        let raw: Vec<Pos2> = (0..=100).map(|i| pos2(i as f32 * 3.0, 0.0)).collect();
        let out = smooth(&raw, 1.0);
        // One point per raw position at most: collinear pieces are not subdivided.
        assert!(out.len() <= raw.len() + 1, "{}", out.len());
    }

    #[test]
    fn the_curve_stays_close_to_the_raw_positions() {
        // A zigzag: the curve may cut the corners, but never by more than half a step.
        let raw: Vec<Pos2> = (0..20)
            .map(|i| pos2(i as f32 * 8.0, if i % 2 == 0 { 0.0 } else { 6.0 }))
            .collect();
        let out = smooth(&raw, 1.0);
        for p in &raw[1..raw.len() - 1] {
            let gap = out
                .windows(2)
                .map(|w| crate::geometry::distance_to_segment(*p, w[0], w[1]))
                .fold(f32::INFINITY, f32::min);
            assert!(gap <= 3.5, "raw point {p:?} is {gap} away");
        }
    }

    #[test]
    fn small_moves_only_move_the_tip() {
        let mut smoother = Smoother::new(pos2(0.0, 0.0), 2.0);
        smoother.add(pos2(1.0, 0.0));
        let mut out = Vec::new();
        smoother.write_to(&mut out);
        assert_eq!(out, vec![pos2(0.0, 0.0), pos2(1.0, 0.0)]);

        smoother.add(pos2(1.5, 0.5));
        smoother.write_to(&mut out);
        assert_eq!(out.last(), Some(&pos2(1.5, 0.5)));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn the_preview_matches_the_final_stroke_until_the_last_step() {
        let raw: Vec<Pos2> = (0..30)
            .map(|i| {
                let t = i as f32 * 0.3;
                pos2(t * 20.0, (t * 2.0).sin() * 30.0)
            })
            .collect();
        let mut smoother = Smoother::new(raw[0], 1.0);
        for p in &raw[1..raw.len() - 1] {
            smoother.add(*p);
        }
        let mut preview = Vec::new();
        smoother.write_to(&mut preview);
        let done = smoother.finish(*raw.last().unwrap());
        // The finished stroke only adds to the end of the preview.
        let shared = preview.len() - 2;
        assert_eq!(preview[..shared], done[..shared]);
    }
}
