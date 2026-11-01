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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: None,
            canvas: CanvasColor::Light,
            language: Lang::default(),
            auto_update: true,
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
    let text = format!(
        "theme={theme}\ncanvas={canvas}\nlanguage={}\nauto_update={}\n",
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
}
