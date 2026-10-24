//! Display mode: watch two pilots fight at real or accelerated time.

use super::arena_view::{self, Effect, Overlays, TANK_COLORS};
use crate::bots::ScriptedBot;
use crate::bots::scripted::{ACE, LEVELS, MAX_LEVEL, VETERAN};
use crate::nn::{ModelFile, list_models};
use crate::sim::world::{Action, Controller, Event, World};
use crate::sim::*;
use eframe::egui::{self, Color32, RichText};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, PartialEq)]
pub enum Pilot {
    /// Scripted bot at a curriculum level.
    Scripted(u8),
    Model(PathBuf),
}

impl Pilot {
    pub fn label(&self) -> String {
        match self {
            Pilot::Scripted(l) => format!("Scripted {}", LEVELS[*l as usize]),
            Pilot::Model(p) => model_label(p),
        }
    }

    fn controller(&self, seed: u64) -> Result<Box<dyn Controller>, String> {
        Ok(match self {
            Pilot::Scripted(l) => Box::new(ScriptedBot::with_level(seed, *l)),
            Pilot::Model(p) => {
                let m = ModelFile::load(p).map_err(|e| format!("{}: {e}", p.display()))?;
                Box::new(m.brain())
            }
        })
    }
}

pub fn model_label(p: &std::path::Path) -> String {
    let rel = p.strip_prefix(crate::nn::models_dir()).unwrap_or(p);
    rel.with_extension("").display().to_string()
}

const SPEEDS: [(f32, &str); 10] = [
    (0.25, "0.25x"),
    (0.5, "0.5x"),
    (1.0, "1x"),
    (2.0, "2x"),
    (4.0, "4x"),
    (8.0, "8x"),
    (16.0, "16x"),
    (32.0, "32x"),
    (100.0, "100x"),
    (f32::INFINITY, "max"),
];

pub struct DisplayTab {
    pilots: [Pilot; 2],
    models: Vec<PathBuf>,
    seed: u64,
    density: f32,
    random_terrain: bool,
    speed: usize,
    paused: bool,
    auto_next: bool,
    overlays: Overlays,
    world: World,
    ctrls: [Box<dyn Controller>; 2],
    acts: [Action; 2],
    acc: f32,
    /// Wins for pilot A, pilot B, draws.
    score: [u32; 3],
    swapped: bool,
    recorded: bool,
    end_wait: f32,
    effects: Vec<Effect>,
    match_no: u64,
    sim_rate: f32,
    error: Option<String>,
}

impl Default for DisplayTab {
    fn default() -> Self {
        let mut d = DisplayTab {
            pilots: [Pilot::Scripted(ACE), Pilot::Scripted(VETERAN)],
            models: list_models(),
            seed: 1,
            density: 0.4,
            random_terrain: true,
            speed: 2,
            paused: false,
            auto_next: true,
            overlays: Overlays::default(),
            world: World::new(Terrain::generate(1, 0.4), false),
            ctrls: [Box::new(ScriptedBot::new(0)), Box::new(ScriptedBot::new(1))],
            acts: [Action::default(); 2],
            acc: 0.0,
            score: [0; 3],
            swapped: false,
            recorded: false,
            end_wait: 0.0,
            effects: Vec::new(),
            match_no: 0,
            sim_rate: 0.0,
            error: None,
        };
        d.restart(false);
        d
    }
}

impl DisplayTab {
    /// Set a pilot from outside (e.g. "watch" button in the training tab).
    pub fn set_pilot(&mut self, side: usize, p: Pilot) {
        self.models = list_models();
        self.pilots[side] = p;
        self.score = [0; 3];
        self.restart(false);
    }

    fn restart(&mut self, next: bool) {
        if next {
            self.match_no += 1;
            self.swapped = !self.swapped;
            if self.random_terrain {
                self.seed = Rng::new(self.seed ^ 0xABCD ^ self.match_no).next_u64() % 1_000_000;
                self.density = Rng::new(self.seed).f32();
            }
        }
        // Pilot A always drives tank 0 (blue); `swapped` exchanges spawn points instead.
        let terrain = Terrain::generate(self.seed, self.density);
        self.world = World::new(terrain, self.swapped);
        self.error = None;
        for (i, p) in self.pilots.iter().enumerate() {
            match p.controller(self.seed.wrapping_add(i as u64 * 7919)) {
                Ok(c) => self.ctrls[i] = c,
                Err(e) => {
                    self.error = Some(e);
                    self.ctrls[i] = Box::new(ScriptedBot::new(i as u64));
                }
            }
            self.ctrls[i].reset();
        }
        self.acts = [Action::default(); 2];
        self.acc = 0.0;
        self.recorded = false;
        self.end_wait = 0.0;
        self.effects.clear();
    }

