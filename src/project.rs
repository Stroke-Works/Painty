//! Native editable Painty project files.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use eframe::egui::{Color32, Pos2, Rect};
use serde::{Deserialize, Serialize};

use crate::model::{Brush, Geometry, Line, Tool};

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;
const MAX_STROKES: usize = 100_000;
const MAX_POINTS: usize = 2_000_000;

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    format_version: u32,
    palette: [[u8; 3]; 6],
    brush_color: [u8; 3],
    brush_width: f32,
    tool: Tool,
    fill_shapes: bool,
    strokes: Vec<StoredLine>,
}

#[derive(Serialize, Deserialize)]
struct StoredLine {
    points: Vec<[f32; 2]>,
    color: [u8; 3],
    width: f32,
    geometry: Geometry,
    filled: bool,
}

pub struct Document {
    pub lines: Vec<Arc<Line>>,
    pub palette: [Color32; 6],
    pub brush: Brush,
}

pub fn save(
    path: &Path,
    lines: &[Arc<Line>],
    palette: &[Color32; 6],
    brush: &Brush,
) -> Result<(), String> {
    let file = ProjectFile {
        format_version: FORMAT_VERSION,
        palette: palette.map(rgb),
        brush_color: rgb(brush.color),
        brush_width: brush.width,
        tool: brush.tool,
        fill_shapes: brush.fill_shapes,
        strokes: lines
            .iter()
            .map(|line| StoredLine {
                points: line.points.iter().map(|p| [p.x, p.y]).collect(),
                color: rgb(line.color),
                width: line.width,
                geometry: line.geometry,
                filled: line.filled,
            })
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err("The project is too large to save.".to_owned());
    }
    fs::write(path, bytes).map_err(|error| error.to_string())
}

pub fn load(path: &Path) -> Result<Document, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err("The project file is too large.".to_owned());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let file: ProjectFile = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if file.format_version != FORMAT_VERSION {
        return Err(format!(
            "Unsupported Painty project version: {}.",
            file.format_version
        ));
    }
    if file.strokes.len() > MAX_STROKES {
        return Err("The project contains too many strokes.".to_owned());
    }
    let total_points = file.strokes.iter().try_fold(0usize, |total, stroke| {
        total.checked_add(stroke.points.len())
    });
    if total_points.is_none_or(|count| count > MAX_POINTS) {
        return Err("The project contains too many points.".to_owned());
    }
    if !file.brush_width.is_finite() || !(1.0..=80.0).contains(&file.brush_width) {
        return Err("The project contains an invalid brush size.".to_owned());
    }

    let mut lines = Vec::with_capacity(file.strokes.len());
    for stroke in file.strokes {
        if !stroke.width.is_finite() || !(0.1..=2000.0).contains(&stroke.width) {
            return Err("The project contains an invalid stroke width.".to_owned());
        }
        if stroke.points.iter().any(|point| {
            !point[0].is_finite()
                || !point[1].is_finite()
                || point[0].abs() > 10_000_000.0
                || point[1].abs() > 10_000_000.0
        }) {
            return Err("The project contains invalid drawing coordinates.".to_owned());
        }
        let points = stroke
            .points
            .into_iter()
            .map(|point| Pos2::new(point[0], point[1]))
            .collect();
        let mut line = Line {
            points,
            color: color(stroke.color),
            width: stroke.width,
            geometry: stroke.geometry,
            filled: stroke.filled,
            bounds: Rect::NOTHING,
        };
        line.finish();
        lines.push(Arc::new(line));
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

    #[test]
    fn project_round_trip_preserves_strokes_palette_and_brush() {
        let path = temp_path();
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

        save(&path, &lines, &palette, &brush).unwrap();
        let loaded = load(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(loaded.lines.len(), 2);
        assert_eq!(loaded.lines[0].points[1], Pos2::new(30.0, 40.0));
        assert_eq!(loaded.lines[1].geometry, Geometry::Ellipse);
        assert!(loaded.lines[1].filled);
        assert_eq!(loaded.palette, palette);
        assert_eq!(loaded.brush.color, brush.color);
        assert_eq!(loaded.brush.width, brush.width);
        assert_eq!(loaded.brush.tool, brush.tool);
        assert!(loaded.brush.fill_shapes);
    }

    #[test]
    fn unsupported_project_version_is_rejected() {
        let path = temp_path();
        fs::write(
            &path,
            r#"{"format_version":2,"palette":[[0,0,0],[0,0,0],[0,0,0],[0,0,0],[0,0,0],[0,0,0]],"brush_color":[0,0,0],"brush_width":6.0,"tool":"brush","fill_shapes":false,"strokes":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            load(&path),
            Err(message) if message.contains("Unsupported Painty project version")
        ));
        let _ = fs::remove_file(path);
    }
}
