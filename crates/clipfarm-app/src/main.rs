#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1080.0, 700.0])
            .with_title("clipfarm — long video in, shorts out"),
        ..Default::default()
    };
    eframe::run_native(
        "clipfarm",
        options,
        Box::new(|cc| Ok(Box::new(app::ClipfarmApp::new(cc)))),
    )
}
