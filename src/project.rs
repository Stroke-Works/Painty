//! Native editable Painty project files.
//!
//! Format 2 (written) is a compact binary file, all numbers little-endian:
//!
//! ```text
//! "PAINTY" | version u16 | palette 6 x rgb | brush rgb | brush width f32 |
//! tool u8 | fill u8 | stroke count u32 |
//! per stroke: geometry u8 | filled u8 | rgb | width f32 | point count u32 | points (x f32, y f32)... |
//! SHA-256 of everything above
//! ```
//!
//! Format 1 (read only) was pretty-printed JSON. Old files still open; saving
//! always writes format 2.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{Color32, Pos2, Rect};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::model::{Brush, Geometry, Line, Tool};

const MAGIC: &[u8; 6] = b"PAINTY";
const FORMAT_VERSION: u16 = 2;
const LEGACY_JSON_VERSION: u32 = 1;
const HEADER_LEN: usize = MAGIC.len() + 2;
const CHECKSUM_LEN: usize = 32;
const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;
const MAX_STROKES: usize = 100_000;
const MAX_POINTS: usize = 2_000_000;

const TRUNCATED: &str = "The project file is truncated.";
const TOO_MANY_STROKES: &str = "The project contains too many strokes.";
const TOO_MANY_POINTS: &str = "The project contains too many points.";

pub struct Document {
    pub lines: Vec<Arc<Line>>,
    pub palette: [Color32; 6],
    pub brush: Brush,
}

// ---- Saving ----

pub fn save(
    path: &Path,
    lines: &[Arc<Line>],
    palette: &[Color32; 6],
    brush: &Brush,
) -> Result<(), String> {
    let bytes = encode(lines, palette, brush)?;
    // Write next to the target, then rename: a crash or a full disk can no
    // longer leave a half-written project in place of the previous one.
    let temp = path.with_extension("painty.tmp");
    if let Err(error) = fs::write(&temp, &bytes) {
        let _ = fs::remove_file(&temp);
        return Err(error.to_string());
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })
}

fn encode(lines: &[Arc<Line>], palette: &[Color32; 6], brush: &Brush) -> Result<Vec<u8>, String> {
    // Refuse what `load` would refuse, so a saved project can always be reopened.
    if lines.len() > MAX_STROKES {
        return Err(TOO_MANY_STROKES.to_owned());
    }
    let total_points: usize = lines.iter().map(|line| line.points.len()).sum();
    if total_points > MAX_POINTS {
        return Err(TOO_MANY_POINTS.to_owned());
    }

    let mut out =
        Vec::with_capacity(HEADER_LEN + 64 + lines.len() * 16 + total_points * 8 + CHECKSUM_LEN);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    for color in palette {
        out.extend_from_slice(&rgb(*color));
    }
    out.extend_from_slice(&rgb(brush.color));
    out.extend_from_slice(&brush.width.to_le_bytes());
    out.push(tool_code(brush.tool));
    out.push(u8::from(brush.fill_shapes));
    out.extend_from_slice(&(lines.len() as u32).to_le_bytes());
    for line in lines {
        out.push(geometry_code(line.geometry));
        out.push(u8::from(line.filled));
        out.extend_from_slice(&rgb(line.color));
        out.extend_from_slice(&line.width.to_le_bytes());
        out.extend_from_slice(&(line.points.len() as u32).to_le_bytes());
        for point in &line.points {
            out.extend_from_slice(&point.x.to_le_bytes());
            out.extend_from_slice(&point.y.to_le_bytes());
        }
    }
    let digest = Sha256::digest(&out);
    out.extend_from_slice(&digest);
    Ok(out)
}

// ---- Loading ----

pub fn load(path: &Path) -> Result<Document, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err("The project file is too large.".to_owned());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    decode(&bytes)
}

fn decode(bytes: &[u8]) -> Result<Document, String> {
    if bytes.starts_with(MAGIC) {
        decode_binary(bytes)
    } else if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        decode_legacy(bytes)
    } else {
        Err("This file is not a Painty project.".to_owned())
    }
}

