//! Architecture comparison: equal-budget training for each (architecture, method),
//! then a round-robin tournament between the champions and the scripted bot.

use super::eval::{Record, head_to_head};
use super::{GenStats, Method, Msg, TrainConfig, spawn};
use crate::bots::ScriptedBot;
use crate::bots::scripted::{ACE, VETERAN};
use crate::nn::{Arch, ModelFile, models_dir};
use crate::sim::world::Controller;
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BenchConfig {
    pub entries: Vec<(Arch, Method)>,
    /// Simulated ticks per entry (0 = no limit).
    pub budget_steps: u64,
    /// Training seconds per entry (0 = no limit).
    #[serde(default)]
    pub budget_seconds: f64,
    /// Matches per pairing in the tournament.
    pub matches: usize,
    pub threads: usize,
    pub seed: u64,
}

impl Default for BenchConfig {
    fn default() -> Self {
        BenchConfig {
            entries: all_entries(),
            budget_steps: 0,
            budget_seconds: 360.0,
            matches: 60,
            threads: std::thread::available_parallelism().map_or(4, |n| n.get()),
            seed: 7,
        }
    }
}

pub fn all_entries() -> Vec<(Arch, Method)> {
    Method::ALL
        .into_iter()
        .flat_map(|m| {
            Arch::ALL
                .into_iter()
                .filter(move |a| m.supports(*a))
                .map(move |a| (a, m))
        })
        .collect()
}

pub fn entry_name(a: Arch, m: Method) -> String {
    format!("{} / {}", a.name(), m.name())
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Contestant {
    pub name: String,
    pub model: Option<PathBuf>,
    pub elo: f32,
    pub vs_scripted: Record,
    pub train_seconds: f64,
    pub generations: u32,
    #[serde(default)]
    pub train_steps: u64,
    /// Scripted level for contestants without a model (veteran when absent).
    #[serde(default)]
    pub level: Option<u8>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BenchResults {
    pub config: Option<BenchConfig>,
    pub contestants: Vec<Contestant>,
    /// score[i][j]: points of i against j (win 1, draw 0.5) / matches.
    pub score: Vec<Vec<f32>>,
    pub finished: String,
}

impl BenchResults {
    pub fn path() -> PathBuf {
        PathBuf::from("results").join("benchmark.json")
    }
    pub fn load() -> Option<BenchResults> {
        serde_json::from_str(&std::fs::read_to_string(Self::path()).ok()?).ok()
    }
    fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all("results")?;
        std::fs::write(
            Self::path(),
            serde_json::to_string_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::write("results/benchmark.md", self.markdown())
    }

    pub fn ranking(&self) -> Vec<usize> {
        let mut r: Vec<usize> = (0..self.contestants.len()).collect();
        r.sort_by(|&a, &b| self.contestants[b].elo.total_cmp(&self.contestants[a].elo));
        r
    }

    pub fn markdown(&self) -> String {
        let mut s = String::from(
            "| Rank | Pilot | Elo | vs scripted (W/D/L %) | Train time |\n|---|---|---|---|---|\n",
        );
        for (rank, &i) in self.ranking().iter().enumerate() {
            let c = &self.contestants[i];
            let (vs, time) = if c.model.is_some() {
                (
                    format!(
                        "{:.0} / {:.0} / {:.0}",
                        c.vs_scripted.win * 100.0,
                        c.vs_scripted.draw * 100.0,
                        c.vs_scripted.loss * 100.0
                    ),
                    format!("{:.0} s", c.train_seconds),
                )
            } else {
                ("-".into(), "-".into())
            };
            s += &format!(
                "| {} | {} | {:.0} | {vs} | {time} |\n",
                rank + 1,
                c.name,
                c.elo
            );
        }
        if let Some(cfg) = &self.config {
            s += &format!(
                "\nBudget per entry: {}{}; {} matches per pairing. {}\n",
                if cfg.budget_seconds > 0.0 {
                    format!("{:.0} s of training", cfg.budget_seconds)
                } else {
                    String::new()
                },
                if cfg.budget_steps > 0 {
                    format!(" {} M simulated ticks", cfg.budget_steps / 1_000_000)
                } else {
                    String::new()
                },
                cfg.matches,
                self.finished
            );
        }
        s
    }
}

/// Bradley-Terry / Elo fit from pairwise scores; the last contestant is anchored at 1000.
pub fn fit_elo(score: &[Vec<f32>]) -> Vec<f32> {
    let n = score.len();
    let mut r = vec![1000.0f32; n];
    for _ in 0..2000 {
        let mut grad = vec![0.0f32; n];
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let e = 1.0 / (1.0 + 10f32.powf((r[j] - r[i]) / 400.0));
                    // Clamp so a perfect record still gives a finite rating.
                    grad[i] += score[i][j].clamp(0.02, 0.98) - e;
                }
            }
        }
        for i in 0..n {
            r[i] += 8.0 * grad[i];
        }
        let shift = r[n - 1] - 1000.0;
        r.iter_mut().for_each(|v| *v -= shift);
    }
    r
}

