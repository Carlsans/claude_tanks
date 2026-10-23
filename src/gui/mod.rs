//! egui front-end: Train, Display and Benchmark tabs.

pub mod arena_view;
pub mod display_tab;

use eframe::egui;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Train,
    Display,
    Benchmark,
}

pub struct App {
    tab: Tab,
    display: display_tab::DisplayTab,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        App {
            tab: Tab::Display,
            display: display_tab::DisplayTab::default(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Claude Tanks");
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Train, "Train");
                ui.selectable_value(&mut self.tab, Tab::Display, "Display");
                ui.selectable_value(&mut self.tab, Tab::Benchmark, "Benchmark");
            });
        });
        match self.tab {
            Tab::Display => self.display.ui(ui),
            Tab::Train | Tab::Benchmark => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Coming soon.");
                });
            }
        }
    }
}