fn decode_binary(bytes: &[u8]) -> Result<Document, String> {
    let version = bytes
        .get(MAGIC.len()..HEADER_LEN)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .ok_or(TRUNCATED)?;
    if version != FORMAT_VERSION {
        return Err(format!("Unsupported Painty project version: {version}."));
    }
    if bytes.len() < HEADER_LEN + CHECKSUM_LEN {
        return Err(TRUNCATED.to_owned());
    }
    let (body, checksum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
    if Sha256::digest(body).as_slice() != checksum {
        return Err("The project file is corrupted.".to_owned());
    }

    let mut reader = Reader {
        data: &body[HEADER_LEN..],
    };
    let mut palette = [Color32::BLACK; 6];
    for slot in &mut palette {
        *slot = color(reader.rgb()?);
    }
    let brush = Brush {
        color: color(reader.rgb()?),
        width: reader.f32()?,
        tool: tool_from(reader.u8()?)?,
        fill_shapes: reader.flag()?,
    };
    check_brush_width(brush.width)?;

    let count = reader.u32()? as usize;
    if count > MAX_STROKES {
        return Err(TOO_MANY_STROKES.to_owned());
    }
    let mut lines = Vec::with_capacity(count);
    let mut total_points = 0usize;
    for _ in 0..count {
        let geometry = geometry_from(reader.u8()?)?;
        let filled = reader.flag()?;
        let stroke_color = color(reader.rgb()?);
        let width = reader.f32()?;
        let len = reader.u32()? as usize;
        total_points = total_points
            .checked_add(len)
            .filter(|total| *total <= MAX_POINTS)
            .ok_or(TOO_MANY_POINTS)?;
        let raw = reader.bytes(len * 8)?;
        let (chunks, _) = raw.as_chunks::<8>();
        let points = chunks
            .iter()
            .map(|c| {
                [
                    f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                ]
            })
            .collect();
        lines.push(build_line(points, stroke_color, width, geometry, filled)?);
    }
    if !reader.data.is_empty() {
        return Err("The project file contains unexpected data.".to_owned());
    }
    Ok(Document {
        lines,
        palette,
        brush,
    })
}

/// Format 1: the former JSON project files.
#[derive(Deserialize)]
struct LegacyProject {
    format_version: u32,
    palette: [[u8; 3]; 6],
    brush_color: [u8; 3],
    brush_width: f32,
    tool: Tool,
    fill_shapes: bool,
    strokes: Vec<LegacyStroke>,
}

#[derive(Deserialize)]
struct LegacyStroke {
    points: Vec<[f32; 2]>,
    color: [u8; 3],
    width: f32,
    geometry: Geometry,
    filled: bool,
}

fn decode_legacy(bytes: &[u8]) -> Result<Document, String> {
    let file: LegacyProject = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if file.format_version != LEGACY_JSON_VERSION {
        return Err(format!(
            "Unsupported Painty project version: {}.",
            file.format_version
        ));
    }
    if file.strokes.len() > MAX_STROKES {
        return Err(TOO_MANY_STROKES.to_owned());
    }
    let total_points = file.strokes.iter().try_fold(0usize, |total, stroke| {
        total.checked_add(stroke.points.len())
    });
    if total_points.is_none_or(|count| count > MAX_POINTS) {
        return Err(TOO_MANY_POINTS.to_owned());
    }
    check_brush_width(file.brush_width)?;

    let mut lines = Vec::with_capacity(file.strokes.len());
    for stroke in file.strokes {
        lines.push(build_line(
            stroke.points,
            color(stroke.color),
            stroke.width,
            stroke.geometry,
            stroke.filled,
        )?);
    }
    Ok(Document {
        lines,
        palette: file.palette.map(color),
        brush: Brush {
            color: color(file.brush_color),
            width: file.brush_width,
            tool: file.tool,
            fill_shapes: file.fill_shapes,
        },
    })
}

// ---- Shared helpers ----

struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        if len > self.data.len() {
            return Err(TRUNCATED.to_owned());
        }
        let (head, tail) = self.data.split_at(len);
        self.data = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, String> {
        let b = self.bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn rgb(&mut self) -> Result<[u8; 3], String> {
        let b = self.bytes(3)?;
        Ok([b[0], b[1], b[2]])
    }

    fn flag(&mut self) -> Result<bool, String> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("The project file is invalid.".to_owned()),
        }
    }
}

