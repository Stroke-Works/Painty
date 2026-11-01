use eframe::egui::{self, Color32, Frame, Key, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::canvas::Canvas;
use crate::config::{self, Config};
use crate::dialog;
use crate::export::{self, Format};
use crate::i18n::{self, t, tf, Lang};
use crate::model::Brush;
use crate::project::{self, Document};
use crate::settings;
use crate::theme::{self, CanvasColor, Colors, Mode};
use crate::toolbar::{self, Action};
use crate::updater::{self, Status, Updater};
use crate::widgets::{self, Kind};

/// Export options chosen in the export popup.
#[derive(Clone)]
struct ExportForm {
    name: String,
    format: Format,
}

impl Default for ExportForm {
    fn default() -> Self {
        Self {
            name: "painty".to_owned(),
            format: Format::Png,
        }
    }
}

/// The single modal dialog shown at a time. `Update` stays status-driven (see `show_update_prompt`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Modal {
    None,
    Settings,
    ClearConfirm,
    Export,
    OpenConfirm,
    ReleaseNotes,
    Error,
    Quit,
}

/// True on the first frame a dialog is open, used to put focus on its safe action.
fn opening_frame(ctx: &egui::Context, id: &str, open: bool) -> bool {
    let key = egui::Id::new(("dialog_opening", id));
    let was_open = ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    ctx.data_mut(|d| d.insert_temp(key, open));
    open && !was_open
}

/// Main application state, updated every frame by egui.
pub struct Painty {
    canvas: Canvas,
    brush: Brush,
    palette: [Color32; 6],
    project_path: Option<PathBuf>,
    pending_project: Option<(PathBuf, Document)>,
    mode: Mode,
    /// Background of the drawing area, independent from the interface theme.
    canvas_color: CanvasColor,
    /// True when the theme was chosen in the settings; false follows the system.
    theme_override: bool,
    /// Last time the system theme was read, while following it.
    last_system_check: Instant,
    language: Lang,
    /// Look for a new version when the app starts.
    auto_update: bool,
    /// Ink color used on the previous frame, to detect a background change.
    last_ink: Option<Color32>,
    /// Settings as last written to disk, to avoid saving every frame.
    last_saved: Config,
    /// The modal dialog currently requested.
    modal: Modal,
    /// Kept outside `Modal` so the dialog content can still be drawn while it fades out.
    export_form: ExportForm,
    /// Message shown by the error dialog, kept for the same reason.
    error_text: String,
    /// True once the window may close without asking (user confirmed or update restart).
    close_confirmed: bool,
    updater: Updater,
    /// True once the automatic update check has been started.
    update_checked: bool,
    /// True once the window has been centered on the monitor.
    centered: bool,
    release_notes_cache: CommonMarkCache,
}

impl Painty {
    /// Loads the saved settings and builds the initial state.
    pub fn new(open_path: Option<PathBuf>) -> Self {
        // Remove files left over from the previous update.
        updater::cleanup();

        let cfg = config::load();
        i18n::set_lang(cfg.language);
        let mode = cfg.theme.unwrap_or_else(theme::system_mode);
        let ink = mode.colors(cfg.canvas).ink;
        let mut palette = cfg.palette;
        if palette == theme::palette(theme::INK_LIGHT) {
            palette[0] = ink;
        }

        let brush = Brush {
            color: ink,
            ..Brush::default()
        };

        let mut app = Self {
            canvas: Canvas::default(),
            brush,
            palette,
            project_path: None,
            pending_project: None,
            mode,
            canvas_color: cfg.canvas,
            theme_override: cfg.theme.is_some(),
            last_system_check: Instant::now(),
            language: cfg.language,
            auto_update: cfg.auto_update,
            last_ink: Some(ink),
            last_saved: cfg,
            modal: Modal::None,
            export_form: ExportForm::default(),
            error_text: String::new(),
            close_confirmed: false,
            updater: Updater::default(),
            update_checked: false,
            centered: false,
            release_notes_cache: CommonMarkCache::default(),
        };
        if let Some(path) = open_path {
            match project::load(&path) {
                Ok(document) => app.apply_project(path, document),
                Err(error) => {
                    app.error_text = error;
                    app.modal = Modal::Error;
                }
            }
        }
        app
    }

