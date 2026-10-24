//! Train mode: configure a run, start/pause/stop/resume it, follow progress plots.
//! No battles are rendered here; use the Display tab to watch a saved pilot.

use crate::bots::scripted::{ACE, LEVELS, MAX_LEVEL};
use crate::nn::Arch;
use crate::train::{self, GenStats, Method, Msg, RunState, TrainConfig, TrainHandle, list_runs};
use eframe::egui::{self, Color32, RichText};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::path::PathBuf;

pub struct TrainTab {
    cfg: TrainConfig,
    handle: Option<TrainHandle>,
    paused: bool,
    history: Vec<GenStats>,
    log: Vec<String>,
    runs: Vec<PathBuf>,
    resume: Option<PathBuf>,
    x_steps: bool,
    /// Saved champions offered as league opponents.
    champions: Vec<PathBuf>,
    /// Set when the user asks to watch a model; taken by the app.
    pub watch_request: Option<PathBuf>,
}

/// `models/champions/*.json`, sorted by name.
fn list_champions() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(crate::nn::models_dir().join("champions"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    v.sort();
    v
}

impl Default for TrainTab {
    fn default() -> Self {
        let mut cfg = TrainConfig::default();
        cfg.run_name = TrainConfig::default_run_name(cfg.arch, cfg.method);
        TrainTab {
            cfg,
            handle: None,
            paused: false,
            history: Vec::new(),
            log: Vec::new(),
            runs: list_runs(),
            resume: None,
            x_steps: false,
            champions: list_champions(),
            watch_request: None,
        }
    }
}

impl TrainTab {
    pub fn running(&self) -> bool {
        self.handle.is_some()
    }

    /// Drain trainer messages; called every frame even when the tab is hidden.
    pub fn poll(&mut self) {
        let Some(h) = &self.handle else { return };
        let mut finished = false;
        while let Ok(m) = h.msg.try_recv() {
            match m {
                Msg::History(hist) => self.history = hist,
                Msg::Stats(s) => self.history.push(s),
                Msg::Log(l) => self.log.push(l),
                Msg::Finished => finished = true,
            }
        }
        if finished || h.is_finished() {
            self.log.push("Training thread finished.".into());
            self.handle = None;
            self.paused = false;
            self.runs = list_runs();
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("train_cfg")
            .resizable(false)
            .default_size(330.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.config_ui(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| self.progress_ui(ui));
        if self.running() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn start(&mut self, resume: Option<RunState>) {
        self.history.clear();
        self.log.clear();
        self.paused = false;
        if resume.is_none() && self.cfg.run_dir().exists() {
            self.cfg.run_name = TrainConfig::default_run_name(self.cfg.arch, self.cfg.method);
        }
        if let Some(r) = &resume {
            self.cfg = r.cfg.clone();
        }
        self.handle = Some(train::spawn(self.cfg.clone(), resume));
    }

    fn config_ui(&mut self, ui: &mut egui::Ui) {
        let running = self.running();
        ui.heading("Run");
        ui.horizontal(|ui| {
            if !running {
                if ui.button(RichText::new("Start new run").strong()).clicked() {
                    self.start(None);
                }
            } else {
                let label = if self.paused { "Resume" } else { "Pause" };
                if ui.button(label).clicked() {
                    let h = self.handle.as_ref().unwrap();
                    let _ = h.cmd.send(if self.paused {
                        train::Cmd::Resume
                    } else {
                        train::Cmd::Pause
                    });
                    self.paused = !self.paused;
                }
                if ui.button("Stop").clicked() {
                    let _ = self.handle.as_ref().unwrap().cmd.send(train::Cmd::Stop);
                }
            }
        });

        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("resume")
                    .width(200.0)
                    .selected_text(
                        self.resume
                            .as_ref()
                            .map_or("(pick a saved run)".into(), |p| {
                                super::display_tab::model_label(p)
                            }),
                    )
                    .show_ui(ui, |ui| {
                        for r in &self.runs {
                            ui.selectable_value(
                                &mut self.resume,
                                Some(r.clone()),
                                super::display_tab::model_label(r),
                            );
                        }
                    });
                if ui
                    .add_enabled(self.resume.is_some(), egui::Button::new("Resume run"))
                    .clicked()
                {
                    match RunState::load(self.resume.as_ref().unwrap()) {
                        Ok(st) => self.start(Some(st)),
                        Err(e) => self.log.push(format!("Cannot resume: {e}")),
                    }
                }
            });
            if ui.small_button("Rescan runs").clicked() {
                self.runs = list_runs();
            }

            ui.separator();
            ui.heading("Configuration");
            let c = &mut self.cfg;
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut c.run_name);
            });
            let mut renamed = false;
            egui::ComboBox::from_label("Architecture")
                .selected_text(c.arch.name())
                .show_ui(ui, |ui| {
                    for a in Arch::ALL {
                        renamed |= ui.selectable_value(&mut c.arch, a, a.name()).changed();
                    }
                });
            egui::ComboBox::from_label("Method")
                .selected_text(c.method.name())
                .show_ui(ui, |ui| {
                    for m in Method::ALL {
                        let ok = m.supports(c.arch);
                        renamed |= ui
                            .add_enabled(ok, egui::Button::selectable(c.method == m, m.name()))
                            .clicked()
                            && {
                                c.method = m;
                                true
                            };
                    }
                });
            if !c.method.supports(c.arch) {
                c.method = Method::ALL
                    .into_iter()
                    .find(|m| m.supports(c.arch))
                    .unwrap();
                renamed = true;
            }
            if renamed {
                c.run_name = TrainConfig::default_run_name(c.arch, c.method);
            }
            if c.arch != Arch::Neat {
                ui.label(format!("{} parameters", c.arch.param_count()));
                ui.checkbox(
                    &mut c.imitation.enabled,
                    "Warm start by imitating the scripted bot",
                );
                if c.imitation.enabled {
                    ui.collapsing("Imitation settings", |ui| {
                        let im = &mut c.imitation;
                        egui::ComboBox::from_label("teacher")
                            .selected_text(LEVELS[im.teacher.min(MAX_LEVEL) as usize])
                            .show_ui(ui, |ui| {
                                for l in 2..=MAX_LEVEL {
                                    ui.selectable_value(&mut im.teacher, l, LEVELS[l as usize]);
                                }
                            });
                        ui.add(
                            egui::Slider::new(&mut im.matches, 20..=2000)
                                .text("demo matches / round"),
                        );
                        ui.add(egui::Slider::new(&mut im.epochs, 1..=100).text("epochs"));
                        ui.add(
                            egui::Slider::new(&mut im.dagger_rounds, 0..=10).text("DAgger rounds"),
                        );
                        ui.add(
                            egui::Slider::new(&mut im.lr, 1e-4..=1e-2)
                                .logarithmic(true)
                                .text("learning rate"),
                        );
                    });
                }
            }
            let pop_label = if c.method == Method::Ppo {
                "parallel games"
            } else {
                "population"
            };
            ui.add(egui::Slider::new(&mut c.population, 4..=512).text(pop_label));
            ui.add(egui::Slider::new(&mut c.matches, 1..=32).text("matches / eval"));
            ui.add(egui::Slider::new(&mut c.density.0, 0.0..=1.0).text("min maze density"));
            ui.add(egui::Slider::new(&mut c.density.1, 0.0..=1.0).text("max maze density"));
            c.density.1 = c.density.1.max(c.density.0);
            let max_threads = std::thread::available_parallelism().map_or(4, |n| n.get());
            ui.add(egui::Slider::new(&mut c.threads, 1..=max_threads).text("CPU threads"));
            if matches!(c.method, Method::Ga | Method::Es) {
                #[cfg(feature = "gpu")]
                ui.checkbox(&mut c.use_gpu, "Use the GPU when faster")
                    .on_hover_text(format!(
                        "{}\nTimes both devices on the first generations and keeps the faster one.",
                        crate::gpu::status()
                    ));
            }
            ui.horizontal(|ui| {
                ui.label("Seed");
                ui.add(egui::DragValue::new(&mut c.seed));
            });

            ui.collapsing("Budget", |ui| {
                let mut mins = (c.max_seconds / 60.0) as f32;
                if ui
                    .add(egui::Slider::new(&mut mins, 0.0..=1440.0).text("max minutes (0 = none)"))
                    .changed()
                {
                    c.max_seconds = mins as f64 * 60.0;
                }
                ui.add(
                    egui::Slider::new(&mut c.max_generations, 0..=10000)
                        .text("max generations (0 = none)"),
                );
                let mut m = (c.max_steps / 1_000_000) as u32;
                if ui
                    .add(
                        egui::Slider::new(&mut m, 0..=100_000)
                            .logarithmic(true)
                            .text("max Msteps (0 = none)"),
                    )
                    .changed()
                {
                    c.max_steps = m as u64 * 1_000_000;
                }
            });
            ui.collapsing("Opponents & shaping", |ui| {
                for (label, level) in [
                    ("start at level", &mut c.start_level),
                    ("top level", &mut c.top_level),
                    ("mix bots from", &mut c.mix_from),
                ] {
                    egui::ComboBox::from_label(label)
                        .selected_text(LEVELS[(*level).min(MAX_LEVEL) as usize])
                        .show_ui(ui, |ui| {
                            for l in 0..=MAX_LEVEL {
                                ui.selectable_value(level, l, LEVELS[l as usize]);
                            }
                        });
                }
                ui.add(egui::Slider::new(&mut c.scripted_share, 0.0..=1.0).text("scripted share"));
                ui.add(egui::Slider::new(&mut c.hof_size, 1..=100).text("hall of fame size"));
                ui.add(
                    egui::Slider::new(&mut c.league_share, 0.0..=1.0)
                        .text("league share (of the non-scripted opponents)"),
                );
                ui.collapsing(format!("League: {} models", c.league.len()), |ui| {
                    for p in &self.champions {
                        let path = p.display().to_string();
                        let mut on = c.league.contains(&path);
                        if ui
                            .checkbox(&mut on, crate::gui::display_tab::model_label(p))
                            .changed()
                        {
                            if on {
                                c.league.push(path);
                            } else {
                                c.league.retain(|x| *x != path);
                            }
                        }
                    }
                });
                ui.add(egui::Slider::new(&mut c.shaping_start, 0.0..=2.0).text("shaping start"));
                ui.add(egui::Slider::new(&mut c.shaping_end, 0.0..=2.0).text("shaping end"));
                ui.add(
                    egui::Slider::new(&mut c.shaping_gens, 0..=5000).text("shaping anneal gens"),
                );
                ui.add(egui::Slider::new(&mut c.eval_matches, 8..=200).text("eval matches"));
            });
            match c.method {
                Method::Ga => {
                    ui.collapsing("GA parameters", |ui| {
                        let g = &mut c.ga;
                        ui.add(egui::Slider::new(&mut g.elite, 0..=32).text("elite"));
                        ui.add(
                            egui::Slider::new(&mut g.tournament, 1..=16).text("tournament size"),
                        );
                        ui.add(
                            egui::Slider::new(&mut g.sigma, 0.001..=0.5)
                                .logarithmic(true)
                                .text("mutation sigma"),
                        );
                        ui.add(
                            egui::Slider::new(&mut g.pretrained_sigma, 0.0005..=0.1)
                                .logarithmic(true)
                                .text("sigma from imitation"),
                        );
                        ui.add(
                            egui::Slider::new(&mut g.crossover, 0.0..=1.0).text("crossover prob"),
                        );
                    });
                }
                Method::Es => {
                    ui.collapsing("ES parameters", |ui| {
                        let e = &mut c.es;
                        ui.add(
                            egui::Slider::new(&mut e.sigma, 0.001..=0.3)
                                .logarithmic(true)
                                .text("noise sigma"),
                        );
                        ui.add(
                            egui::Slider::new(&mut e.lr, 1e-4..=0.1)
                                .logarithmic(true)
                                .text("learning rate"),
                        );
                        ui.add(
                            egui::Slider::new(&mut e.weight_decay, 0.0..=0.05).text("weight decay"),
                        );
                    });
                }
                Method::Neat => {
                    ui.collapsing("NEAT parameters", |ui| {
                        let n = &mut c.neat;
                        ui.add(
                            egui::Slider::new(&mut n.target_species, 1..=40).text("target species"),
                        );
                        ui.add(
                            egui::Slider::new(&mut n.stagnation, 1..=200).text("stagnation limit"),
                        );
                        ui.add(
                            egui::Slider::new(&mut n.survival, 0.05..=1.0)
                                .text("survival fraction"),
                        );
                        ui.add(
                            egui::Slider::new(&mut n.crossover, 0.0..=1.0).text("crossover prob"),
                        );
                        ui.add(
                            egui::Slider::new(&mut n.weight_power, 0.01..=2.0)
                                .text("weight mutation"),
                        );
                        ui.add(
                            egui::Slider::new(&mut n.add_conn, 0.0..=1.0)
                                .text("add connection prob"),
                        );
                        ui.add(egui::Slider::new(&mut n.add_node, 0.0..=1.0).text("add node prob"));
                    });
                }
                Method::Ppo => {
                    ui.collapsing("PPO parameters", |ui| super::ppo_params_ui(ui, &mut c.ppo));
                }
            }
            if ui.button("Reset to defaults").clicked() {
                let name = c.run_name.clone();
                let (arch, method) = (c.arch, c.method);
                *c = TrainConfig {
                    run_name: name,
                    arch,
                    method,
                    ..Default::default()
                };
            }
        });
    }

    fn progress_ui(&mut self, ui: &mut egui::Ui) {
        let status = if self.running() {
            if self.paused { "paused" } else { "training" }
        } else {
            "idle"
        };
        ui.horizontal(|ui| {
            ui.heading(format!("{}  [{status}]", self.cfg.run_name));
        });
        if let Some(last) = self.history.last() {
            let rate = rate(&self.history);
            ui.label(format!(
                "generation {}   {:.1} M sim steps   {:.0} s   {:.2} M steps/s   curriculum: {}   shaping {:.2}",
                last.generation,
                last.steps as f64 / 1e6,
                last.seconds,
                rate / 1e6,
                LEVELS[last.level as usize],
                last.shaping
            ));
            ui.label(format!(
                "champion vs scripted veteran: win {:.0}%  draw {:.0}%  loss {:.0}%",
                last.vs_scripted.win * 100.0,
                last.vs_scripted.draw * 100.0,
                last.vs_scripted.loss * 100.0
            ));
            if !last.extra.is_empty() {
                let txt: Vec<String> = last
                    .extra
                    .iter()
                    .map(|(k, v)| format!("{k}: {v:.2}"))
                    .collect();
                ui.label(txt.join("   "));
            }
        } else {
            ui.label("Start a run to see progress. Battles are not rendered while training.");
        }
        ui.horizontal(|ui| {
            let best = self.cfg.run_dir().join("best.json");
            if ui
                .add_enabled(
                    best.exists(),
                    egui::Button::new("Watch best vs scripted in Display"),
                )
                .clicked()
            {
                self.watch_request = Some(best);
            }
            ui.checkbox(&mut self.x_steps, "x axis = sim steps");
        });

        let x = |s: &GenStats| {
            if self.x_steps {
                s.steps as f64 / 1e6
            } else {
                s.generation as f64
            }
        };
        let series = |f: &dyn Fn(&GenStats) -> f64| -> PlotPoints {
            self.history.iter().map(|s| [x(s), f(s)]).collect()
        };
        let h = ((ui.available_height() - 140.0) / 3.0).max(120.0);
        ui.label("Fitness");
        Plot::new("fitness")
            .height(h)
            .legend(Legend::default())
            .show(ui, |p| {
                p.line(
                    Line::new("best", series(&|s| s.best_fitness as f64))
                        .color(Color32::LIGHT_GREEN),
                );
                p.line(
                    Line::new("mean", series(&|s| s.mean_fitness as f64))
                        .color(Color32::LIGHT_BLUE),
                );
                p.line(
                    Line::new("curriculum level", series(&|s| s.level as f64 / 3.0))
                        .color(Color32::GRAY),
                );
            });
        ui.label("Champion vs the veteran (%), and wins vs the ace");
        Plot::new("winrate")
            .height(h)
            .include_y(0.0)
            .include_y(100.0)
            .legend(Legend::default())
            .show(ui, |p| {
                p.line(
                    Line::new("win", series(&|s| s.vs_scripted.win as f64 * 100.0))
                        .color(Color32::LIGHT_GREEN),
                );
                p.line(
                    Line::new("draw", series(&|s| s.vs_scripted.draw as f64 * 100.0))
                        .color(Color32::GRAY),
                );
                p.line(
                    Line::new("loss", series(&|s| s.vs_scripted.loss as f64 * 100.0))
                        .color(Color32::LIGHT_RED),
                );
                p.line(
                    Line::new("win vs ace", series(&|s| s.vs_ace.win as f64 * 100.0))
                        .color(Color32::LIGHT_BLUE),
                );
                if self.history.iter().any(|s| s.level > ACE) {
                    p.line(
                        Line::new(
                            "win vs stronger bots",
                            series(&|s| s.vs_top.win as f64 * 100.0),
                        )
                        .color(Color32::from_rgb(200, 140, 255)),
                    );
                }
            });
        let extras: Vec<String> = self
            .history
            .last()
            .map(|s| s.extra.iter().map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        if !extras.is_empty() {
            ui.label("Method details");
            Plot::new("extra")
                .height(h * 0.8)
                .legend(Legend::default())
                .show(ui, |p| {
                    for (k, name) in extras.iter().enumerate() {
                        let pts: PlotPoints = self
                            .history
                            .iter()
                            .filter_map(|s| s.extra.get(k).map(|e| [x(s), e.1 as f64]))
                            .collect();
                        p.line(Line::new(name.clone(), pts));
                    }
                });
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(110.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for l in &self.log {
                    ui.label(l);
                }
            });
    }
}

/// Sim steps per second over the last few generations.
fn rate(h: &[GenStats]) -> f64 {
    let n = h.len();
    if n < 2 {
        return 0.0;
    }
    let a = &h[n.saturating_sub(6)];
    let b = &h[n - 1];
    let dt = b.seconds - a.seconds;
    if dt > 0.0 {
        (b.steps - a.steps) as f64 / dt
    } else {
        0.0
    }
}
