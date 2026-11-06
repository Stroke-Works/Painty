//! Draws strokes as triangle meshes.
//!
//! A stroke is cut into triangles once, in world coordinates, and the triangles
//! are kept with the stroke. Each frame only moves their corners to the screen,
//! which is much cheaper than building the outline again. The soft edge (one
//! screen pixel of anti-aliasing) is added at that point, so it stays one pixel
//! wide at any zoom.
//!
//! A stroke is a chain of flat bands, one per segment. Where two segments meet
//! at a gentle angle they share their corners (a miter joint). At a sharper
//! angle, or when the segments are too short for a miter, each band ends square
//! and a round disc covers the joint. Both ends of a line get a disc too.

use std::f32::consts::TAU;

use eframe::egui::epaint::{Mesh, Vertex, WHITE_UV};
use eframe::egui::{vec2, Color32, Painter, Pos2, Shape, Vec2};

use crate::model::{circle_steps, Geometry, Line};

/// A miter joint may stick out of the round joint it replaces by this much, in world units.
const MITER_EXCESS: f32 = 0.1;
/// Points closer than this are the same point.
const MIN_SEGMENT: f32 = 1e-3;
/// A batch is sent to the painter before it reaches this many vertices.
const MAX_BATCH_VERTICES: usize = 60_000;

/// How a vertex is placed on the screen, once the zoom and the pixel size are known.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rim {
    /// Inside a filled shape: full color, no offset.
    Fill,
    /// On the center line of a stroke: full color, no offset.
    Core,
    /// Edge of the solid part of a stroke.
    Inner,
    /// Edge of the soft border of a stroke: transparent.
    Outer,
}

#[derive(Clone, Copy)]
struct Vert {
    /// Position in the drawing.
    at: Pos2,
    /// Direction, scaled by the miter length, in which `Inner` and `Outer` vertices move.
    dir: Vec2,
    rim: Rim,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Join {
    /// Gentle bends are mitered, sharp ones are round.
    Round,
    /// Every bend is mitered: the corners of a rectangle stay square.
    Miter,
}

/// The triangles of one stroke, in world coordinates.
#[derive(Default)]
pub struct StrokeMesh {
    radius: f32,
    verts: Vec<Vert>,
    indices: Vec<u32>,
}

fn perp(d: Vec2) -> Vec2 {
    vec2(-d.y, d.x)
}

impl StrokeMesh {
    /// Cuts `line` into triangles.
    pub fn new(line: &Line) -> Self {
        let mut mesh = Self {
            radius: line.width * 0.5,
            ..Self::default()
        };
        let outline = line.outline_points();
        match line.geometry {
            Geometry::Freehand | Geometry::Line => mesh.stroke(&outline, false, Join::Round),
            Geometry::Rectangle => {
                if line.filled && outline.len() == 5 {
                    mesh.fill(&outline[..4]);
                }
                mesh.stroke(&outline, true, Join::Miter);
            }
            Geometry::Ellipse => {
                if line.filled && outline.len() > 3 {
                    mesh.fill(&outline[..outline.len() - 1]);
                }
                mesh.stroke(&outline, true, Join::Round);
            }
        }
        mesh.verts.shrink_to_fit();
        mesh.indices.shrink_to_fit();
        mesh
    }

    fn vertex(&mut self, at: Pos2, dir: Vec2, rim: Rim) -> u32 {
        self.verts.push(Vert { at, dir, rim });
        (self.verts.len() - 1) as u32
    }

    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }

    fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.triangle(a, b, c);
        self.triangle(a, c, d);
    }

    /// Fills a convex polygon.
    fn fill(&mut self, polygon: &[Pos2]) {
        if polygon.len() < 3 {
            return;
        }
        let first = self.verts.len() as u32;
        for p in polygon {
            self.vertex(*p, Vec2::ZERO, Rim::Fill);
        }
        for i in 1..polygon.len() as u32 - 1 {
            self.triangle(first, first + i, first + i + 1);
        }
    }

    /// A round disc, used for the ends of a line and for the joints that need one.
    fn disc(&mut self, at: Pos2) {
        let steps = circle_steps(self.radius);
        let center = self.vertex(at, Vec2::ZERO, Rim::Core);
        let inner = self.verts.len() as u32;
        for k in 0..steps {
            let a = TAU * k as f32 / steps as f32;
            self.vertex(at, vec2(a.cos(), a.sin()), Rim::Inner);
        }
        let outer = self.verts.len() as u32;
        for k in 0..steps {
            let a = TAU * k as f32 / steps as f32;
            self.vertex(at, vec2(a.cos(), a.sin()), Rim::Outer);
        }
        let steps = steps as u32;
        for k in 0..steps {
            let next = (k + 1) % steps;
            self.triangle(center, inner + k, inner + next);
            self.quad(inner + k, outer + k, outer + next, inner + next);
        }
    }

    /// Four vertices across the stroke at `at`: left edge, left inside, right inside,
    /// right edge. `side` points to the left and is as long as the miter requires.
    fn section(&mut self, at: Pos2, side: Vec2) -> [u32; 4] {
        [
            self.vertex(at, side, Rim::Outer),
            self.vertex(at, side, Rim::Inner),
            self.vertex(at, -side, Rim::Inner),
            self.vertex(at, -side, Rim::Outer),
        ]
    }

    /// Joins two sections into a band: soft left edge, solid middle, soft right edge.
    fn band(&mut self, from: [u32; 4], to: [u32; 4]) {
        for k in 0..3 {
            self.quad(from[k], to[k], to[k + 1], from[k + 1]);
        }
    }

    /// The side vector of a miter joint between the directions `before` and `after`,
    /// or `None` when the joint must be round.
    fn miter(
        &self,
        before: Vec2,
        after: Vec2,
        len_before: f32,
        len_after: f32,
        join: Join,
    ) -> Option<Vec2> {
        let dot = before.dot(after);
        // A U-turn has no miter.
        if 1.0 + dot <= 1e-4 {
            return None;
        }
        let side = (perp(before) + perp(after)) / (1.0 + dot);
        if join == Join::Miter {
            return Some(side);
        }
        let r = self.radius;
        let half_cos = ((1.0 + dot) * 0.5).sqrt();
        // How far the pointed corner sticks out of the round joint.
        let excess = r * (1.0 / half_cos - 1.0);
        // The inner corner must stay on the two segments, or it sticks out of the stroke.
        let reach = r * (1.0 - half_cos * half_cos).sqrt() / half_cos;
        (excess <= MITER_EXCESS && len_before.min(len_after) >= reach).then_some(side)
    }

    /// Cuts a polyline into bands and joints. `closed` joins the last point to the first.
    fn stroke(&mut self, points: &[Pos2], closed: bool, join: Join) {
        let mut pts: Vec<Pos2> = Vec::with_capacity(points.len());
        for p in points {
            if pts.last().is_none_or(|q| q.distance(*p) > MIN_SEGMENT) {
                pts.push(*p);
            }
        }
        if closed && pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) <= MIN_SEGMENT {
            pts.pop();
        }
        let n = pts.len();
        match n {
            0 => return,
            1 => return self.disc(pts[0]),
            _ => {}
        }
        // Two points cannot enclose anything: draw them as a line.
        let closed = closed && n >= 3;
        let segments = if closed { n } else { n - 1 };
        let dirs: Vec<Vec2> = (0..segments)
            .map(|i| (pts[(i + 1) % n] - pts[i]).normalized())
            .collect();
        let lens: Vec<f32> = (0..segments)
            .map(|i| pts[(i + 1) % n].distance(pts[i]))
            .collect();

        let mut starts = vec![[0u32; 4]; segments];
        let mut ends = vec![[0u32; 4]; segments];
        if !closed {
            starts[0] = self.section(pts[0], perp(dirs[0]));
            ends[segments - 1] = self.section(pts[n - 1], perp(dirs[segments - 1]));
            self.disc(pts[0]);
            self.disc(pts[n - 1]);
        }
        let joints = if closed { 0..n } else { 1..n - 1 };
        for j in joints {
            let (before, after) = ((j + segments - 1) % segments, j);
            match self.miter(dirs[before], dirs[after], lens[before], lens[after], join) {
                Some(side) => {
                    let shared = self.section(pts[j], side);
                    ends[before] = shared;
                    starts[after] = shared;
                }
                None => {
                    ends[before] = self.section(pts[j], perp(dirs[before]));
                    starts[after] = self.section(pts[j], perp(dirs[after]));
                    self.disc(pts[j]);
                }
            }
        }
        for i in 0..segments {
            self.band(starts[i], ends[i]);
        }
    }

    /// Appends the triangles to `out`, placed on the screen. `origin` is where the
    /// drawing's origin is, `zoom` the scale and `feather` the width of the soft edge,
    /// both in screen points.
    fn emit(&self, color: Color32, origin: Pos2, zoom: f32, feather: f32, out: &mut Mesh) {
        let half_feather = feather * 0.5;
        let width = 2.0 * self.radius * zoom;
        // A line thinner than the soft edge is drawn as wide as it, but fainter.
        let (radius, strength) = if width >= feather {
            (self.radius * zoom, 1.0)
        } else {
            (half_feather, width / feather)
        };
        let inner = radius - half_feather;
        let outer = radius + half_feather;
        let core = if strength < 1.0 {
            color.gamma_multiply(strength)
        } else {
            color
        };

        let base = out.vertices.len() as u32;
        out.vertices.extend(self.verts.iter().map(|v| {
            let (offset, color) = match v.rim {
                Rim::Fill => (0.0, color),
                Rim::Core => (0.0, core),
                Rim::Inner => (inner, core),
                Rim::Outer => (outer, Color32::TRANSPARENT),
            };
            Vertex {
                pos: origin + v.at.to_vec2() * zoom + v.dir * offset,
                uv: WHITE_UV,
                color,
            }
        }));
        out.indices.extend(self.indices.iter().map(|i| base + i));
    }
}

