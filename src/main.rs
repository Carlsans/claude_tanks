// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
    // Models and results live in relative folders. When launched from elsewhere
    // (file manager, desktop shortcut), use the ones shipped next to the binary.
    if !std::path::Path::new("models").exists()
        && let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        && dir.join("models").exists()
    {
        let _ = std::env::set_current_dir(dir);
    }
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
