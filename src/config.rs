use eframe::egui;
use std::{env, fs, path::PathBuf};

use crate::i18n::Lang;
use crate::theme::{self, CanvasColor, Mode};

/// User settings, saved between sessions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Config {
    /// Interface theme chosen in the settings. `None` follows the system theme.
    pub theme: Option<Mode>,
    /// Background of the drawing area, independent from the interface theme.
    pub canvas: CanvasColor,
    pub language: Lang,
    /// Look for a new version each time the app starts.
    pub auto_update: bool,
    pub palette: [egui::Color32; 6],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: None,
            canvas: CanvasColor::Light,
            language: Lang::default(),
            auto_update: true,
            palette: theme::palette(theme::INK_LIGHT),
        }
    }
}

/// `%APPDATA%\Painty\settings.txt`
fn path() -> Option<PathBuf> {
    env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Painty").join("settings.txt"))
}

/// Reads the `key=value` settings file. Unknown keys are ignored, and settings
/// from version 0.1.x are migrated (`canvas_follows_theme` becomes `canvas`).
fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    let mut legacy_follow = None;
    let mut canvas_set = false;

    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("theme", "system") => cfg.theme = None,
            ("theme", "dark") => cfg.theme = Some(Mode::Dark),
            ("theme", "light") => cfg.theme = Some(Mode::Light),
            ("canvas", "light") => {
                cfg.canvas = CanvasColor::Light;
                canvas_set = true;
            }
            ("canvas", "dark") => {
                cfg.canvas = CanvasColor::Dark;
                canvas_set = true;
            }
            ("canvas_follows_theme", v) => legacy_follow = Some(v == "true"),
            ("language", v) => {
                if let Some(lang) = Lang::from_code(v) {
                    cfg.language = lang;
                }
            }
            // Anything but an explicit "false" keeps the default.
            ("auto_update", v) => cfg.auto_update = v != "false",
            ("palette", v) => {
                if let Some(palette) = parse_palette(v) {
                    cfg.palette = palette;
                }
            }
            _ => {}
        }
    }

    // The old setting only said whether the canvas followed the theme. Resolve
    // it once the whole file has been read, whatever the line order.
    if !canvas_set {
        if legacy_follow == Some(true) {
            cfg.canvas = cfg.theme.unwrap_or_else(theme::system_mode).canvas();
        } else {
            cfg.canvas = CanvasColor::Light;
        }
    }

    cfg
}

fn parse_palette(value: &str) -> Option<[egui::Color32; 6]> {
    let colors = value
        .split(',')
        .map(|hex| {
            if hex.len() != 6 || !hex.is_ascii() {
                return None;
            }
            Some(egui::Color32::from_rgb(
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    colors.try_into().ok()
}

/// Loads the settings, or the defaults when the file is missing or unreadable.
pub fn load() -> Config {
    path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// Writes the settings to disk. Errors are ignored: settings are not critical.
pub fn save(cfg: &Config) {
    let Some(path) = path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let theme = match cfg.theme {
        Some(Mode::Dark) => "dark",
        Some(Mode::Light) => "light",
        None => "system",
    };
    let canvas = match cfg.canvas {
        CanvasColor::Light => "light",
        CanvasColor::Dark => "dark",
    };
    let palette = cfg
        .palette
        .iter()
        .map(|color| format!("{:02x}{:02x}{:02x}", color.r(), color.g(), color.b()))
        .collect::<Vec<_>>()
        .join(",");
    let text = format!(
        "theme={theme}\ncanvas={canvas}\nlanguage={}\nauto_update={}\npalette={palette}\n",
        cfg.language.code(),
        cfg.auto_update
    );
    let _ = fs::write(path, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_settings_are_read() {
        let cfg = parse("theme=dark\ncanvas=light\nlanguage=fr\n");
        assert_eq!(cfg.theme, Some(Mode::Dark));
        assert_eq!(cfg.canvas, CanvasColor::Light);
        assert_eq!(cfg.language, Lang::Fr);
    }

    #[test]
    fn old_settings_are_migrated() {
        let cfg = parse("theme=dark\ncanvas_follows_theme=true\nlanguage=fr\n");
        assert_eq!(cfg.theme, Some(Mode::Dark));
        assert_eq!(cfg.canvas, CanvasColor::Dark);
        assert_eq!(cfg.language, Lang::Fr);
    }

    #[test]
    fn old_white_canvas_stays_light() {
        let cfg = parse("theme=light\ncanvas_follows_theme=false\n");
        assert_eq!(cfg.canvas, CanvasColor::Light);
        assert_eq!(cfg.theme, Some(Mode::Light));
    }

    #[test]
    fn auto_update_is_on_unless_turned_off() {
        assert!(parse("").auto_update);
        assert!(parse("auto_update=true\n").auto_update);
        assert!(parse("auto_update=nonsense\n").auto_update);
        assert!(!parse("auto_update=false\n").auto_update);
    }

    #[test]
    fn missing_theme_follows_system() {
        let cfg = parse("canvas=dark\nlanguage=en\n");
        assert_eq!(cfg.theme, None);
        assert_eq!(cfg.canvas, CanvasColor::Dark);
    }

    #[test]
    fn custom_palette_is_parsed_and_invalid_values_are_ignored() {
        let cfg = parse("palette=010203,040506,070809,0a0b0c,0d0e0f,101112\n");
        assert_eq!(cfg.palette[0], egui::Color32::from_rgb(1, 2, 3));
        assert_eq!(cfg.palette[5], egui::Color32::from_rgb(16, 17, 18));
        assert_eq!(
            parse("palette=invalid\n").palette,
            Config::default().palette
        );
        assert_eq!(
            parse("palette=0é000,040506,070809,0a0b0c,0d0e0f,101112\n").palette,
            Config::default().palette
        );
    }
}