/// Collects strokes into as few meshes as possible and sends them to the painter.
pub struct Pen<'a> {
    painter: &'a Painter,
    mesh: Mesh,
    origin: Pos2,
    zoom: f32,
    feather: f32,
}

impl<'a> Pen<'a> {
    /// `origin` is the screen position of the drawing's origin.
    pub fn new(painter: &'a Painter, origin: Pos2, zoom: f32) -> Self {
        Self {
            painter,
            mesh: Mesh::default(),
            origin,
            zoom,
            feather: 1.0 / painter.ctx().pixels_per_point(),
        }
    }

    /// Draws a finished stroke, whose triangles are kept for the next frames.
    pub fn draw(&mut self, line: &Line) {
        let stroke = line.mesh.get_or_init(|| StrokeMesh::new(line));
        self.push(stroke, line.color);
    }

    /// Draws a stroke that is still changing.
    pub fn draw_fresh(&mut self, line: &Line) {
        self.push(&StrokeMesh::new(line), line.color);
    }

    fn push(&mut self, stroke: &StrokeMesh, color: Color32) {
        if self.mesh.vertices.len() + stroke.verts.len() > MAX_BATCH_VERTICES {
            self.flush();
        }
        stroke.emit(color, self.origin, self.zoom, self.feather, &mut self.mesh);
    }

    fn flush(&mut self) {
        let mesh = std::mem::take(&mut self.mesh);
        if !mesh.is_empty() {
            self.painter.add(Shape::mesh(mesh));
        }
    }

    /// Sends what is left to the painter.
    pub fn finish(mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, Rect};

    const SIZE: usize = 160;

    /// Coverage of each pixel by the triangles, from 0 to 1, drawn at zoom 1 with a
    /// one pixel soft edge. Overlapping triangles combine like opaque layers would.
    fn render(line: &Line) -> Vec<f32> {
        let mut out = Mesh::default();
        StrokeMesh::new(line).emit(Color32::WHITE, Pos2::ZERO, 1.0, 1.0, &mut out);
        assert!(out.is_valid());
        let mut coverage = vec![0.0f32; SIZE * SIZE];
        for tri in out.indices.chunks(3) {
            let v = [
                out.vertices[tri[0] as usize],
                out.vertices[tri[1] as usize],
                out.vertices[tri[2] as usize],
            ];
            let (a, b, c) = (v[0].pos, v[1].pos, v[2].pos);
            let area = (b - a).x * (c - a).y - (b - a).y * (c - a).x;
            if area.abs() < 1e-9 {
                continue;
            }
            let min = a.min(b).min(c);
            let max = a.max(b).max(c);
            let x0 = (min.x.floor().max(0.0)) as usize;
            let y0 = (min.y.floor().max(0.0)) as usize;
            let x1 = (max.x.ceil().max(0.0) as usize).min(SIZE - 1);
            let y1 = (max.y.ceil().max(0.0) as usize).min(SIZE - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let p = pos2(x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((b - p).x * (c - p).y - (b - p).y * (c - p).x) / area;
                    let w1 = ((c - p).x * (a - p).y - (c - p).y * (a - p).x) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let alpha = (w0 * f32::from(v[0].color.a())
                        + w1 * f32::from(v[1].color.a())
                        + w2 * f32::from(v[2].color.a()))
                        / 255.0;
                    let cell = &mut coverage[y * SIZE + x];
                    *cell = 1.0 - (1.0 - *cell) * (1.0 - alpha);
                }
            }
        }
        coverage
    }

