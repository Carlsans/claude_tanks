//! Benchmark mode: equal-budget training of every architecture, then a tournament.

use super::display_tab::Pilot;
use crate::bots::scripted::VETERAN;
use crate::train::GenStats;
use crate::train::benchmark::{
    BenchConfig, BenchHandle, BenchMsg, BenchResults, all_entries, entry_name, spawn_benchmark,
};
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;

pub struct BenchTab {
    cfg: BenchConfig,
    enabled: Vec<bool>,
    handle: Option<BenchHandle>,
    current: Option<usize>,
    last: Vec<Option<GenStats>>,
    log: Vec<String>,
    results: Option<BenchResults>,
    pub watch_request: Option<(PathBuf, Pilot)>,
}

impl Default for BenchTab {
    fn default() -> Self {
        let cfg = BenchConfig::default();
        let n = cfg.entries.len();
        BenchTab {
            cfg,
            enabled: vec![true; n],
            handle: None,
            current: None,
            last: vec![None; n],
            log: Vec::new(),
            results: BenchResults::load(),
            watch_request: None,
        }
    }
}

impl BenchTab {
    pub fn poll(&mut self) {
        let Some(h) = &self.handle else { return };
        let mut done = false;
        while let Ok(m) = h.msg.try_recv() {
            match m {
                BenchMsg::Entry(k) => self.current = Some(k),
                BenchMsg::Stats(k, s) => self.last[k] = Some(s),
                BenchMsg::Log(l) => self.log.push(l),
                BenchMsg::Results(r) => self.results = Some(r),
                BenchMsg::Finished => done = true,
            }
        }
        if done || h.is_finished() {
            self.handle = None;
            self.current = None;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let running = self.handle.is_some();
        let entries = all_entries();
        egui::Panel::left("bench_cfg").resizable(false).default_size(320.0).show(ui, |ui| {
            ui.heading("Benchmark");
            ui.label("Each selected architecture/method is trained with the same simulation budget, then all champions and the scripted bot play a round-robin tournament.");
            ui.add_enabled_ui(!running, |ui| {
                for (k, (a, m)) in entries.iter().enumerate() {
                    ui.checkbox(&mut self.enabled[k], entry_name(*a, *m));
                }
                let mut mins = (self.cfg.budget_seconds / 60.0) as f32;
                if ui.add(egui::Slider::new(&mut mins, 0.0..=240.0).text("minutes / entry (0 = none)")).changed() {
                    self.cfg.budget_seconds = mins as f64 * 60.0;
                }
                let mut m = (self.cfg.budget_steps / 1_000_000) as u32;
                if ui.add(egui::Slider::new(&mut m, 0..=20_000).logarithmic(true).text("M steps / entry (0 = none)")).changed() {
                    self.cfg.budget_steps = m as u64 * 1_000_000;
                }
                ui.add(egui::Slider::new(&mut self.cfg.matches, 10..=400).text("matches / pairing"));
                let max_threads = std::thread::available_parallelism().map_or(4, |n| n.get());
                ui.add(egui::Slider::new(&mut self.cfg.threads, 1..=max_threads).text("CPU threads"));
            });
            if running {
                if ui.button("Stop benchmark").clicked() {
                    self.handle.as_ref().unwrap().stop();
                }
            } else if ui.button(RichText::new("Start benchmark").strong()).clicked() {
                self.cfg.entries = entries
                    .iter()
                    .zip(&self.enabled)
                    .filter(|(_, e)| **e)
                    .map(|(x, _)| *x)
                    .collect();
                self.last = vec![None; self.cfg.entries.len()];
                self.log.clear();
                self.handle = Some(spawn_benchmark(self.cfg.clone()));
            }
            ui.separator();
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for l in &self.log {
                    ui.small(l);
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if running {
                    self.progress_ui(ui);
                    ui.separator();
                }
                self.results_ui(ui);
            });
        });
        if running {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    fn progress_ui(&self, ui: &mut egui::Ui) {
        ui.heading("Training progress");
        for (k, (a, m)) in self.cfg.entries.iter().enumerate() {
            let frac = self.last[k]
                .as_ref()
                .map_or(0.0, |s| s.steps as f32 / self.cfg.budget_steps as f32);
            ui.horizontal(|ui| {
                let name = entry_name(*a, *m);
                let txt = if self.current == Some(k) {
                    RichText::new(name).strong()
                } else {
                    RichText::new(name)
                };
                ui.add_sized([200.0, 16.0], egui::Label::new(txt));
                ui.add(
                    egui::ProgressBar::new(frac.min(1.0))
                        .desired_width(220.0)
                        .show_percentage(),
                );
                if let Some(s) = &self.last[k] {
                    ui.label(format!(
                        "gen {}  win vs veteran {:.0}%  loss {:.0}%",
                        s.generation,
                        s.vs_scripted.win * 100.0,
                        s.vs_scripted.loss * 100.0
                    ));
                }
            });
        }
    }

    fn results_ui(&mut self, ui: &mut egui::Ui) {
        let Some(r) = &self.results else {
            ui.label("No benchmark results yet.");
            return;
        };
        ui.heading("Results");
        ui.label(&r.finished);
        let rank = r.ranking();
        egui::Grid::new("ranking")
            .striped(true)
            .num_columns(6)
            .show(ui, |ui| {
                for h in ["#", "Pilot", "Elo", "vs veteran W/D/L %", "train time", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (pos, &i) in rank.iter().enumerate() {
                    let c = &r.contestants[i];
                    ui.label(format!("{}", pos + 1));
                    ui.label(&c.name);
                    ui.label(format!("{:.0}", c.elo));
                    if c.model.is_some() {
                        ui.label(format!(
                            "{:.0} / {:.0} / {:.0}",
                            c.vs_scripted.win * 100.0,
                            c.vs_scripted.draw * 100.0,
                            c.vs_scripted.loss * 100.0
                        ));
                        ui.label(format!("{:.0} s", c.train_seconds));
                    } else {
                        ui.label("-");
                        ui.label("-");
                    }
                    if let Some(p) = &c.model {
                        if ui.small_button("watch vs #1").clicked() {
                            let top = &r.contestants[rank[0]];
                            let pilot = match &top.model {
                                Some(m) => Pilot::Model(m.clone()),
                                None => Pilot::Scripted(top.level.unwrap_or(VETERAN)),
                            };
                            self.watch_request = Some((p.clone(), pilot));
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });

        ui.add_space(10.0);
        ui.label(RichText::new("Score matrix (row vs column, win = 1, draw = 0.5)").strong());
        let n = r.contestants.len();
        let cell = 34.0;
        let label_w = 190.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(label_w + cell * n as f32, cell * n as f32 + 20.0),
            egui::Sense::hover(),
        );
        let p = ui.painter_at(rect);
        let font = egui::FontId::proportional(11.0);
        for (row, &i) in rank.iter().enumerate() {
            let y = rect.min.y + 20.0 + row as f32 * cell;
            p.text(
                egui::pos2(rect.min.x, y + cell / 2.0),
                egui::Align2::LEFT_CENTER,
                &r.contestants[i].name,
                font.clone(),
                ui.visuals().text_color(),
            );
            for (col, &j) in rank.iter().enumerate() {
                let x = rect.min.x + label_w + col as f32 * cell;
                if row == 0 {
                    p.text(
                        egui::pos2(x + cell / 2.0, rect.min.y + 8.0),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", col + 1),
                        font.clone(),
                        ui.visuals().text_color(),
                    );
                }
                let c =
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(cell - 2.0, cell - 2.0));
                if i == j {
                    p.rect_filled(c, 2.0, Color32::from_gray(40));
                    continue;
                }
                let v = r.score[i][j];
                let color = if v >= 0.5 {
                    Color32::from_rgb(40, (80.0 + 160.0 * (v - 0.5) * 2.0) as u8, 60)
                } else {
                    Color32::from_rgb((80.0 + 160.0 * (0.5 - v) * 2.0) as u8, 50, 50)
                };
                p.rect_filled(c, 2.0, color);
                p.text(
                    c.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:.0}", v * 100.0),
                    font.clone(),
                    Color32::WHITE,
                );
            }
        }
    }
}
