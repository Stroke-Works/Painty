//! Image export. Draws the strokes at a higher resolution, then averages each block
//! of pixels, which gives smooth edges. Saves PNG or JPEG.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{vec2, Color32, Pos2, Rect, Vec2};
use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, RgbImage};

use crate::i18n::t;
use crate::model::{Geometry, Line};

/// Margin kept around the drawing in the exported image, in points.
const MARGIN: f32 = 8.0;
/// The drawing is rendered at up to this multiple of the output size, then averaged down.
const MAX_SUPERSAMPLE: u32 = 3;
/// JPEG quality, from 1 to 100.
const JPEG_QUALITY: u8 = 95;
/// Largest exported image, in pixels. A bigger drawing is exported at a lower scale.
const MAX_OUTPUT_PIXELS: f64 = 64_000_000.0;
/// Largest working buffer, in pixels (3 bytes each). Supersampling is lowered to fit,
/// which keeps the memory used by a big export reasonable.
const MAX_WORK_PIXELS: f64 = 120_000_000.0;

/// Output file format.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
}

impl Format {
    /// File extension, without the dot.
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
        }
    }

    /// Name shown in the file dialog filter.
    pub fn label(self) -> &'static str {
        match self {
            Format::Png => t("export.png"),
            Format::Jpeg => t("export.jpeg"),
        }
    }
}

/// Bounding box of all strokes, with a margin, in screen points.
/// Returns `None` when there is nothing to export.
pub fn bounds(lines: &[Arc<Line>]) -> Option<Rect> {
    let mut bounds = Rect::NOTHING;
    for line in lines {
        bounds = bounds.union(line.bounds);
    }
    if bounds.is_positive() {
        Some(bounds.expand(MARGIN))
    } else {
        None
    }
}

/// Size and scale actually used for an export.
#[derive(Debug, PartialEq)]
struct Plan {
    /// Output size, in pixels.
    w: u32,
    h: u32,
    /// Supersampling factor: the drawing is rendered at `s` times the output size.
    s: u32,
    /// Pixels per point, reduced from the requested value when the image is too big.
    ppp: f32,
}

/// Output size in pixels for a drawing of `size` points at `ppp` pixels per point.
fn pixel_size(size: Vec2, ppp: f64) -> (u64, u64) {
    let w = (size.x as f64 * ppp).round().max(1.0) as u64;
    let h = (size.y as f64 * ppp).round().max(1.0) as u64;
    (w, h)
}

/// Chooses the output size and supersampling so that a huge drawing can never
/// overflow an index or exhaust the memory.
fn plan(size: Vec2, ppp: f32) -> Plan {
    let mut ppp = ppp as f64;
    let (mut w, mut h) = pixel_size(size, ppp);
    // Shrink until the image fits. One or two rounds are enough: the factor is
    // exact, the small safety margin only absorbs rounding.
    while (w as f64) * (h as f64) > MAX_OUTPUT_PIXELS {
        ppp *= (MAX_OUTPUT_PIXELS / ((w as f64) * (h as f64))).sqrt() * 0.999;
        (w, h) = pixel_size(size, ppp);
    }

    let area = (w as f64) * (h as f64);
    let s = (1..=MAX_SUPERSAMPLE)
        .rev()
        .find(|s| area * f64::from(s * s) <= MAX_WORK_PIXELS)
        .unwrap_or(1);

    Plan {
        w: w as u32,
        h: h as u32,
        s,
        ppp: ppp as f32,
    }
}

/// Writes the drawing to `path` in the given format.
/// `origin` is the top-left corner of the drawing area and `size` its size, both in points.
/// `ppp` (pixels per point) keeps the export as sharp as the screen. A very large drawing
/// is exported at a lower scale instead of failing.
pub fn save(
    path: &Path,
    format: Format,
    lines: &[Arc<Line>],
    origin: Pos2,
    size: Vec2,
    bg: Color32,
    ppp: f32,
) -> Result<(), String> {
    let Plan { w, h, s, ppp } = plan(size, ppp);
    let (w, h, s) = (w as usize, h as usize, s as usize);
    let (bw, bh) = (w * s, h * s);

    // Start with the background color, at the supersampled resolution.
    let len = bw
        .checked_mul(bh)
        .and_then(|n| n.checked_mul(3))
        .ok_or(t("error.export_size"))?;
    let pixel = [bg.r(), bg.g(), bg.b()];
    let buf: Vec<u8> = pixel.iter().copied().cycle().take(len).collect();

    let mut buf = buf;
    {
        let mut raster = Raster {
            buf: &mut buf,
            w: bw,
            h: bh,
        };
        for line in lines {
            raster.draw(line, origin, ppp * s as f32);
        }
    }

    let pixels = downsample(&buf, bw, bh, s);
    drop(buf);
    let image = RgbImage::from_raw(w as u32, h as u32, pixels).ok_or(t("error.export_size"))?;

    match format {
        Format::Png => image
            .save_with_format(path, ImageFormat::Png)
            .map_err(|e| e.to_string()),
        Format::Jpeg => {
            let file = File::create(path).map_err(|e| e.to_string())?;
            let mut out = BufWriter::new(file);
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
                .encode_image(&image)
                .map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())
        }
    }
}