    /// Pixels where the triangles and the exact shape disagree by more than half a pixel
    /// of coverage, and the number of pixels the exact shape covers.
    fn disagreement(line: &Line, exact: impl Fn(Pos2) -> bool) -> (usize, usize) {
        let coverage = render(line);
        let mut wrong = 0;
        let mut covered = 0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let inside = exact(pos2(x as f32 + 0.5, y as f32 + 0.5));
                covered += usize::from(inside);
                let drawn = coverage[y * SIZE + x];
                if (drawn - f32::from(u8::from(inside))).abs() > 0.6 {
                    wrong += 1;
                }
            }
        }
        (wrong, covered)
    }

    fn near_polyline(points: Vec<Pos2>, radius: f32) -> impl Fn(Pos2) -> bool {
        move |p| match points.as_slice() {
            [only] => only.distance(p) <= radius,
            pts => pts
                .windows(2)
                .any(|w| crate::geometry::distance_to_segment(p, w[0], w[1]) <= radius),
        }
    }

    fn stroke_of(points: Vec<Pos2>, width: f32) -> Line {
        Line::from_points(Color32::WHITE, width, points)
    }

    fn assert_matches(line: &Line, exact: impl Fn(Pos2) -> bool) {
        let (wrong, covered) = disagreement(line, exact);
        assert!(covered > 0, "the shape covers nothing");
        // A pixel on the very edge can round either way, and a sharp corner has a few
        // more of them. Allow 1 % of the shape plus a few pixels, never more.
        assert!(
            wrong * 100 <= covered + 400,
            "{wrong} wrong pixels out of {covered}"
        );
    }

    #[test]
    fn a_zigzag_with_sharp_corners_is_drawn_exactly() {
        for width in [1.0, 6.0, 30.0] {
            let points = vec![
                pos2(20.0, 20.0),
                pos2(80.0, 30.0),
                pos2(30.0, 60.0),
                pos2(130.0, 70.0),
                pos2(60.0, 130.0),
            ];
            let line = stroke_of(points.clone(), width);
            assert_matches(&line, near_polyline(points, width / 2.0));
        }
    }

    #[test]
    fn a_dense_curve_under_a_thick_brush_has_no_spikes() {
        // One point per pixel along a tight curve, with a brush wider than the curve.
        let points: Vec<Pos2> = (0..120)
            .map(|i| {
                let a = i as f32 * 0.06;
                pos2(
                    80.0 + 40.0 * a.cos() * (1.0 - i as f32 / 200.0),
                    80.0 + 40.0 * a.sin(),
                )
            })
            .collect();
        for width in [4.0, 20.0, 60.0] {
            let line = stroke_of(points.clone(), width);
            assert_matches(&line, near_polyline(points.clone(), width / 2.0));
        }
    }

    #[test]
    fn a_wobbly_stroke_is_drawn_exactly() {
        let points: Vec<Pos2> = (0..150)
            .map(|i| {
                let t = i as f32;
                pos2(
                    10.0 + t,
                    80.0 + 25.0 * (t * 0.13).sin() + (t * 1.7).sin() * 3.0,
                )
            })
            .collect();
        for width in [2.0, 12.0, 40.0] {
            let line = stroke_of(points.clone(), width);
            assert_matches(&line, near_polyline(points.clone(), width / 2.0));
        }
    }

    #[test]
    fn a_dot_is_a_round_disc() {
        let line = stroke_of(vec![pos2(80.0, 80.0)], 40.0);
        assert_matches(&line, |p| p.distance(pos2(80.0, 80.0)) <= 20.0);
    }

    #[test]
    fn a_line_has_round_ends() {
        let points = vec![pos2(30.0, 80.0), pos2(130.0, 80.0)];
        let line = Line::shape(
            Geometry::Line,
            Color32::WHITE,
            24.0,
            false,
            points[0],
            points[1],
        );
        assert_matches(&line, near_polyline(points, 12.0));
    }

    #[test]
    fn a_rectangle_keeps_square_corners() {
        let (a, b) = (pos2(30.0, 40.0), pos2(120.0, 110.0));
        let line = Line::shape(Geometry::Rectangle, Color32::WHITE, 14.0, false, a, b);
        let outer = Rect::from_two_pos(a, b).expand(7.0);
        let inner = Rect::from_two_pos(a, b).shrink(7.0);
        assert_matches(&line, |p| outer.contains(p) && !inner.contains(p));
    }

    #[test]
    fn a_thick_stroke_on_a_narrow_rectangle_stays_inside_its_outline() {
        let (a, b) = (pos2(60.0, 30.0), pos2(76.0, 130.0));
        let line = Line::shape(Geometry::Rectangle, Color32::WHITE, 40.0, false, a, b);
        let outer = Rect::from_two_pos(a, b).expand(20.0);
        // The stroke is wider than the rectangle: it has no hole.
        assert_matches(&line, |p| outer.contains(p));
    }

    #[test]
    fn a_filled_rectangle_and_ellipse_cover_their_inside() {
        let (a, b) = (pos2(30.0, 40.0), pos2(120.0, 110.0));
        let rect = Line::shape(Geometry::Rectangle, Color32::WHITE, 4.0, true, a, b);
        let outer = Rect::from_two_pos(a, b).expand(2.0);
        assert_matches(&rect, |p| outer.contains(p));

        let ellipse = Line::shape(Geometry::Ellipse, Color32::WHITE, 4.0, true, a, b);
        let center = a.lerp(b, 0.5);
        let (rx, ry) = (45.0 + 2.0, 35.0 + 2.0);
        assert_matches(&ellipse, |p| {
            let (x, y) = ((p.x - center.x) / rx, (p.y - center.y) / ry);
            x * x + y * y <= 1.0
        });
    }

    #[test]
    fn an_ellipse_outline_follows_its_polygon() {
        let (a, b) = (pos2(20.0, 50.0), pos2(140.0, 110.0));
        let line = Line::shape(Geometry::Ellipse, Color32::WHITE, 10.0, false, a, b);
        assert_matches(&line, near_polyline(line.outline_points(), 5.0));
    }

    #[test]
    fn a_stroke_thinner_than_a_pixel_fades_instead_of_vanishing() {
        let line = stroke_of(vec![pos2(10.0, 80.0), pos2(150.0, 80.0)], 0.25);
        let coverage = render(&line);
        let row: f32 = (0..SIZE)
            .map(|x| coverage[80 * SIZE + x] + coverage[79 * SIZE + x])
            .sum();
        // About a quarter of a pixel of coverage per pixel of length.
        let per_pixel = row / 140.0;
        assert!((0.15..=0.4).contains(&per_pixel), "{per_pixel}");
    }

    #[test]
    fn degenerate_strokes_make_valid_meshes() {
        let cases = [
            Vec::new(),
            vec![pos2(5.0, 5.0)],
            vec![pos2(5.0, 5.0), pos2(5.0, 5.0)],
            vec![pos2(5.0, 5.0), pos2(5.0, 5.0005), pos2(5.0, 5.0)],
            vec![pos2(1.0, 1.0), pos2(9.0, 1.0), pos2(1.0, 1.0)],
        ];
        for points in cases {
            let mesh = StrokeMesh::new(&stroke_of(points, 8.0));
            let max = mesh.verts.len() as u32;
            assert!(mesh.indices.iter().all(|i| *i < max));
            assert!(mesh
                .verts
                .iter()
                .all(|v| v.at.is_finite() && v.dir.is_finite()));
        }
    }

    #[test]
    fn a_stroke_needs_few_triangles_when_it_is_smooth() {
        // A gentle arc is a single strip: no disc at its joints.
        let points: Vec<Pos2> = (0..100)
            .map(|i| {
                let a = i as f32 * 0.01;
                pos2(80.0 + 60.0 * a.cos(), 80.0 + 60.0 * a.sin())
            })
            .collect();
        let mesh = StrokeMesh::new(&stroke_of(points, 6.0));
        // 99 bands of 6 triangles, plus two discs.
        assert!(mesh.indices.len() / 3 < 99 * 6 + 2 * 200);
    }
}