    /// Closes `modal`, unless another dialog has taken its place in the meantime.
    fn close(&mut self, modal: Modal) {
        if self.modal == modal {
            self.modal = Modal::None;
        }
    }

    /// Opens the export popup, unless there is nothing to draw on yet.
    fn open_export(&mut self) {
        if self.canvas.rect().is_some() {
            self.export_form = ExportForm::default();
            self.modal = Modal::Export;
        }
    }

    /// Asks where to save, then writes the drawing in the chosen format.
    /// Returns the error message if the export failed.
    fn save_export(
        &mut self,
        ctx: &egui::Context,
        form: ExportForm,
        bg: Color32,
    ) -> Option<String> {
        let rect = self.canvas.rect()?;
        // Export the whole drawing, even when it no longer fits in the window.
        // An empty canvas falls back to the visible area.
        let (origin, size) = export::bounds(self.canvas.lines())
            .map(|bounds| (bounds.min, bounds.size()))
            .unwrap_or((rect.min, rect.size()));

        // Remove characters that are not allowed in file names.
        let name: String = form
            .name
            .trim()
            .chars()
            .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
            .collect();
        let name = if name.is_empty() {
            "painty".to_owned()
        } else {
            name
        };
        let ext = form.format.ext();

        // Dialog cancelled: nothing to do.
        let mut path = rfd::FileDialog::new()
            .add_filter(form.format.label(), &[ext])
            .set_file_name(format!("{name}.{ext}"))
            .save_file()?;

        // Add the extension if the user typed a name without the right one.
        let has_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext));
        if !has_ext {
            let mut os = path.into_os_string();
            os.push(format!(".{ext}"));
            path = os.into();
        }

        export::save(
            &path,
            form.format,
            self.canvas.lines(),
            origin,
            size,
            bg,
            ctx.pixels_per_point(),
        )
        .err()
    }

    fn save_project(&mut self) {
        let path = self.project_path.clone().or_else(|| {
            rfd::FileDialog::new()
                .add_filter(t("project.file_type"), &["painty"])
                .set_file_name("drawing.painty")
                .save_file()
        });
        let Some(mut path) = path else {
            return;
        };
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("painty"))
        {
            path.set_extension("painty");
        }
        match project::save(&path, self.canvas.lines(), &self.palette, &self.brush) {
            Ok(()) => self.project_path = Some(path),
            Err(error) => {
                self.error_text = error;
                self.modal = Modal::Error;
            }
        }
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(t("project.file_type"), &["painty"])
            .pick_file()
        else {
            return;
        };
        match project::load(&path) {
            Ok(document) if self.canvas.is_empty() => self.apply_project(path, document),
            Ok(document) => {
                self.pending_project = Some((path, document));
                self.modal = Modal::OpenConfirm;
            }
            Err(error) => {
                self.error_text = error;
                self.modal = Modal::Error;
            }
        }
    }

    fn apply_project(&mut self, path: PathBuf, document: Document) {
        self.canvas.replace_lines(document.lines);
        self.palette = document.palette;
        self.brush = document.brush;
        self.project_path = Some(path);
    }

    /// True while the update prompt is (or should be) on screen.
    fn update_prompt_open(&self) -> bool {
        matches!(
            self.updater.status,
            Status::Available(_) | Status::Downloading(_) | Status::InstallFailed(_)
        )
    }

    // ---- Per-frame steps, called in order by `update` ----

    /// Centers the window once, at a reasonable size (at most 900x600, 70 % of the screen).
    fn center_window(&mut self, ctx: &egui::Context) {
        if self.centered {
            return;
        }
        let monitor = ctx.input(|i| i.viewport().monitor_size);
        if let Some(monitor) = monitor {
            let size = egui::vec2(900.0, 600.0).min(monitor * 0.7);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                ((monitor - size) * 0.5).to_pos2(),
            ));
            self.centered = true;
        }
    }

    /// Silent update check on the first frame, unless turned off in the settings.
    /// Errors are not shown.
    fn start_update_check(&mut self, ctx: &egui::Context) {
        if self.update_checked {
            return;
        }
        self.update_checked = true;
        if self.auto_update {
            self.updater.check(ctx, false);
        }
    }

    /// Focus rings only appear for keyboard navigation, never after a mouse click.
    fn track_input_mode(ctx: &egui::Context) {
        let (key_pressed, pointer_pressed) = ctx.input(|i| {
            (
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key {
                            pressed: true,
                            key: Key::Tab
                                | Key::ArrowLeft
                                | Key::ArrowRight
                                | Key::ArrowUp
                                | Key::ArrowDown,
                            ..
                        }
                    )
                }),
                i.pointer.any_pressed(),
            )
        });
        if key_pressed {
            widgets::set_keyboard_mode(ctx, true);
        } else if pointer_pressed {
            widgets::set_keyboard_mode(ctx, false);
        }
    }

    /// Asks before closing when the drawing would be lost. A restart after an
    /// update bypasses the question: the new version is already running.
    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested {
            if !self.close_confirmed && !self.canvas.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.modal = Modal::Quit;
            }
        } else {
            self.close_confirmed = false;
        }
        if self.updater.restart {
            self.updater.restart = false;
            self.close_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Follows the Windows theme while no explicit choice was saved. The
    /// periodic repaint also notices a theme change made outside the app.
    fn follow_system_theme(&mut self, ctx: &egui::Context) {
        if self.theme_override {
            return;
        }
        ctx.request_repaint_after(Duration::from_secs(2));
        if self.last_system_check.elapsed() >= Duration::from_secs(2) {
            self.last_system_check = Instant::now();
            self.mode = theme::system_mode();
        }
    }

    /// When the canvas background changes, the default ink follows (black <-> white).
    fn follow_ink(&mut self, ink: Color32) {
        if let Some(prev) = self.last_ink {
            if prev != ink && self.brush.color == prev {
                self.brush.color = ink;
            }
            if self.palette[0] == prev {
                self.palette[0] = ink;
            }
        }
        self.last_ink = Some(ink);
    }

    /// Keyboard shortcuts, unless a dialog is open.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.modal != Modal::None || self.update_prompt_open() {
            return;
        }
        let (cmd, shift, undo, redo, save, open, reset_view) = ctx.input(|i| {
            (
                i.modifiers.command,
                i.modifiers.shift,
                i.key_pressed(Key::Z),
                i.key_pressed(Key::Y),
                i.key_pressed(Key::S),
                i.key_pressed(Key::O),
                i.key_pressed(Key::Num0),
            )
        });
        if cmd && undo {
            // Ctrl+Shift+Z redoes, like Ctrl+Y.
            if shift {
                self.canvas.redo();
            } else {
                self.canvas.undo();
            }
        }
        if cmd && redo {
            self.canvas.redo();
        }
        if cmd && save {
            if shift {
                self.open_export();
            } else {
                self.save_project();
            }
        }
        if cmd && open {
            self.open_project();
        }
        if cmd && reset_view {
            self.canvas.reset_view();
        }
    }

    /// Floating toolbar and the action it returns.
    fn show_toolbar(&mut self, ctx: &egui::Context, c: &Colors) {
        match toolbar::show(ctx, &mut self.brush, &mut self.palette, c) {
            Some(Action::OpenProject) => self.open_project(),
            Some(Action::SaveProject) => self.save_project(),
            Some(Action::Undo) => self.canvas.undo(),
            Some(Action::Clear) => {
                // Nothing to clear: skip the confirmation.
                if !self.canvas.is_empty() {
                    self.modal = Modal::ClearConfirm;
                }
            }
            Some(Action::Settings) => {
                self.modal = if self.modal == Modal::Settings {
                    Modal::None
                } else {
                    Modal::Settings
                };
            }
            Some(Action::Export) => self.open_export(),
            None => {}
        }
    }

    /// Settings dialog. It keeps its own open flag, synced with `Modal`.
    fn show_settings(&mut self, ctx: &egui::Context, c: &Colors) {
        let mut settings_open = self.modal == Modal::Settings;
        let previous_mode = self.mode;
        let show_release_notes = settings::show(
            ctx,
            &mut settings_open,
            &mut self.mode,
            &mut self.canvas_color,
            &mut self.language,
            &mut self.auto_update,
            &mut self.updater,
            c,
        );
        if show_release_notes {
            self.modal = Modal::ReleaseNotes;
        }
        // Picking a theme stops following the system, and is saved.
        if self.mode != previous_mode {
            self.theme_override = true;
        }
        if !settings_open {
            self.close(Modal::Settings);
        }
    }

    fn show_release_notes(&mut self, ctx: &egui::Context, c: &Colors) {
        let open = self.modal == Modal::ReleaseNotes;
        let mut close = false;
        let dismissed = dialog::show(ctx, "release_notes", open, c, 600.0, c.surface, |ui| {
            widgets::title(ui, c, t("settings.release_notes"));
            ui.add_space(12.0);
            CommonMarkViewer::new().show(
                ui,
                &mut self.release_notes_cache,
                include_str!("../RELEASE_NOTES.md"),
            );
            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = widgets::button(
                    ui,
                    "release_notes_close",
                    c,
                    t("button.close"),
                    Kind::Primary,
                    100.0,
                )
                .clicked();
            });
        });
        if close || dismissed {
            self.modal = Modal::Settings;
        }
    }

    /// Persists settings as soon as they change.
    fn save_settings_if_changed(&mut self) {
        let current = Config {
            theme: self.theme_override.then_some(self.mode),
            canvas: self.canvas_color,
            language: self.language,
            auto_update: self.auto_update,
            palette: self.palette,
        };
        if current != self.last_saved {
            config::save(&current);
            self.last_saved = current;
        }
    }

    fn show_open_confirm(&mut self, ctx: &egui::Context, c: &Colors) {
        let mut replace = false;
        let mut cancel = false;
        let open = self.modal == Modal::OpenConfirm;
        let dismissed = dialog::show(ctx, "confirm_open", open, c, 380.0, c.surface, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            widgets::title(ui, c, t("dialog.open.title"));
            ui.add_space(10.0);
            ui.label(
                RichText::new(t("dialog.open.body"))
                    .size(14.0)
                    .color(c.text_dim),
            );
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if widgets::button(ui, "open_cancel", c, t("button.cancel"), Kind::Soft, 161.0)
                    .clicked()
                {
                    cancel = true;
                }
                if widgets::button(
                    ui,
                    "open_replace",
                    c,
                    t("button.replace"),
                    Kind::Danger,
                    161.0,
                )
                .clicked()
                {
                    replace = true;
                }
            });
        });
        if replace {
            if let Some((path, document)) = self.pending_project.take() {
                self.apply_project(path, document);
            }
        }
        if replace || cancel || dismissed {
            self.pending_project = None;
            self.close(Modal::OpenConfirm);
        }
    }

    /// "Delete drawing" confirmation dialog: 340 px card, safe action focused first.
    fn show_clear_dialog(&mut self, ctx: &egui::Context, c: &Colors) {
        let mut confirm = false;
        let mut cancel = false;
        let open = self.modal == Modal::ClearConfirm;
        let focus_cancel = opening_frame(ctx, "confirm_clear", open);
        let dismissed = dialog::show(ctx, "confirm_clear", open, c, 340.0, c.surface, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            widgets::title(ui, c, t("dialog.clear.title"));
            ui.add_space(10.0);
            ui.label(
                RichText::new(t("dialog.clear.body"))
                    .size(14.0)
                    .color(c.text_dim),
            );
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let keep = widgets::button(ui, "keep", c, t("button.cancel"), Kind::Soft, 141.0);
                if focus_cancel {
                    keep.request_focus();
                }
                if keep.clicked() {
                    cancel = true;
                }
                if widgets::button(ui, "wipe", c, t("button.delete"), Kind::Danger, 141.0).clicked()
                {
                    confirm = true;
                }
            });
        });

        if confirm {
            self.canvas.clear();
        }
        if confirm || cancel || dismissed {
            self.close(Modal::ClearConfirm);
        }
    }

    /// Export popup: name and format. The file location is asked afterwards.
    fn show_export_dialog(&mut self, ctx: &egui::Context, c: &Colors) {
        let mut go = false;
        let mut cancel = false;
        let open = self.modal == Modal::Export;
        let dismissed = dialog::show(ctx, "export_form", open, c, 380.0, c.surface, |ui| {
            let form = &mut self.export_form;
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

            widgets::title(ui, c, t("dialog.export.title"));
            ui.add_space(18.0);

            ui.label(RichText::new(t("export.name")).size(13.0).color(c.text_dim));
            ui.add_space(14.0);
            widgets::text_field(ui, "export_name", c, &mut form.name, "painty");
            ui.add_space(14.0);

            ui.label(
                RichText::new(t("export.format"))
                    .size(13.0)
                    .color(c.text_dim),
            );
            ui.add_space(14.0);
            widgets::segmented_sized(
                ui,
                "export_format",
                c,
                &mut form.format,
                &[(Format::Png, "PNG"), (Format::Jpeg, "JPEG")],
                162.0,
                41.0,
                14.0,
            );
            ui.add_space(20.0);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                if widgets::button(
                    ui,
                    "export_cancel",
                    c,
                    t("button.cancel"),
                    Kind::Soft,
                    161.0,
                )
                .clicked()
                {
                    cancel = true;
                }
                if widgets::button(ui, "export_go", c, t("button.export"), Kind::Primary, 161.0)
                    .clicked()
                {
                    go = true;
                }
            });
        });

        if go {
            let form = self.export_form.clone();
            self.modal = Modal::None;
            if let Some(error) = self.save_export(ctx, form, c.canvas_bg) {
                self.error_text = error;
                self.modal = Modal::Error;
            }
        }
        if cancel || dismissed {
            self.close(Modal::Export);
        }
    }

    /// Export error dialog.
    fn show_error_dialog(&mut self, ctx: &egui::Context, c: &Colors) {
        let error = self.error_text.clone();
        let mut close = false;
        let open = self.modal == Modal::Error;
        let dismissed = dialog::show(ctx, "operation_error", open, c, 360.0, c.surface, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            widgets::title(ui, c, t("dialog.error.title"));
            ui.add_space(10.0);
            ui.label(RichText::new(error.as_str()).color(c.text_dim));
            ui.add_space(24.0);
            if widgets::button(
                ui,
                "operation_error_close",
                c,
                t("button.close"),
                Kind::Soft,
                312.0,
            )
            .clicked()
            {
                close = true;
            }
        });
        if close || dismissed {
            self.close(Modal::Error);
        }
    }

    /// Update prompt: shown when a new version is available, while it downloads,
    /// or when the install failed. It waits until every other dialog has faded out.
    fn show_update_prompt(&mut self, ctx: &egui::Context, c: &Colors) {
        let status = self.updater.status.clone();
        let has_drawing = !self.canvas.is_empty();
        let other_dialog_visible = self.modal != Modal::None
            || [
                "settings",
                "confirm_clear",
                "export_form",
                "operation_error",
                "confirm_exit",
                "confirm_open",
            ]
            .iter()
            .any(|id| dialog::is_visible(ctx, id));
        let show = self.update_prompt_open() && !other_dialog_visible;

        let mut install = false;
        let mut later = false;
        let dismissed = dialog::show(ctx, "update", show, c, 380.0, c.surface, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            match &status {
                Status::Available(release) => {
                    widgets::title(ui, c, t("update.title"));
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(tf("update.ready", &[("version", release.version.as_str())]))
                            .color(c.text_dim),
                    );
                    if has_drawing {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(t("update.lost_warning"))
                                .color(c.danger)
                                .size(13.0),
                        );
                    }
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        if widgets::button(ui, "later", c, t("button.later"), Kind::Soft, 161.0)
                            .clicked()
                        {
                            later = true;
                        }
                        let install_label = if has_drawing {
                            t("button.install_anyway")
                        } else {
                            t("button.install")
                        };
                        // Destructive when the drawing would be lost, main action otherwise.
                        let install_kind = if has_drawing {
                            Kind::Danger
                        } else {
                            Kind::Primary
                        };
                        if widgets::button(ui, "install", c, install_label, install_kind, 161.0)
                            .clicked()
                        {
                            install = true;
                        }
                    });
                }
                Status::Downloading(progress) => {
                    widgets::title(ui, c, t("update.downloading"));
                    ui.add_space(14.0);
                    let fraction = progress.fraction();
                    // Indeterminate (animated) bar when the size is unknown.
                    let mut bar = egui::ProgressBar::new(fraction.unwrap_or(0.0))
                        .desired_width(300.0)
                        .animate(fraction.is_none());
                    if fraction.is_some() {
                        bar = bar.show_percentage();
                    }
                    ui.add(bar);
                }
                Status::InstallFailed(error) => {
                    widgets::title(ui, c, t("update.failed_title"));
                    ui.add_space(10.0);
                    ui.label(RichText::new(error.as_str()).color(c.text_dim));
                    ui.add_space(24.0);
                    if widgets::button(ui, "update_close", c, t("button.close"), Kind::Soft, 332.0)
                        .clicked()
                    {
                        later = true;
                    }
                }
                _ => {}
            }
        });

        if install {
            if let Status::Available(release) = self.updater.status.clone() {
                self.updater.install(ctx, release);
            }
        }
        // The prompt cannot be closed while the download is running.
        if later || (dismissed && !matches!(self.updater.status, Status::Downloading(_))) {
            self.updater.dismiss();
        }
    }

    /// "Quit without saving?" confirmation, when the drawing is not empty.
    fn show_quit_dialog(&mut self, ctx: &egui::Context, c: &Colors) {
        let mut quit = false;
        let mut keep = false;
        let open = self.modal == Modal::Quit;
        let focus_keep = opening_frame(ctx, "confirm_exit", open);
        let dismissed = dialog::show(ctx, "confirm_exit", open, c, 340.0, c.surface, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            widgets::title(ui, c, t("dialog.quit.title"));
            ui.add_space(10.0);
            ui.label(
                RichText::new(t("dialog.quit.body"))
                    .size(14.0)
                    .color(c.text_dim),
            );
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let stay = widgets::button(
                    ui,
                    "quit_cancel",
                    c,
                    t("button.keep_drawing"),
                    Kind::Soft,
                    141.0,
                );
                if focus_keep {
                    stay.request_focus();
                }
                if stay.clicked() {
                    keep = true;
                }
                if widgets::button(ui, "quit_confirm", c, t("button.quit"), Kind::Danger, 141.0)
                    .clicked()
                {
                    quit = true;
                }
            });
        });

        if quit {
            self.modal = Modal::None;
            self.close_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if keep || dismissed {
            self.close(Modal::Quit);
        }
    }
}