fn check_brush_width(width: f32) -> Result<(), String> {
    if width.is_finite() && (1.0..=80.0).contains(&width) {
        Ok(())
    } else {
        Err("The project contains an invalid brush size.".to_owned())
    }
}

/// Validates one stroke and builds it, whatever the file format it came from.
fn build_line(
    points: Vec<[f32; 2]>,
    color: Color32,
    width: f32,
    geometry: Geometry,
    filled: bool,
) -> Result<Arc<Line>, String> {
    if !width.is_finite() || !(0.1..=2000.0).contains(&width) {
        return Err("The project contains an invalid stroke width.".to_owned());
    }
    if points.iter().any(|point| {
        !point[0].is_finite()
            || !point[1].is_finite()
            || point[0].abs() > 10_000_000.0
            || point[1].abs() > 10_000_000.0
    }) {
        return Err("The project contains invalid drawing coordinates.".to_owned());
    }
    let mut line = Line {
        points: points
            .into_iter()
            .map(|point| Pos2::new(point[0], point[1]))
            .collect(),
        color,
        width,
        geometry,
        filled,
        bounds: Rect::NOTHING,
    };
    line.finish();
    Ok(Arc::new(line))
}

// The codes are part of the file format: never renumber them.
fn tool_code(tool: Tool) -> u8 {
    match tool {
        Tool::Brush => 0,
        Tool::Eraser => 1,
        Tool::Line => 2,
        Tool::Rectangle => 3,
        Tool::Ellipse => 4,
        Tool::Eyedropper => 5,
    }
}

fn tool_from(code: u8) -> Result<Tool, String> {
    Ok(match code {
        0 => Tool::Brush,
        1 => Tool::Eraser,
        2 => Tool::Line,
        3 => Tool::Rectangle,
        4 => Tool::Ellipse,
        5 => Tool::Eyedropper,
        _ => return Err("The project file is invalid.".to_owned()),
    })
}

fn geometry_code(geometry: Geometry) -> u8 {
    match geometry {
        Geometry::Freehand => 0,
        Geometry::Line => 1,
        Geometry::Rectangle => 2,
        Geometry::Ellipse => 3,
    }
}

fn geometry_from(code: u8) -> Result<Geometry, String> {
    Ok(match code {
        0 => Geometry::Freehand,
        1 => Geometry::Line,
        2 => Geometry::Rectangle,
        3 => Geometry::Ellipse,
        _ => return Err("The project file is invalid.".to_owned()),
    })
}

fn rgb(color: Color32) -> [u8; 3] {
    [color.r(), color.g(), color.b()]
}

