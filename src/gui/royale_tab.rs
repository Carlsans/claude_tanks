//! Robocode tab: a Tank Royale server that real Robocode bots join, and
//! battles between them and our pilots under Robocode rules.

use super::arena_view::TANK_COLORS;
use super::display_tab::model_label;
use crate::bots::ScriptedBot;
use crate::bots::scripted::{LEVELS, MAX_LEVEL, VETERAN};
use crate::nn::{ModelFile, list_models};
use crate::royale::battle::Score;
use crate::royale::pilot::{Adapter, Driver};
use crate::royale::protocol::BotHandshake;
use crate::royale::server::{
    BattleOptions, BotDir, DEFAULT_PORT, Entrant, Live, Server, ranks, run_battle,
};
use crate::royale::{BOT_RADIUS, RADAR_RADIUS, Setup};
use eframe::egui::{self, Color32, Pos2, RichText, Shape, Stroke};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Clone, PartialEq)]
enum Choice {
    /// A connected bot: connection id and name.
    Remote(u64, String),
    Scripted(u8),
    Model(PathBuf),
}

impl Choice {
    fn label(&self) -> String {
        match self {
            Choice::Remote(_, n) => format!("{n} (Robocode bot)"),
            Choice::Scripted(l) => format!("Our {}", LEVELS[*l as usize]),
            Choice::Model(p) => model_label(p),
        }
    }
}

pub struct RoyaleTab {
    port: u16,
    server: Option<Arc<Server>>,
    bots_dir: String,
    dirs: Vec<BotDir>,
    started: Vec<(String, Child)>,
    models: Vec<PathBuf>,
    choices: [Choice; 2],
    rounds: u32,
    tps: f64,
    max_speed: bool,
    live: Arc<Mutex<Live>>,
    stop: Arc<AtomicBool>,
    battle: Option<JoinHandle<Result<Vec<Score>, String>>>,
    message: Option<String>,
}

impl Default for RoyaleTab {
    fn default() -> Self {
        let mut t = RoyaleTab {
            port: DEFAULT_PORT,
            server: None,
            bots_dir: "bots".into(),
            dirs: Vec::new(),
            started: Vec::new(),
            models: royale_models(),
            choices: [Choice::Scripted(VETERAN + 2), Choice::Scripted(VETERAN)],
            rounds: 10,
            tps: 30.0,
            max_speed: false,
            live: Arc::default(),
            stop: Arc::new(AtomicBool::new(false)),
            battle: None,
            message: None,
        };
        t.dirs = BotDir::scan(t.bots_dir.as_ref());
        t
    }
}

impl Drop for RoyaleTab {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for (_, c) in &mut self.started {
            let _ = c.kill();
        }
    }
}

impl RoyaleTab {
    fn server(&mut self) -> Option<Arc<Server>> {
        if self.server.is_none() {
            match Server::start(self.port) {
                Ok(s) => self.server = Some(Arc::new(s)),
                Err(e) => self.message = Some(format!("Cannot listen on port {}: {e}", self.port)),
            }
        }
        self.server.clone()
    }

