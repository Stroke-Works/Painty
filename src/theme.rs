//! Colour tokens, typography and egui styling shared by every screen.
//!
//! The palette below comes from `IMPLEMENTATION.md`: the dark and light themes
//! use the same roles, so a control only reads from [`Colors`] and never from a
//! hard-coded value.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Rounding, Shadow,
    Stroke, TextStyle, Ui, Visuals,
};

/// Default ink on a light canvas.
pub const INK_LIGHT: Color32 = Color32::from_rgb(20, 20, 20);
/// Default ink on a dark canvas.
pub const INK_DARK: Color32 = Color32::from_rgb(242, 242, 243);

/// Interface theme chosen in the settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

impl Mode {
    /// The canvas colour that goes with this interface theme.
    pub fn canvas(self) -> CanvasColor {
        match self {
            Mode::Light => CanvasColor::Light,
            Mode::Dark => CanvasColor::Dark,
        }
    }

    /// All the colours used by the interface. `canvas` is independent from the
    /// interface theme, as chosen in the settings.
    pub fn colors(self, canvas: CanvasColor) -> Colors {
        match self {
            Mode::Light => Colors {
                dark: false,
                window_bg: Color32::from_rgb(0xf4, 0xf4, 0xf5),
                surface: Color32::WHITE,
                surface_alt: Color32::WHITE,
                border: Color32::from_rgb(0xe4, 0xe4, 0xe7),
                row: Color32::from_rgb(0xf4, 0xf4, 0xf5),
                track: Color32::from_rgb(0xe9, 0xe9, 0xec),
                active: Color32::WHITE,
                hover: Color32::from_rgb(0xec, 0xec, 0xef),
                field: Color32::from_rgb(0xf4, 0xf4, 0xf5),
                text: Color32::from_rgb(0x18, 0x18, 0x1b),
                // Darkened from #71717a: that value gave 4.4:1 on the row colour and
                // 4.0:1 on the segmented track, below the 4.5:1 required.
                text_dim: Color32::from_rgb(0x62, 0x62, 0x6b),
                separator: Color32::from_rgb(0xd4, 0xd4, 0xd8),
                accent: Color32::from_rgb(0x25, 0x63, 0xeb),
                primary_bg: Color32::from_rgb(0x18, 0x18, 0x1b),
                primary_fg: Color32::WHITE,
                danger: Color32::from_rgb(0xdc, 0x26, 0x26),
                danger_hover: Color32::from_rgb(0xb9, 0x1c, 0x1c),
                success: Color32::from_rgb(0x15, 0x80, 0x3d),
                knob: Color32::WHITE,
                knob_border: Color32::from_rgb(0xa1, 0xa1, 0xaa),
                canvas_bg: canvas.background(),
                ink: canvas.ink(),
            },
            Mode::Dark => Colors {
                dark: true,
                window_bg: Color32::from_rgb(0x0f, 0x0f, 0x10),
                surface: Color32::from_rgb(0x1c, 0x1c, 0x1e),
                surface_alt: Color32::from_rgb(0x1a, 0x1a, 0x1c),
                border: Color32::from_rgb(0x2a, 0x2a, 0x2d),
                row: Color32::from_rgb(0x23, 0x23, 0x26),
                track: Color32::from_rgb(0x16, 0x16, 0x18),
                active: Color32::from_rgb(0x3a, 0x3a, 0x3e),
                hover: Color32::from_rgb(0x2a, 0x2a, 0x2d),
                field: Color32::from_rgb(0x2a, 0x2a, 0x2d),
                text: Color32::from_rgb(0xf2, 0xf2, 0xf3),
                text_dim: Color32::from_rgb(0x9a, 0x9a, 0xa0),
                separator: Color32::from_rgb(0x3a, 0x3a, 0x3e),
                accent: Color32::from_rgb(0x3b, 0x82, 0xf6),
                primary_bg: Color32::from_rgb(0xf2, 0xf2, 0xf3),
                primary_fg: Color32::from_rgb(0x0f, 0x0f, 0x10),
                danger: Color32::from_rgb(0xc0, 0x39, 0x2b),
                danger_hover: Color32::from_rgb(0xa9, 0x32, 0x26),
                success: Color32::from_rgb(0x6f, 0xcf, 0x97),
                knob: Color32::from_rgb(0xf2, 0xf2, 0xf3),
                knob_border: Color32::from_rgb(0x0f, 0x0f, 0x10),
                canvas_bg: canvas.background(),
                ink: canvas.ink(),
            },
        }
    }
}

/// Background of the drawing area, chosen in the settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CanvasColor {
    #[default]
    Light,
    Dark,
}

impl CanvasColor {
    pub fn background(self) -> Color32 {
        match self {
            CanvasColor::Light => Color32::WHITE,
            CanvasColor::Dark => Color32::from_rgb(0x14, 0x14, 0x16),
        }
    }

