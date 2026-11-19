#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod curl;
mod highlight;
mod http;
mod loadtest;
mod model;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Kolibri")
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([720.0, 420.0])
            .with_app_id("kolibri"),
        ..Default::default()
    };
    eframe::run_native("Kolibri", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