    fn running(&self) -> bool {
        self.battle.as_ref().is_some_and(|h| !h.is_finished())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some(h) = self.battle.take_if(|h| h.is_finished())
            && let Ok(Err(e)) = h.join()
            && e != "stopped"
        {
            self.message = Some(e);
        }
        egui::Panel::left("royale_controls")
            .resizable(false)
            .default_size(330.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            let live = self.live.lock().unwrap();
            draw(ui, &live);
        });
        ui.ctx().request_repaint();
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Robocode battles");
        ui.label("Robocode Tank Royale rules. Real Robocode bots join over the network, as on the official server.");
        let connected = self.server.as_ref().map(|s| s.bots()).unwrap_or_default();
        match &self.server {
            Some(s) => {
                ui.label(format!(
                    "Server: {}, {} bots connected",
                    s.url(),
                    connected.len()
                ));
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label("Port");
                    ui.add(egui::DragValue::new(&mut self.port));
                    if ui.button("Start server").clicked() {
                        self.server();
                    }
                });
            }
        }
        ui.separator();
        self.battle_ui(ui, &connected);
        self.results_ui(ui);
        ui.separator();
        egui::CollapsingHeader::new("Bot folders")
            .default_open(true)
            .show(ui, |ui| self.folders_ui(ui));
        egui::CollapsingHeader::new(format!("Connected bots ({})", connected.len())).show(
            ui,
            |ui| {
                for (_, h) in &connected {
                    let lang = h.programming_lang.clone().unwrap_or_default();
                    ui.label(format!("{} {} ({lang})", h.name, h.version))
                        .on_hover_text(h.authors.join(", "));
                }
            },
        );
    }

    fn battle_ui(&mut self, ui: &mut egui::Ui, connected: &[(u64, BotHandshake)]) {
        ui.heading("Battle");
        for side in 0..2 {
            ui.horizontal(|ui| {
                ui.label(RichText::new(if side == 0 { "A" } else { "B" }).color(TANK_COLORS[side]));
                egui::ComboBox::from_id_salt(("royale_pilot", side))
                    .width(270.0)
                    .selected_text(self.choices[side].label())
                    .show_ui(ui, |ui| {
                        for (id, h) in connected {
                            let c = Choice::Remote(*id, h.name.clone());
                            let l = c.label();
                            ui.selectable_value(&mut self.choices[side], c, l);
                        }
                        for l in VETERAN..=MAX_LEVEL {
                            let c = Choice::Scripted(l);
                            let label = c.label();
                            ui.selectable_value(&mut self.choices[side], c, label);
                        }
                        for m in &self.models {
                            let c = Choice::Model(m.clone());
                            let l = c.label();
                            ui.selectable_value(&mut self.choices[side], c, l);
                        }
                    });
            });
        }
        if ui.small_button("Rescan models/").clicked() {
            self.models = royale_models();
        }
        ui.add(egui::Slider::new(&mut self.rounds, 1..=100).text("rounds"));
        ui.horizontal(|ui| {
            ui.add_enabled(
                !self.max_speed,
                egui::Slider::new(&mut self.tps, 5.0..=500.0)
                    .logarithmic(true)
                    .text("turns / s"),
            );
            ui.checkbox(&mut self.max_speed, "max");
        });
        ui.horizontal(|ui| {
            if self.running() {
                if ui.button("Stop battle").clicked() {
                    self.stop.store(true, Ordering::Relaxed);
                }
            } else if ui.button("Start battle").clicked() {
                self.start_battle(connected);
            }
        });
        if let Some(m) = &self.message {
            ui.colored_label(Color32::LIGHT_RED, m);
        }
        ui.small("Our pilots were trained under our own rules; here they play Robocode's.");
    }

    fn results_ui(&self, ui: &mut egui::Ui) {
        let live = self.live.lock().unwrap();
        if live.names.is_empty() {
            return;
        }
        ui.separator();
        ui.heading("Results");
        let r = ranks(&live.results);
        egui::Grid::new("royale_results")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                ui.label("rounds won");
                ui.label("score");
                ui.label("bullet dmg");
                ui.end_row();
                for (k, n) in live.names.iter().enumerate() {
                    let s = live.results.get(k).copied().unwrap_or_default();
                    ui.label(
                        RichText::new(format!("#{} {n}", r.get(k).copied().unwrap_or(1)))
                            .color(TANK_COLORS[k % 2]),
                    );
                    ui.label(s.first_places.to_string());
                    ui.label(format!("{:.0}", s.total()));
                    ui.label(format!("{:.0}", s.bullet_damage));
                    ui.end_row();
                }
            });
        if let Some(t) = &live.turn {
            let state = if live.finished { "finished" } else { "playing" };
            ui.label(format!("Round {}, turn {} ({state})", t.round, t.turn));
        }
        for l in live.log.iter().rev().take(6) {
            ui.small(l);
        }
    }

    fn folders_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.bots_dir).desired_width(200.0));
            if ui.button("Scan").clicked() {
                self.dirs = BotDir::scan(self.bots_dir.as_ref());
            }
        });
        if self.dirs.is_empty() {
            ui.label(
                "Put bot directories here, e.g. the official sample bots \
                 (sample-bots-Java from the Tank Royale releases; Java 11+ needed). \
                 Classic Robocode robots run through the robocode-api-bridge.",
            );
            ui.hyperlink_to(
                "Tank Royale releases",
                "https://github.com/robocode-dev/tank-royale/releases",
            );
        }
        let mut launch = None;
        egui::ScrollArea::vertical()
            .id_salt("bot_dirs")
            .max_height(260.0)
            .show(ui, |ui| {
                egui::Grid::new("bot_dirs").num_columns(2).show(ui, |ui| {
                    for d in &self.dirs {
                        ui.label(&d.name).on_hover_text(&d.description);
                        if ui.small_button("Start").clicked() {
                            launch = Some(d.clone());
                        }
                        ui.end_row();
                    }
                });
            });
        if let Some(d) = launch
            && let Some(s) = self.server()
        {
            match d.launch(&s.url()) {
                Ok(c) => self.started.push((d.name.clone(), c)),
                Err(e) => self.message = Some(format!("{}: {e}", d.name)),
            }
        }
        self.started
            .retain_mut(|(_, c)| matches!(c.try_wait(), Ok(None)));
        if !self.started.is_empty() {
            ui.horizontal(|ui| {
                ui.label(format!("{} bot processes running", self.started.len()));
                if ui.small_button("Stop them").clicked() {
                    for (_, c) in &mut self.started {
                        let _ = c.kill();
                    }
                }
            });
        }
    }

    fn start_battle(&mut self, connected: &[(u64, BotHandshake)]) {
        self.message = None;
        let Some(server) = self.server() else { return };
        let mut entrants = Vec::new();
        let mut names = Vec::new();
        for (k, c) in self.choices.iter().enumerate() {
            match c {
                Choice::Remote(id, name) => {
                    if !connected.iter().any(|(i, _)| i == id) {
                        self.message = Some(format!("{name} is no longer connected"));
                        return;
                    }
                    entrants.push(Entrant::Remote(*id));
                    names.push(name.clone());
                }
                Choice::Scripted(l) => {
                    let bot = ScriptedBot::with_level(k as u64 + 1, *l);
                    entrants.push(Entrant::Local(Box::new(Adapter::new(Driver::Scripted(
                        bot,
                    )))));
                    names.push(c.label());
                }
                Choice::Model(p) => match ModelFile::load(p) {
                    Ok(m) => {
                        entrants.push(Entrant::Local(Box::new(Adapter::new(Driver::Net(
                            m.brain(),
                        )))));
                        names.push(c.label());
                    }
                    Err(e) => {
                        self.message = Some(format!("{}: {e}", p.display()));
                        return;
                    }
                },
            }
        }
        if let [Choice::Remote(a, _), Choice::Remote(b, _)] = &self.choices
            && a == b
        {
            self.message =
                Some("A connected bot can only play one side; start a second copy.".into());
            return;
        }
        let opts = BattleOptions {
            setup: Setup {
                rounds: self.rounds,
                ..Setup::default()
            },
            tps: (!self.max_speed).then_some(self.tps),
            seed: rand_seed(),
        };
        self.stop = Arc::new(AtomicBool::new(false));
        let (live, stop) = (self.live.clone(), self.stop.clone());
        self.battle = Some(std::thread::spawn(move || {
            run_battle(&server, entrants, names, &opts, &live, &stop)
        }));
    }
}