    fn step_once(&mut self) {
        let w = &mut self.world;
        if w.done() {
            return;
        }
        if w.tick.is_multiple_of(ACTION_REPEAT) {
            for i in 0..2 {
                self.acts[i] = self.ctrls[i].act(&w.observe(i));
            }
        }
        w.step(self.acts);
        for e in &w.events {
            self.effects.push(match *e {
                Event::Fire { tank, pos } => Effect {
                    pos,
                    age: 0.0,
                    life: 0.15,
                    radius: 7.0,
                    color: TANK_COLORS[tank],
                },
                Event::WallHit { pos } => Effect {
                    pos,
                    age: 0.0,
                    life: 0.25,
                    radius: 8.0,
                    color: Color32::from_rgb(220, 200, 160),
                },
                Event::TankHit { pos, .. } => Effect {
                    pos,
                    age: 0.0,
                    life: 1.2,
                    radius: 40.0,
                    color: Color32::from_rgb(255, 160, 40),
                },
            });
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let real_dt = ui.input(|i| i.unstable_dt).min(0.1);
        self.advance(real_dt);

        egui::Panel::left("display_controls")
            .resizable(false)
            .default_size(290.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            arena_view::draw(ui, &self.world, self.overlays, &self.effects);
        });
        ui.ctx().request_repaint();
    }

    fn advance(&mut self, real_dt: f32) {
        for e in &mut self.effects {
            e.age += real_dt;
        }
        self.effects.retain(|e| e.age < e.life);

        if self.world.done() {
            if !self.recorded {
                self.recorded = true;
                let r = self.world.result.unwrap();
                self.score[r.winner.unwrap_or(2)] += 1;
            }
            if self.auto_next && !self.paused {
                self.end_wait += real_dt;
                if self.end_wait > 1.5 || SPEEDS[self.speed].0 > 16.0 {
                    self.restart(true);
                }
            }
            return;
        }
        if self.paused {
            return;
        }
        let speed = SPEEDS[self.speed].0;
        let start = Instant::now();
        let mut steps = 0u32;
        if speed.is_infinite() {
            while start.elapsed().as_secs_f32() < 0.014 && !self.world.done() {
                self.step_once();
                steps += 1;
            }
        } else {
            self.acc += real_dt * speed;
            while self.acc >= DT && !self.world.done() {
                self.acc -= DT;
                self.step_once();
                steps += 1;
                if start.elapsed().as_secs_f32() > 0.02 {
                    self.acc = 0.0;
                    break;
                }
            }
        }
        if real_dt > 0.0 {
            let rate = steps as f32 * DT / real_dt;
            self.sim_rate = self.sim_rate * 0.9 + rate * 0.1;
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Battle");
        let mut changed = false;
        for (side, color) in TANK_COLORS.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if side == 0 { "A" } else { "B" }).color(*color));
                egui::ComboBox::from_id_salt(("pilot", side))
                    .width(220.0)
                    .selected_text(self.pilots[side].label())
                    .show_ui(ui, |ui| {
                        for l in VETERAN..=MAX_LEVEL {
                            let p = Pilot::Scripted(l);
                            let label = p.label();
                            changed |= ui
                                .selectable_value(&mut self.pilots[side], p, label)
                                .changed();
                        }
                        for m in &self.models {
                            changed |= ui
                                .selectable_value(
                                    &mut self.pilots[side],
                                    Pilot::Model(m.clone()),
                                    model_label(m),
                                )
                                .changed();
                        }
                    });
            });
        }
        if ui.small_button("Rescan models/").clicked() {
            self.models = list_models();
        }
        if changed {
            self.score = [0; 3];
            self.restart(false);
        }
        if let Some(e) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, e);
        }

        ui.separator();
        ui.heading("Terrain");
        ui.checkbox(&mut self.random_terrain, "New random terrain each match");
        let mut regen = false;
        ui.horizontal(|ui| {
            ui.label("Seed");
            regen |= ui.add(egui::DragValue::new(&mut self.seed)).changed();
        });
        regen |= ui
            .add(egui::Slider::new(&mut self.density, 0.0..=1.0).text("maze density"))
            .changed();
        if regen {
            self.restart(false);
        }

        ui.separator();
        ui.heading("Playback");
        ui.horizontal_wrapped(|ui| {
            for (k, (_, name)) in SPEEDS.iter().enumerate() {
                ui.selectable_value(&mut self.speed, k, *name);
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button(if self.paused { "Play" } else { "Pause" })
                .clicked()
            {
                self.paused = !self.paused;
            }
            if ui
                .add_enabled(self.paused, egui::Button::new("Step"))
                .clicked()
            {
                for _ in 0..ACTION_REPEAT {
                    self.step_once();
                }
            }
            if ui.button("Restart").clicked() {
                self.restart(false);
            }
            if ui.button("Next").clicked() {
                self.restart(true);
            }
        });
        ui.checkbox(&mut self.auto_next, "Auto-start next match");
        ui.label(format!(
            "t = {:.1}/{:.0}s   sim speed ≈ {:.1}x",
            self.world.time(),
            MATCH_TIME,
            self.sim_rate
        ));

        ui.separator();
        ui.heading("Overlays");
        ui.checkbox(&mut self.overlays.radar, "Radar beams");
        ui.checkbox(&mut self.overlays.rays, "Wall sensor rays");
        ui.checkbox(&mut self.overlays.last_seen, "Last seen enemy position");

        ui.separator();
        ui.heading("Score");
        ui.label(format!(
            "A {}  -  B {}   (draws {})",
            self.score[0], self.score[1], self.score[2]
        ));
        if let Some(r) = self.world.result {
            let txt = match r.winner {
                Some(w) => format!("Winner: {}", if w == 0 { "A" } else { "B" }),
                None => "Draw".to_string(),
            };
            ui.label(RichText::new(txt).strong());
        }

        for (i, t) in self.world.tanks.iter().enumerate() {
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "{} {}{}",
                    if i == 0 { "A" } else { "B" },
                    self.pilots[i].label(),
                    if t.alive { "" } else { "  (destroyed)" }
                ))
                .color(TANK_COLORS[i])
                .strong(),
            );
            ui.horizontal(|ui| {
                ui.label("Ammo");
                ammo_pips(ui, t.ammo);
                ui.label(format!("  speed {:+.0}", t.speed));
            });
            bar(ui, "reload", 1.0 - t.cooldown / FIRE_COOLDOWN);
            bar(ui, "regen", t.regen / AMMO_REGEN);
            let a = t.last_action;
            for (name, v) in [
                ("throttle", a.throttle),
                ("hull", a.hull),
                ("turret", a.turret),
                ("radar", a.radar),
            ] {
                signed_bar(ui, name, v);
            }
            ui.label(format!(
                "fire {}   shots {}  hits {}",
                if a.fire { "FIRE" } else { "-" },
                t.stats.shots,
                t.stats.hits
            ));
        }
    }
}

