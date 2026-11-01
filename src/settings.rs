use eframe::egui::{self, pos2, vec2, Align, Align2, Context, FontId, Layout, Sense};

use crate::dialog;
use crate::i18n::{self, t, tf, Lang};
use crate::theme::{CanvasColor, Colors, Mode};
use crate::updater::{Status, Updater};
use crate::widgets::{self, Kind};

/// Card width from the mockup, padding included.
const CARD_W: f32 = 400.0;
/// Inner height of the 440 px card (24 px padding on each side).
const CONTENT_H: f32 = 392.0;
/// Separator (1) + top padding (16) + footer row (40).
const FOOTER_H: f32 = 57.0;
/// Width reserved on the right of a setting row for its control.
const CONTROL_W: f32 = 124.0;
/// Segment width for the two-option controls.
const SEG_W: f32 = 50.0;
/// Height of the segmented controls and the language button.
const SEG_H: f32 = 36.0;
/// Width reserved for the version and update status in the footer.
const INFO_W: f32 = 150.0;

/// Settings dialog: theme, canvas colour, language, update check and the update footer.
pub fn show(
    ctx: &Context,
    open: &mut bool,
    mode: &mut Mode,
    canvas: &mut CanvasColor,
    language: &mut Lang,
    auto_update: &mut bool,
    updater: &mut Updater,
    c: &Colors,
) {
    let dismissed = dialog::show(ctx, "settings", *open, c, CARD_W, c.surface_alt, |ui| {
        let width = ui.available_width();
        ui.set_min_height(CONTENT_H);
        // Every vertical gap below is explicit, so the card stays 440 px high.
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let start_y = ui.cursor().top();

        widgets::title(ui, c, t("settings.title"));
        ui.add_space(20.0);

        widgets::setting_row(ui, c, t("settings.theme"), CONTROL_W, |ui| {
            widgets::segmented_sized(
                ui,
                "settings_theme",
                c,
                mode,
                &[
                    (Mode::Light, t("theme.light")),
                    (Mode::Dark, t("theme.dark")),
                ],
                SEG_W,
                SEG_H,
                13.0,
            );
        });
        ui.add_space(12.0);

        widgets::setting_row(ui, c, t("settings.canvas"), CONTROL_W, |ui| {
            widgets::segmented_sized(
                ui,
                "settings_canvas",
                c,
                canvas,
                &[
                    (CanvasColor::Light, t("canvas.light")),
                    (CanvasColor::Dark, t("canvas.dark")),
                ],
                SEG_W,
                SEG_H,
                13.0,
            );
        });
        ui.add_space(12.0);

        widgets::setting_row(ui, c, t("settings.language"), CONTROL_W, |ui| {
            if let Some(lang) = widgets::language_menu(ui, c, *language) {
                *language = lang;
                // Applies from the next frame, so every text switches at once.
                i18n::set_lang(lang);
            }
        });
        ui.add_space(12.0);

        widgets::setting_row(ui, c, t("settings.auto_update"), CONTROL_W, |ui| {
            widgets::segmented_sized(
                ui,
                "settings_auto_update",
                c,
                auto_update,
                &[(true, t("toggle.on")), (false, t("toggle.off"))],
                SEG_W,
                SEG_H,
                13.0,
            );
        });

        // Push the footer to the bottom of the fixed-height card. The cursor is
        // used instead of `min_rect`, which already includes the minimum height.
        let used = ui.cursor().top() - start_y;
        ui.add_space((CONTENT_H - FOOTER_H - used).max(12.0));

        let (sep, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
        ui.painter().rect_filled(sep, 0.0, c.border);
        ui.add_space(16.0);

        let busy = matches!(updater.status, Status::Checking | Status::Downloading(_));
        let (status, color, detail) = status_line(&updater.status, c);

        ui.horizontal(|ui| {
            let (info, _) = ui.allocate_exact_size(vec2(INFO_W, 40.0), Sense::hover());
            let painter = ui.painter().with_clip_rect(info);
            painter.text(
                pos2(info.left(), info.top() + 9.0),
                Align2::LEFT_CENTER,
                tf(
                    "settings.version",
                    &[("version", env!("CARGO_PKG_VERSION"))],
                ),
                FontId::proportional(13.0),
                c.text_dim,
            );
            painter.text(
                pos2(info.left(), info.top() + 27.0),
                Align2::LEFT_CENTER,
                status,
                FontId::proportional(13.0),
                color,
            );
            if let Some(detail) = detail {
                let status_rect = egui::Rect::from_min_size(
                    pos2(info.left(), info.top() + 19.0),
                    vec2(INFO_W, 16.0),
                );
                ui.interact(
                    status_rect,
                    egui::Id::new("settings_status"),
                    Sense::hover(),
                )
                .on_hover_text(detail);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let check = ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::button(
                            ui,
                            "check_updates",
                            c,
                            t("button.check_updates"),
                            Kind::Primary,
                            0.0,
                        )
                        .clicked()
                    })
                    .inner;
                if check {
                    updater.check(ctx, true);
                }
            });
        });
    });

    if dismissed {
        *open = false;
    }
}

/// Short footer status: text, colour and the full error in a tooltip.
fn status_line(status: &Status, c: &Colors) -> (String, egui::Color32, Option<String>) {
    match status {
        Status::Idle => (String::new(), c.text_dim, None),
        Status::Checking => (t("update.status_checking").to_owned(), c.text_dim, None),
        Status::UpToDate => (t("update.status_up_to_date").to_owned(), c.success, None),
        Status::Available(release) => (
            tf("update.status_available", &[("version", &release.version)]),
            c.accent,
            None,
        ),
        Status::Downloading(progress) => match progress.fraction() {
            Some(fraction) => (
                tf(
                    "update.status_downloading",
                    &[("percent", &format!("{:.0}", fraction * 100.0))],
                ),
                c.text_dim,
                None,
            ),
            None => (
                t("update.status_downloading_unknown").to_owned(),
                c.text_dim,
                None,
            ),
        },
        Status::CheckFailed(error) => (
            t("update.status_failed").to_owned(),
            c.danger,
            Some(error.clone()),
        ),
        Status::InstallFailed(error) => (
            t("update.status_install_failed").to_owned(),
            c.danger,
            Some(error.clone()),
        ),
    }
}