/// Saved models without the periodic `gen_*` snapshots.
fn royale_models() -> Vec<PathBuf> {
    list_models()
        .into_iter()
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("gen_"))
        })
        .collect()
}

fn rand_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// The arena: Robocode coordinates (y up) on screen.
fn draw(ui: &mut egui::Ui, live: &Live) {
    let (w, h) = (
        live.setup.arena_width as f32,
        live.setup.arena_height as f32,
    );
    let avail = ui.available_size();
    let scale = (avail.x / w).min(avail.y / h).max(0.1);
    let (resp, painter) =
        ui.allocate_painter(egui::vec2(w * scale, h * scale), egui::Sense::hover());
    let rect = resp.rect;
    let painter = painter.with_clip_rect(rect);
    let p =
        |x: f64, y: f64| Pos2::new(rect.min.x + x as f32 * scale, rect.max.y - y as f32 * scale);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 30, 36));
    let grid = Color32::from_rgb(38, 40, 48);
    for k in 1..(w as usize / 100) {
        let x = rect.min.x + (k * 100) as f32 * scale;
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(1.0, grid),
        );
    }
    for k in 1..(h as usize / 100) {
        let y = rect.max.y - (k * 100) as f32 * scale;
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(1.0, grid),
        );
    }
    let Some(t) = &live.turn else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Start a battle",
            egui::FontId::proportional(20.0),
            Color32::GRAY,
        );
        return;
    };
    let dir = |a: f64| {
        let r = a.to_radians();
        (r.cos(), r.sin())
    };
    for b in &t.bots {
        let color = TANK_COLORS[(b.id as usize - 1) % 2];
        if !b.alive() {
            continue;
        }
        // The radar arc swept this turn.
        if b.radar_sweep.abs() > 1e-9 {
            let n = 12;
            let mut pts = vec![p(b.x, b.y)];
            for k in 0..=n {
                let a = b.radar_direction - b.radar_sweep * k as f64 / n as f64;
                let (c, s) = dir(a);
                pts.push(p(b.x + c * RADAR_RADIUS, b.y + s * RADAR_RADIUS));
            }
            let fill = color.gamma_multiply(0.10);
            for k in 1..pts.len() - 1 {
                painter.add(Shape::convex_polygon(
                    vec![pts[0], pts[k], pts[k + 1]],
                    fill,
                    Stroke::NONE,
                ));
            }
        }
    }
    for b in &t.bots {
        let color = if b.alive() {
            TANK_COLORS[(b.id as usize - 1) % 2]
        } else {
            Color32::from_gray(70)
        };
        let r = BOT_RADIUS;
        let (c, s) = dir(b.direction);
        let corner = |u: f64, v: f64| p(b.x + u * c - v * s, b.y + u * s + v * c);
        let body = vec![
            corner(-r, -r * 0.8),
            corner(r, -r * 0.8),
            corner(r, r * 0.8),
            corner(-r, r * 0.8),
        ];
        painter.add(Shape::convex_polygon(
            body,
            color.gamma_multiply(0.7),
            Stroke::new(1.5, color),
        ));
        let (gc, gs) = dir(b.gun_direction);
        painter.line_segment(
            [p(b.x, b.y), p(b.x + gc * (r + 8.0), b.y + gs * (r + 8.0))],
            Stroke::new(4.0 * scale.max(0.5), Color32::from_gray(210)),
        );
        painter.circle_filled(p(b.x, b.y), 7.0 * scale, color);
        let (rc, rs) = dir(b.radar_direction);
        painter.line_segment(
            [p(b.x, b.y), p(b.x + rc * 10.0, b.y + rs * 10.0)],
            Stroke::new(2.0, Color32::WHITE),
        );
        // Energy bar and name.
        // Keep the label inside the arena (below the bot near the top edge).
        let above = b.y + r + 30.0 < h as f64;
        let mut top = p(
            b.x,
            if above {
                b.y + r + 14.0
            } else {
                b.y - r - 22.0
            },
        );
        top.x = top.x.clamp(rect.min.x + 60.0, rect.max.x - 60.0);
        let bw = 44.0 * scale.max(0.6);
        let frac = (b.energy.max(0.0) / 100.0).min(1.0) as f32;
        let bar = egui::Rect::from_min_size(top - egui::vec2(bw / 2.0, 0.0), egui::vec2(bw, 4.0));
        painter.rect_filled(bar, 1.0, Color32::from_gray(50));
        painter.rect_filled(
            egui::Rect::from_min_size(bar.min, egui::vec2(bw * frac, 4.0)),
            1.0,
            if frac > 0.3 {
                Color32::from_rgb(90, 200, 110)
            } else {
                Color32::from_rgb(230, 90, 70)
            },
        );
        let name = live.names.get(b.id as usize - 1).map_or("", |s| s.as_str());
        painter.text(
            top - egui::vec2(0.0, 2.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{name}  {:.0}", b.energy.max(0.0)),
            egui::FontId::proportional(11.0),
            Color32::from_gray(200),
        );
    }
    for b in &t.bullets {
        let (x, y) = b.position();
        painter.circle_filled(
            p(x, y),
            (1.5 + b.power as f32) * scale.max(0.6),
            Color32::from_rgb(255, 235, 180),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tab runs a battle between two of our bots and draws it without panicking.
    #[test]
    fn tab_runs_and_draws_a_battle() {
        let mut tab = RoyaleTab::default();
        tab.port = 0;
        tab.max_speed = true;
        tab.rounds = 2;
        tab.start_battle(&[]);
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        for _ in 0..200 {
            let mut out = ctx.run_ui(input(), |ui| tab.ui(ui));
            out.textures_delta.clear();
            if tab.live.lock().unwrap().finished {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let live = tab.live.lock().unwrap();
        assert!(live.turn.is_some(), "no turn played: {:?}", tab.message);
        assert_eq!(live.names.len(), 2);
    }
}