fn ammo_pips(ui: &mut egui::Ui, ammo: u32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(54.0, 14.0), egui::Sense::hover());
    for k in 0..MAX_AMMO {
        let c = egui::pos2(rect.min.x + 8.0 + k as f32 * 18.0, rect.center().y);
        if k < ammo {
            ui.painter()
                .circle_filled(c, 6.0, Color32::from_rgb(230, 200, 120));
        } else {
            ui.painter().circle_stroke(c, 6.0, (1.0, Color32::GRAY));
        }
    }
}

fn bar(ui: &mut egui::Ui, label: &str, v: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([60.0, 14.0], egui::Label::new(label));
        ui.add(
            egui::ProgressBar::new(v.clamp(0.0, 1.0))
                .desired_width(170.0)
                .desired_height(10.0),
        );
    });
}

fn signed_bar(ui: &mut egui::Ui, label: &str, v: f32) {
    ui.horizontal(|ui| {
        ui.add_sized([60.0, 14.0], egui::Label::new(label));
        let (rect, _) = ui.allocate_exact_size(egui::vec2(170.0, 10.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, Color32::from_gray(50));
        let mid = rect.center().x;
        let x = mid + v.clamp(-1.0, 1.0) * rect.width() / 2.0;
        let r = egui::Rect::from_min_max(
            egui::pos2(mid.min(x), rect.min.y),
            egui::pos2(mid.max(x), rect.max.y),
        );
        p.rect_filled(r, 2.0, Color32::from_rgb(120, 180, 120));
        p.line_segment(
            [egui::pos2(mid, rect.min.y), egui::pos2(mid, rect.max.y)],
            (1.0, Color32::GRAY),
        );
    });
}

use crate::rng::Rng;