impl eframe::App for Painty {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.center_window(ctx);
        self.start_update_check(ctx);
        self.updater.poll(ctx);
        Self::track_input_mode(ctx);
        self.handle_close_request(ctx);
        self.follow_system_theme(ctx);

        let c = self.mode.colors(self.canvas_color);
        self.follow_ink(c.ink);
        self.handle_shortcuts(ctx);

        egui::CentralPanel::default()
            .frame(Frame::none().fill(c.canvas_bg))
            .show(ctx, |ui| self.canvas.show(ui, &mut self.brush, c.text_dim));

        self.show_toolbar(ctx, &c);
        self.show_settings(ctx, &c);
        self.show_release_notes(ctx, &c);
        self.save_settings_if_changed();

        // Dialogs, from the most common to the least. The update prompt waits for the others.
        self.show_clear_dialog(ctx, &c);
        self.show_export_dialog(ctx, &c);
        self.show_open_confirm(ctx, &c);
        self.show_error_dialog(ctx, &c);
        self.show_update_prompt(ctx, &c);
        self.show_quit_dialog(ctx, &c);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_release_notes_are_markdown_for_the_current_version() {
        let notes = include_str!("../RELEASE_NOTES.md");
        let expected_title = format!("# 🎨 Painty {}", env!("CARGO_PKG_VERSION"));
        assert!(notes.starts_with(&expected_title));
        assert!(notes.lines().any(|line| line.starts_with("- ")));
    }
}