    pub fn ink(self) -> Color32 {
        match self {
            CanvasColor::Light => INK_LIGHT,
            CanvasColor::Dark => INK_DARK,
        }
    }
}

/// Every colour used by the interface for the current theme.
#[derive(Clone, Copy)]
pub struct Colors {
    pub dark: bool,
    /// Window background around the canvas.
    pub window_bg: Color32,
    /// Cards, popups and the toolbar.
    pub surface: Color32,
    /// Settings card, slightly darker than the other popups in the dark theme.
    pub surface_alt: Color32,
    /// Border around surfaces.
    pub border: Color32,
    /// Setting rows and secondary buttons.
    pub row: Color32,
    /// Segmented-control track.
    pub track: Color32,
    /// Selected segment.
    pub active: Color32,
    /// Hovered ghost button or row.
    pub hover: Color32,
    /// Text field background.
    pub field: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub separator: Color32,
    pub accent: Color32,
    pub primary_bg: Color32,
    pub primary_fg: Color32,
    pub danger: Color32,
    pub danger_hover: Color32,
    pub success: Color32,
    /// Slider thumb.
    pub knob: Color32,
    pub knob_border: Color32,
    /// Background of the drawing area.
    pub canvas_bg: Color32,
    /// Default ink for the canvas background.
    pub ink: Color32,
}

impl Colors {
    /// Dimming layer behind a modal popup: 50 % black in dark mode, 25 % in light mode.
    pub fn scrim(self) -> Color32 {
        Color32::from_black_alpha(if self.dark { 128 } else { 64 })
    }

    /// Border of the selected segment.
    pub fn segment_border(self) -> Color32 {
        if self.dark {
            self.border
        } else {
            Color32::from_rgb(0xd9, 0xd9, 0xdf)
        }
    }
}

/// Colour swatches. The first one is the ink, which is black or white depending on the canvas.
pub fn palette(ink: Color32) -> [Color32; 6] {
    [
        ink,
        Color32::from_rgb(0xdc, 0x26, 0x26),
        Color32::from_rgb(0xf9, 0x73, 0x16),
        Color32::from_rgb(0x16, 0xa3, 0x4a),
        Color32::from_rgb(0x25, 0x63, 0xeb),
        Color32::from_rgb(0x93, 0x33, 0xea),
    ]
}

/// Inter SemiBold, used for popup titles and primary buttons.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(Arc::from("Inter SemiBold")))
}

/// Loads Inter (with the system font as fallback) and sets readable text sizes.
pub fn setup(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "inter".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
    );
    fonts.font_data.insert(
        "inter_semibold".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
    );

    // Inter first, then egui's built-in fonts as the system-ui fallback.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    fonts.families.insert(
        FontFamily::Name(Arc::from("Inter SemiBold")),
        vec!["inter_semibold".to_owned(), "inter".to_owned()],
    );

    ctx.set_fonts(fonts);

    ctx.style_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, semibold(18.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Small, FontId::proportional(13.0)),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

/// Applies the theme to the current `Ui` and its children.
pub fn apply(ui: &mut Ui, c: &Colors) {
    let mut v = if c.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    v.window_fill = c.surface_alt;
    v.panel_fill = c.window_bg;
    v.window_stroke = Stroke::new(1.0_f32, c.border);
    v.window_rounding = Rounding::same(16.0);
    v.menu_rounding = Rounding::same(12.0);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.extreme_bg_color = c.field;
    v.faint_bg_color = c.row;
    v.selection.bg_fill = c.accent;
    // No outline around focused text fields or selected items.
    v.selection.stroke = Stroke::NONE;
    v.hyperlink_color = c.accent;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.border);

    let w = &mut v.widgets;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = c.row;
    w.hovered.weak_bg_fill = c.hover;
    w.hovered.bg_fill = c.hover;
    w.active.weak_bg_fill = c.hover;
    w.active.bg_fill = c.hover;

    for s in [&mut w.inactive, &mut w.hovered, &mut w.active] {
        s.rounding = Rounding::same(10.0);
        s.bg_stroke = Stroke::NONE;
        s.fg_stroke = Stroke::new(1.0_f32, c.text);
    }

    ui.style_mut().visuals = v;
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
    ui.spacing_mut().button_padding = egui::vec2(12.0, 8.0);
    ui.spacing_mut().interact_size = egui::vec2(32.0, 32.0);
}

/// Reads the Windows app theme. Falls back to the light theme when the
/// registry value is unavailable.
#[cfg(windows)]
pub fn system_mode() -> Mode {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    /// Hides the console window that `reg.exe` would otherwise flash.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.split_whitespace().last() == Some("0x0") {
                Mode::Dark
            } else {
                Mode::Light
            }
        }
        _ => Mode::Light,
    }
}

/// Other platforms do not expose a system theme here; use the light theme.
#[cfg(not(windows))]
pub fn system_mode() -> Mode {
    Mode::Light
}
