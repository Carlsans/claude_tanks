//! egui front-end: Train, Display and Benchmark tabs.

pub mod arena_view;
pub mod bench_tab;
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
    bench: bench_tab::BenchTab,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        App {
            tab: Tab::Display,
            display: display_tab::DisplayTab::default(),
            train: train_tab::TrainTab::default(),
            bench: bench_tab::BenchTab::default(),
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
        self.bench.poll();
        if let Some((a, b)) = self.bench.watch_request.take() {
            self.display.set_pilot(0, display_tab::Pilot::Model(a));
            let b = b.map_or(display_tab::Pilot::Scripted, display_tab::Pilot::Model);
            self.display.set_pilot(1, b);
            self.tab = Tab::Display;
        }
        match self.tab {
            Tab::Display => self.display.ui(ui),
            Tab::Train => self.train.ui(ui),
            Tab::Benchmark => self.bench.ui(ui),
        }
    }
}

/// PPO hyper-parameter widgets (shared by the Train and Benchmark tabs).
pub fn ppo_params_ui(ui: &mut egui::Ui, p: &mut crate::train::ppo::PpoParams) {
    ui.add(
        egui::Slider::new(&mut p.lr, 1e-5..=1e-2)
            .logarithmic(true)
            .text("policy lr"),
    );
    ui.add(
        egui::Slider::new(&mut p.vf_lr, 1e-5..=1e-2)
            .logarithmic(true)
            .text("value lr"),
    );
    ui.add(egui::Slider::new(&mut p.epochs, 1..=16).text("epochs"));
    ui.add(
        egui::Slider::new(&mut p.minibatch, 256..=16384)
            .logarithmic(true)
            .text("minibatch samples"),
    );
    ui.add(egui::Slider::new(&mut p.clip, 0.05..=0.5).text("clip"));
    ui.add(egui::Slider::new(&mut p.gamma, 0.9..=0.999).text("gamma"));
    ui.add(egui::Slider::new(&mut p.lambda, 0.8..=1.0).text("GAE lambda"));
    ui.add(egui::Slider::new(&mut p.ent_coef, 0.0..=0.05).text("entropy coef"));
    ui.add(egui::Slider::new(&mut p.chunk, 4..=128).text("BPTT chunk"));
}
