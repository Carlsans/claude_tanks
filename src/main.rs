fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_title("Claude Tanks"),
        ..Default::default()
    };
    eframe::run_ui_native("Claude Tanks", options, |ui, _frame| {
        eframe::egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Claude Tanks");
        });
    })
}
