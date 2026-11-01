//! Application entry point: window options, app icon, image loaders and fonts.

// Hide the console window in release builds (GUI app, no terminal).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod config;
mod dialog;
mod export;
mod i18n;
mod model;
mod settings;
mod theme;
mod toolbar;
mod updater;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    // Use the saved language from the first frame, including the window's first texts.
    i18n::set_lang(config::load().language);

    // Window icon, embedded in the binary at compile time.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("invalid icon");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([480.0, 320.0])
            .with_title("Painty")
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "Painty",
        options,
        Box::new(|cc| {
            // Needed for the SVG icons loaded with include_image!.
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::setup(&cc.egui_ctx);
            Ok(Box::new(app::Painty::new()))
        }),
    )
}
