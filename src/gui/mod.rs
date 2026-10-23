//! egui front-end: Train, Display and Benchmark tabs.

pub mod arena_view;
pub mod display_tab;
pub mod train_tab;

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
    train: train_tab::TrainTab,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        App {
            tab: Tab::Display,
            display: display_tab::DisplayTab::default(),
            train: train_tab::TrainTab::default(),
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
        self.train.poll();
        if let Some(p) = self.train.watch_request.take() {
            self.display.set_pilot(0, display_tab::Pilot::Model(p));
            self.display.set_pilot(1, display_tab::Pilot::Scripted);
            self.tab = Tab::Display;
        }
        match self.tab {
            Tab::Display => self.display.ui(ui),
            Tab::Train => self.train.ui(ui),
            Tab::Benchmark => {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Coming soon.");
                });
            }
        }
    }
}

/// PPO hyper-parameter widgets (shared by the Train and Benchmark tabs).
pub fn ppo_params_ui(ui: &mut egui::Ui, _p: &mut crate::train::ppo::PpoParams) {
    ui.label("(PPO not implemented yet)");
}
