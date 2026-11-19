#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod curl;
mod highlight;
mod http;
mod kit;
mod loadtest;
mod model;
mod postman;
mod theme;

/// App icon (window, taskbar and macOS Dock), embedded in the binary.
const ICON_PNG: &[u8] = include_bytes!("../assets/icon-512.png");

fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        // The name stays for the Dock / window menus but is not drawn in the title bar.
        .with_title("Kolibri")
        .with_title_shown(false)
        .with_inner_size([1200.0, 780.0])
        .with_min_inner_size([720.0, 420.0])
        .with_app_id("kolibri");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON_PNG) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native("Kolibri", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_icon_decodes() {
        let icon = eframe::icon_data::from_png_bytes(super::ICON_PNG).unwrap();
        assert_eq!((icon.width, icon.height), (512, 512));
        assert_eq!(icon.rgba[3], 0, "corners must be transparent");
    }
}