pub enum BenchMsg {
    /// Entry index started.
    Entry(usize),
    Stats(usize, GenStats),
    Log(String),
    Results(BenchResults),
    Finished,
}

pub struct BenchHandle {
    pub msg: Receiver<BenchMsg>,
    stop: Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl BenchHandle {
    pub fn stop(&self) {
        let _ = self.stop.send(());
    }
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

impl Drop for BenchHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn spawn_benchmark(cfg: BenchConfig) -> BenchHandle {
    let (tx, rx) = crossbeam_channel::unbounded();
    let (stop_tx, stop_rx) = crossbeam_channel::unbounded();
    let thread = std::thread::spawn(move || {
        run(cfg, &tx, &stop_rx);
        let _ = tx.send(BenchMsg::Finished);
    });
    BenchHandle {
        msg: rx,
        stop: stop_tx,
        thread: Some(thread),
    }
}

fn run(cfg: BenchConfig, tx: &Sender<BenchMsg>, stop: &Receiver<()>) {
    let stamp = super::timestamp();
    let mut contestants = Vec::new();
    for (k, &(arch, method)) in cfg.entries.iter().enumerate() {
        let _ = tx.send(BenchMsg::Entry(k));
        let tc = TrainConfig {
            run_name: format!(
                "bench_{stamp}/{}_{}",
                arch.short(),
                method.name().to_lowercase()
            ),
            arch,
            method,
            seed: cfg.seed + k as u64,
            max_steps: cfg.budget_steps,
            max_seconds: cfg.budget_seconds,
            threads: cfg.threads,
            ..Default::default()
        };
        let dir = tc.run_dir();
        let h = spawn(tc, None);
        let mut last = GenStats::default();
        loop {
            if stop.try_recv().is_ok() {
                let _ = h.cmd.send(super::Cmd::Stop);
                drop(h);
                let _ = tx.send(BenchMsg::Log("Benchmark stopped.".into()));
                return;
            }
            match h.msg.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(Msg::Stats(s)) => {
                    last = s.clone();
                    let _ = tx.send(BenchMsg::Stats(k, s));
                }
                Ok(Msg::Log(l)) => {
                    let _ = tx.send(BenchMsg::Log(format!("[{}] {l}", entry_name(arch, method))));
                }
                Ok(Msg::Finished) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                _ => {}
            }
        }
        let best = dir.join("best.json");
        contestants.push(Contestant {
            name: entry_name(arch, method),
            model: best.exists().then_some(best),
            elo: 0.0,
            vs_scripted: last.vs_scripted,
            train_seconds: last.seconds,
            generations: last.generation + 1,
            train_steps: last.steps,
            level: None,
        });
    }
    // The veteran goes last: the Elo fit anchors the last contestant at 1000.
    contestants.push(Contestant {
        name: "Scripted bot (ace)".into(),
        level: Some(ACE),
        ..Default::default()
    });
    contestants.push(Contestant {
        name: "Scripted bot (veteran)".into(),
        level: Some(VETERAN),
        ..Default::default()
    });

    let _ = tx.send(BenchMsg::Log("Tournament...".into()));
    let models: Vec<Option<ModelFile>> = contestants
        .iter()
        .map(|c| c.model.as_ref().and_then(|p| ModelFile::load(p).ok()))
        .collect();
    let make = |i: usize| -> Box<dyn Controller> {
        match &models[i] {
            Some(m) => Box::new(m.brain()),
            None => Box::new(ScriptedBot::with_level(
                i as u64,
                contestants[i].level.unwrap_or(VETERAN),
            )),
        }
    };
    let n = contestants.len();
    let mut score = vec![vec![0.5f32; n]; n];
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.threads.max(1))
        .build()
        .unwrap();
    let pairs: Vec<(usize, usize)> = (0..n)
        .flat_map(|i| ((i + 1)..n).map(move |j| (i, j)))
        .collect();
    for (i, j) in pairs {
        if stop.try_recv().is_ok() {
            return;
        }
        let seed = 5_000_000 + cfg.seed * 1000;
        let r = pool.install(|| head_to_head(&|| make(i), &|| make(j), cfg.matches, seed));
        score[i][j] = r.win + 0.5 * r.draw;
        score[j][i] = r.loss + 0.5 * r.draw;
    }
    let elo = fit_elo(&score);
    for (c, e) in contestants.iter_mut().zip(&elo) {
        c.elo = *e;
    }

    // Keep champions in a stable, committed location.
    let champ_dir = models_dir().join("champions");
    for (c, m) in contestants.iter_mut().zip(&models) {
        if let Some(m) = m {
            let p = champ_dir.join(format!(
                "{}_{}.json",
                m.arch.short(),
                m.method.to_lowercase()
            ));
            if m.save(&p).is_ok() {
                c.model = Some(p);
            }
        }
    }
    let results = BenchResults {
        config: Some(cfg),
        contestants,
        score,
        finished: format!("Finished {} UTC.", super::timestamp()),
    };
    if let Err(e) = results.save() {
        let _ = tx.send(BenchMsg::Log(format!("Could not save results: {e}")));
    }
    let _ = tx.send(BenchMsg::Results(results));
}
