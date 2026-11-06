//! Plane geometry for the canvas: hit tests and the cuts made by the eraser.

use eframe::egui::{Pos2, Rect};

use crate::model::{Geometry, Line};

/// Distance from `point` to the segment `a`-`b`.
pub fn distance_to_segment(point: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let length_sq = ab.length_sq();
    if length_sq <= f32::EPSILON {
        point.distance(a)
    } else {
        point.distance(a + ab * ((point - a).dot(ab) / length_sq).clamp(0.0, 1.0))
    }
}

/// True when `point` is within `reach` of the drawn outline of `line`, or inside
/// it when it is a filled shape.
pub fn contains(line: &Line, point: Pos2, reach: f32) -> bool {
    let Some((a, b)) = line.points.first().zip(line.points.last()) else {
        return false;
    };
    if line.filled {
        match line.geometry {
            Geometry::Rectangle => {
                if Rect::from_two_pos(*a, *b).contains(point) {
                    return true;
                }
            }
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
    let outline = line.outline_points();
    match outline.as_slice() {
        [] => false,
        [only] => only.distance(point) <= reach,
        points => points
            .windows(2)
            .any(|pair| distance_to_segment(point, pair[0], pair[1]) <= reach),
    }
}

/// True when the eraser disc of the given `radius` at `p` overlaps `line`.
pub fn touches(line: &Line, p: Pos2, radius: f32) -> bool {
    let reach = radius + line.width * 0.5;
    if !line.bounds.expand(reach).contains(p) {
        return false;
    }
    if line.geometry != Geometry::Freehand {
        return contains(line, p, reach);
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
pub fn split_around(line: &Line, p: Pos2, reach: f32) -> Vec<Line> {
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
pub fn disc_interval(a: Pos2, b: Pos2, center: Pos2, r: f32) -> Option<(f32, f32)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, Color32};

    fn shape(geometry: Geometry, filled: bool) -> Line {
        Line::shape(
            geometry,
            Color32::RED,
            4.0,
            filled,
            pos2(10.0, 10.0),
            pos2(40.0, 40.0),
        )
    }

    #[test]
    fn a_single_dot_can_be_picked() {
        let dot = Line::from_points(Color32::RED, 6.0, vec![pos2(10.0, 10.0)]);
        assert!(contains(&dot, pos2(11.0, 10.0), 3.0));
        assert!(!contains(&dot, pos2(20.0, 10.0), 3.0));
    }

    #[test]
    fn a_filled_shape_is_hit_inside_and_an_empty_one_only_on_its_outline() {
        assert!(contains(
            &shape(Geometry::Rectangle, true),
            pos2(25.0, 25.0),
            0.0
        ));
        assert!(!contains(
            &shape(Geometry::Rectangle, false),
            pos2(25.0, 25.0),
            2.0
        ));
        assert!(contains(
            &shape(Geometry::Rectangle, false),
            pos2(10.0, 25.0),
            2.0
        ));
        assert!(contains(
            &shape(Geometry::Ellipse, true),
            pos2(25.0, 25.0),
            0.0
        ));
        // The corner of the bounding box is outside the ellipse.
        assert!(!contains(
            &shape(Geometry::Ellipse, true),
            pos2(11.0, 11.0),
            0.0
        ));
    }

    #[test]
    fn a_filled_shape_is_erasable() {
        assert!(touches(
            &shape(Geometry::Rectangle, true),
            pos2(20.0, 20.0),
            2.0
        ));
    }

    #[test]
    fn a_segment_crossing_the_disc_is_cut_in_two() {
        let line = Line::from_points(Color32::BLACK, 4.0, vec![pos2(0.0, 0.0), pos2(100.0, 0.0)]);
        let pieces = split_around(&line, pos2(50.0, 0.0), 7.0);
        assert_eq!(pieces.len(), 2);
        assert!((pieces[0].points[1].x - 43.0).abs() < 0.01);
        assert!((pieces[1].points[0].x - 57.0).abs() < 0.01);
    }

    #[test]
    fn a_disc_that_misses_the_segment_gives_no_interval() {
        assert_eq!(
            disc_interval(pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(5.0, 20.0), 3.0),
            None
        );
    }
}