/// Averages each `s` by `s` block of an RGB buffer of size `sw` by `sh`.
fn downsample(src: &[u8], sw: usize, sh: usize, s: usize) -> Vec<u8> {
    let w = sw / s;
    let h = sh / s;
    let n = s * s;
    let mut out = vec![0u8; w * h * 3];

    for y in 0..h {
        for x in 0..w {
            let mut acc = [0usize; 3];
            for dy in 0..s {
                for dx in 0..s {
                    let i = ((y * s + dy) * sw + x * s + dx) * 3;
                    acc[0] += src[i] as usize;
                    acc[1] += src[i + 1] as usize;
                    acc[2] += src[i + 2] as usize;
                }
            }
            let o = (y * w + x) * 3;
            for c in 0..3 {
                out[o + c] = ((acc[c] + n / 2) / n) as u8;
            }
        }
    }
    out
}

/// RGB pixel buffer that strokes are painted into.
struct Raster<'a> {
    buf: &'a mut [u8],
    w: usize,
    h: usize,
}

impl Raster<'_> {
    /// Paints one stroke as a series of round segments.
    fn draw(&mut self, line: &Line, origin: Pos2, ppp: f32) {
        let r = line.width * 0.5 * ppp;
        let pts: Vec<Vec2> = line
            .outline_points()
            .iter()
            .map(|p| (*p - origin) * ppp)
            .collect();

        if line.filled && matches!(line.geometry, Geometry::Rectangle | Geometry::Ellipse) {
            self.fill_polygon(&pts, line.color);
        }
        match (line.geometry, pts.as_slice()) {
            (_, []) => {}
            (Geometry::Rectangle | Geometry::Ellipse, points) if points.len() > 2 => {
                for pair in points.windows(2) {
                    self.segment(pair[0], pair[1], r, line.color);
                }
            }
            (_, [only]) => self.segment(*only, *only, r, line.color),
            (_, points) => {
                for pair in points.windows(2) {
                    self.segment(pair[0], pair[1], r, line.color);
                }
            }
        }
    }

    fn fill_polygon(&mut self, points: &[Vec2], color: Color32) {
        let points = if points.first() == points.last() {
            &points[..points.len().saturating_sub(1)]
        } else {
            points
        };
        if points.len() < 3 {
            return;
        }
        let min_x = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let max_x = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_y = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
        let x0 = min_x.floor().max(0.0) as usize;
        let x1 = max_x.ceil().min(self.w as f32 - 1.0) as usize;
        let y0 = min_y.floor().max(0.0) as usize;
        let y1 = max_y.ceil().min(self.h as f32 - 1.0) as usize;
        if x0 > x1 || y0 > y1 {
            return;
        }
        let rgb = [color.r(), color.g(), color.b()];
        for y in y0..=y1 {
            for x in x0..=x1 {
                let point = vec2(x as f32 + 0.5, y as f32 + 0.5);
                if point_in_polygon(point, points) {
                    let index = (y * self.w + x) * 3;
                    self.buf[index..index + 3].copy_from_slice(&rgb);
                }
            }
        }
    }

    /// Fills every pixel within `r` of the segment from `a` to `b`.
    fn segment(&mut self, a: Vec2, b: Vec2, r: f32, color: Color32) {
        let x0 = (a.x.min(b.x) - r).floor().max(0.0);
        let x1 = (a.x.max(b.x) + r).ceil().min(self.w as f32 - 1.0);
        let y0 = (a.y.min(b.y) - r).floor().max(0.0);
        let y1 = (a.y.max(b.y) + r).ceil().min(self.h as f32 - 1.0);
        if x0 > x1 || y0 > y1 {
            return;
        }

        let ab = b - a;
        let len2 = ab.length_sq();
        let px = [color.r(), color.g(), color.b()];

        for y in (y0 as usize)..=(y1 as usize) {
            for x in (x0 as usize)..=(x1 as usize) {
                let p = vec2(x as f32 + 0.5, y as f32 + 0.5);
                let t = if len2 > 0.0 {
                    ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if (p - (a + ab * t)).length() <= r {
                    let i = (y * self.w + x) * 3;
                    self.buf[i..i + 3].copy_from_slice(&px);
                }
            }
        }
    }
}

fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let a = polygon[current];
        let b = polygon[previous];
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    fn stroke(points: &[Pos2]) -> Arc<Line> {
        Arc::new(Line::from_points(Color32::BLACK, 4.0, points.to_vec()))
    }

    fn pixel(buf: &[u8], x: usize, y: usize) -> [u8; 3] {
        let index = (y * 20 + x) * 3;
        buf[index..index + 3].try_into().unwrap()
    }

    #[test]
    fn bounds_cover_every_stroke() {
        let lines = vec![
            stroke(&[pos2(10.0, 10.0)]),
            stroke(&[pos2(100.0, 50.0), pos2(200.0, 80.0)]),
        ];
        let expected = lines[0].bounds.union(lines[1].bounds).expand(MARGIN);
        assert_eq!(bounds(&lines), Some(expected));
    }

    #[test]
    fn empty_drawing_has_no_bounds() {
        assert_eq!(bounds(&[]), None);
    }

    #[test]
    fn small_drawing_keeps_full_quality() {
        let plan = plan(vec2(100.0, 50.0), 2.0);
        assert_eq!(
            plan,
            Plan {
                w: 200,
                h: 100,
                s: MAX_SUPERSAMPLE,
                ppp: 2.0
            }
        );
    }

    #[test]
    fn huge_drawing_is_scaled_down_instead_of_overflowing() {
        let plan = plan(vec2(40_000.0, 40_000.0), 1.5);
        assert!((plan.w as f64) * (plan.h as f64) <= MAX_OUTPUT_PIXELS);
        assert!(plan.ppp < 1.5);
        assert_eq!(plan.s, 1);
        // A buffer index never comes close to overflowing a 32-bit integer.
        let len = (plan.w as u64 * plan.s as u64) * (plan.h as u64 * plan.s as u64) * 3;
        assert!(len < 1_000_000_000);
    }

    #[test]
    fn supersampling_drops_with_the_image_size() {
        // 20 megapixels: 3x would need 180 MP, 2x needs 80 MP.
        let plan = plan(vec2(5_000.0, 4_000.0), 1.0);
        assert_eq!(plan.s, 2);
    }

    #[test]
    fn downsample_averages_each_block() {
        // One 2x2 block: two black pixels and two white ones.
        let src = [0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 0];
        assert_eq!(downsample(&src, 2, 2, 2), vec![128, 128, 128]);
    }

    #[test]
    fn raster_paints_a_dot() {
        let mut buf = vec![255u8; 20 * 20 * 3];
        let mut raster = Raster {
            buf: &mut buf,
            w: 20,
            h: 20,
        };
        let dot = Line::from_points(Color32::BLACK, 6.0, vec![pos2(10.0, 10.0)]);
        raster.draw(&dot, pos2(0.0, 0.0), 1.0);

        let at = |x: usize, y: usize| buf[(y * 20 + x) * 3];
        assert_eq!(at(10, 10), 0);
        assert_eq!(at(0, 0), 255);
    }

    #[test]
    fn raster_fills_rectangle_shapes() {
        let mut buf = vec![255u8; 20 * 20 * 3];
        let mut raster = Raster {
            buf: &mut buf,
            w: 20,
            h: 20,
        };
        let rectangle = Line::shape(
            Geometry::Rectangle,
            Color32::RED,
            2.0,
            true,
            pos2(4.0, 4.0),
            pos2(16.0, 16.0),
        );
        raster.draw(&rectangle, pos2(0.0, 0.0), 1.0);

        assert_eq!(pixel(&buf, 10, 10), [255, 0, 0]);
        assert_eq!(pixel(&buf, 1, 1), [255, 255, 255]);
    }

    #[test]
    fn raster_fills_ellipses_without_filling_their_corners() {
        let mut buf = vec![255u8; 20 * 20 * 3];
        let mut raster = Raster {
            buf: &mut buf,
            w: 20,
            h: 20,
        };
        let ellipse = Line::shape(
            Geometry::Ellipse,
            Color32::BLUE,
            2.0,
            true,
            pos2(4.0, 4.0),
            pos2(16.0, 16.0),
        );
        raster.draw(&ellipse, pos2(0.0, 0.0), 1.0);

        assert_eq!(pixel(&buf, 10, 10), [0, 0, 255]);
        assert_eq!(pixel(&buf, 4, 4), [255, 255, 255]);
    }
}