fn color(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("painty_project_{nonce}.painty"))
    }

    fn sample() -> (Vec<Arc<Line>>, [Color32; 6], Brush) {
        let lines = vec![
            Arc::new(Line::from_points(
                Color32::from_rgb(12, 34, 56),
                7.0,
                vec![Pos2::new(1.0, 2.0), Pos2::new(30.0, 40.0)],
            )),
            Arc::new(Line::shape(
                Geometry::Ellipse,
                Color32::RED,
                3.0,
                true,
                Pos2::new(5.0, 6.0),
                Pos2::new(50.0, 60.0),
            )),
        ];
        let palette = [
            Color32::BLACK,
            Color32::RED,
            Color32::GREEN,
            Color32::BLUE,
            Color32::WHITE,
            Color32::GRAY,
        ];
        let brush = Brush {
            color: Color32::BLUE,
            width: 12.0,
            tool: Tool::Rectangle,
            fill_shapes: true,
        };
        (lines, palette, brush)
    }

    #[test]
    fn project_round_trip_preserves_strokes_palette_and_brush() {
        let path = temp_path();
        let (lines, palette, brush) = sample();

        save(&path, &lines, &palette, &brush).unwrap();
        let loaded = load(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(loaded.lines.len(), 2);
        assert_eq!(loaded.lines[0].points[1], Pos2::new(30.0, 40.0));
        assert_eq!(loaded.lines[0].color, Color32::from_rgb(12, 34, 56));
        assert_eq!(loaded.lines[1].geometry, Geometry::Ellipse);
        assert!(loaded.lines[1].filled);
        assert_eq!(loaded.palette, palette);
        assert_eq!(loaded.brush.color, brush.color);
        assert_eq!(loaded.brush.width, brush.width);
        assert_eq!(loaded.brush.tool, brush.tool);
        assert!(loaded.brush.fill_shapes);
    }

    #[test]
    fn every_tool_and_geometry_code_round_trips() {
        for tool in [
            Tool::Brush,
            Tool::Eraser,
            Tool::Line,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Eyedropper,
        ] {
            assert_eq!(tool_from(tool_code(tool)).unwrap(), tool);
        }
        for geometry in [
            Geometry::Freehand,
            Geometry::Line,
            Geometry::Rectangle,
            Geometry::Ellipse,
        ] {
            assert_eq!(geometry_from(geometry_code(geometry)).unwrap(), geometry);
        }
    }

    #[test]
    fn old_json_projects_still_open() {
        let json = r#"{"format_version":1,
            "palette":[[1,2,3],[0,0,0],[0,0,0],[0,0,0],[0,0,0],[255,255,255]],
            "brush_color":[9,8,7],"brush_width":6.0,"tool":"eraser","fill_shapes":true,
            "strokes":[
              {"points":[[1.0,2.0],[3.0,4.0]],"color":[10,20,30],"width":5.0,
               "geometry":"freehand","filled":false},
              {"points":[[0.0,0.0],[10.0,10.0]],"color":[255,0,0],"width":2.0,
               "geometry":"ellipse","filled":true}]}"#;
        let loaded = decode(json.as_bytes()).unwrap();

        assert_eq!(loaded.lines.len(), 2);
        assert_eq!(loaded.lines[0].points[1], Pos2::new(3.0, 4.0));
        assert_eq!(loaded.lines[0].color, Color32::from_rgb(10, 20, 30));
        assert_eq!(loaded.lines[1].geometry, Geometry::Ellipse);
        assert!(loaded.lines[1].filled);
        assert_eq!(loaded.palette[0], Color32::from_rgb(1, 2, 3));
        assert_eq!(loaded.brush.tool, Tool::Eraser);
        assert!(loaded.brush.fill_shapes);
    }

    #[test]
    fn old_json_projects_are_saved_in_the_new_format() {
        let path = temp_path();
        let json = r#"{"format_version":1,"palette":[[0,0,0],[0,0,0],[0,0,0],[0,0,0],[0,0,0],[0,0,0]],
            "brush_color":[0,0,0],"brush_width":6.0,"tool":"brush","fill_shapes":false,
            "strokes":[{"points":[[1.0,2.0]],"color":[1,1,1],"width":3.0,
            "geometry":"freehand","filled":false}]}"#;
        let old = decode(json.as_bytes()).unwrap();

        save(&path, &old.lines, &old.palette, &old.brush).unwrap();
        let bytes = fs::read(&path).unwrap();
        let reloaded = load(&path).unwrap();
        let _ = fs::remove_file(path);

        assert!(bytes.starts_with(MAGIC));
        assert_eq!(reloaded.lines.len(), 1);
        assert_eq!(reloaded.lines[0].points[0], Pos2::new(1.0, 2.0));
    }

    #[test]
    fn unsupported_json_version_is_rejected() {
        let json = r#"{"format_version":2,"palette":[[0,0,0],[0,0,0],[0,0,0],[0,0,0],[0,0,0],[0,0,0]],"brush_color":[0,0,0],"brush_width":6.0,"tool":"brush","fill_shapes":false,"strokes":[]}"#;
        assert!(matches!(
            decode(json.as_bytes()),
            Err(message) if message.contains("Unsupported Painty project version")
        ));
    }

    #[test]
    fn newer_binary_version_is_rejected() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 64]);
        assert!(matches!(
            decode(&bytes),
            Err(message) if message.contains("Unsupported Painty project version: 3")
        ));
    }

    #[test]
    fn corrupted_or_truncated_files_are_rejected() {
        let (lines, palette, brush) = sample();
        let bytes = encode(&lines, &palette, &brush).unwrap();

        let mut flipped = bytes.clone();
        flipped[HEADER_LEN + 30] ^= 0xFF;
        assert!(matches!(
            decode(&flipped),
            Err(message) if message.contains("corrupted")
        ));
        assert!(decode(&bytes[..bytes.len() - 5]).is_err());
        assert!(decode(&bytes[..HEADER_LEN + 4]).is_err());
        assert!(decode(MAGIC).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_project_is_rejected() {
        assert!(matches!(
            decode(b"hello"),
            Err(message) if message.contains("not a Painty project")
        ));
        assert!(decode(b"").is_err());
    }

    #[test]
    fn invalid_values_are_rejected_even_with_a_valid_checksum() {
        let (lines, palette, mut brush) = sample();
        brush.width = f32::NAN;
        let bytes = encode(&lines, &palette, &brush).unwrap();
        assert!(matches!(
            decode(&bytes),
            Err(message) if message.contains("brush size")
        ));

        let bad = vec![Arc::new(Line::from_points(
            Color32::BLACK,
            3.0,
            vec![Pos2::new(f32::INFINITY, 0.0)],
        ))];
        let (_, palette, brush) = sample();
        let bytes = encode(&bad, &palette, &brush).unwrap();
        assert!(matches!(
            decode(&bytes),
            Err(message) if message.contains("coordinates")
        ));
    }

    #[test]
    fn a_project_over_the_limits_is_refused_when_saving() {
        let (_, palette, brush) = sample();
        let many: Vec<Arc<Line>> = (0..=MAX_STROKES)
            .map(|_| Arc::new(Line::new(Color32::BLACK, 3.0)))
            .collect();
        assert!(matches!(
            encode(&many, &palette, &brush),
            Err(message) if message.contains("too many strokes")
        ));

        let long = vec![Arc::new(Line::from_points(
            Color32::BLACK,
            3.0,
            vec![Pos2::ZERO; MAX_POINTS + 1],
        ))];
        assert!(matches!(
            encode(&long, &palette, &brush),
            Err(message) if message.contains("too many points")
        ));
    }

    #[test]
    fn saving_over_an_existing_project_replaces_it_and_leaves_no_temp_file() {
        let path = temp_path();
        let (lines, palette, brush) = sample();
        save(&path, &lines, &palette, &brush).unwrap();
        save(&path, &lines[..1], &palette, &brush).unwrap();
        let loaded = load(&path).unwrap();
        let temp_exists = path.with_extension("painty.tmp").exists();
        let _ = fs::remove_file(path);

        assert_eq!(loaded.lines.len(), 1);
        assert!(!temp_exists);
    }

    #[test]
    fn binary_files_are_much_smaller_than_the_old_json() {
        let points: Vec<Pos2> = (0..1000)
            .map(|i| Pos2::new(i as f32, 2.0 * i as f32))
            .collect();
        let lines = vec![Arc::new(Line::from_points(Color32::BLACK, 3.0, points))];
        let (_, palette, brush) = sample();
        let bytes = encode(&lines, &palette, &brush).unwrap();
        assert!(bytes.len() < 8_200);
    }
}
