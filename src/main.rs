// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 900.0])
            .with_title("Claude Tanks"),
        ..Default::default()
    };
    eframe::run_native(
        "Claude Tanks",
        options,
        Box::new(|cc| Ok(Box::new(claude_tanks::gui::App::new(cc)))),
    )
}
