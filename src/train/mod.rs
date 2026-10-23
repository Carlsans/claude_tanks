//! Training runs on a background thread; the GUI talks to it through channels.

pub mod eval;
pub mod ga;
pub mod neat_evo;
pub mod ppo;

use crate::bots::scripted::{LEVELS, MAX_LEVEL};
use crate::nn::{Arch, Brain, Genes, ModelFile, models_dir};
use crate::rng::Rng;
use crossbeam_channel::{Receiver, Sender};
use eval::{Opponent, Record, Shaping};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Champion win rate against the current curriculum level needed to move up.
pub const PROMOTE_WIN_RATE: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Method {
    Ga,
    Neat,
    Ppo,
}

impl Method {
    pub const ALL: [Method; 3] = [Method::Ga, Method::Neat, Method::Ppo];
    pub fn name(self) -> &'static str {
        match self {
            Method::Ga => "GA",
            Method::Neat => "NEAT",
            Method::Ppo => "PPO",
        }
    }
    /// Architectures this method can train.
    pub fn supports(self, arch: Arch) -> bool {
        match self {
            Method::Ga => arch != Arch::Neat,
            Method::Neat => arch == Arch::Neat,
            Method::Ppo => matches!(arch, Arch::MlpS | Arch::MlpL | Arch::Gru),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TrainConfig {
    pub run_name: String,
    pub arch: Arch,
    pub method: Method,
    pub seed: u64,
    /// Population size (GA / NEAT) or parallel environments (PPO).
    pub population: usize,
    /// Matches per candidate per generation.
    pub matches: usize,
    pub density: (f32, f32),
    pub threads: usize,
    /// Stop after this many simulated ticks (0 = no limit).
    pub max_steps: u64,
    /// Stop after this many generations (0 = no limit).
    pub max_generations: u32,
    /// Fraction of opponents that are the scripted bot (rest: hall of fame).
    pub scripted_share: f32,
    pub hof_size: usize,
    pub shaping_start: f32,
    pub shaping_end: f32,
    pub shaping_gens: u32,
    /// Matches vs the scripted bot used to score the champion each generation.
    pub eval_matches: usize,
    pub ga: ga::GaParams,
    pub neat: neat_evo::NeatParams,
    pub ppo: ppo::PpoParams,
}

impl Default for TrainConfig {
    fn default() -> Self {
        TrainConfig {
            run_name: String::new(),
            arch: Arch::MlpS,
            method: Method::Ga,
            seed: 1,
            population: 96,
            matches: 8,
            density: (0.0, 1.0),
            threads: std::thread::available_parallelism().map_or(4, |n| n.get()),
            max_steps: 0,
            max_generations: 0,
            scripted_share: 0.5,
            hof_size: 20,
            shaping_start: 0.6,
            shaping_end: 0.15,
            shaping_gens: 300,
            eval_matches: 48,
            ga: ga::GaParams::default(),
            neat: neat_evo::NeatParams::default(),
            ppo: ppo::PpoParams::default(),
        }
    }
}

impl TrainConfig {
    pub fn shaping(&self, generation: u32) -> Shaping {
        let k = if self.shaping_gens == 0 {
            1.0
        } else {
            (generation as f32 / self.shaping_gens as f32).min(1.0)
        };
        Shaping {
            weight: self.shaping_start + (self.shaping_end - self.shaping_start) * k,
        }
    }

    pub fn run_dir(&self) -> PathBuf {
        models_dir().join(&self.run_name)
    }

    pub fn default_run_name(arch: Arch, method: Method) -> String {
        format!(
            "{}_{}_{}",
            arch.short(),
            method.name().to_lowercase(),
            timestamp()
        )
    }
}

/// UTC "YYYYMMDD-HHMMSS" without pulling in a date crate.
pub fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// One generation's numbers, sent to the GUI and kept in the run history.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct GenStats {
    pub generation: u32,
    pub steps: u64,
    pub seconds: f64,
    pub best_fitness: f32,
    pub mean_fitness: f32,
    pub shaping: f32,
    /// Curriculum level of the scripted training opponents.
    pub level: u8,
    pub vs_scripted: Record,
    /// Method-specific numbers (species, losses, network size...).
    pub extra: Vec<(String, f32)>,
}

/// What a trainer reports after one generation / iteration.
pub struct StepReport {
    pub best_fitness: f32,
    pub mean_fitness: f32,
    pub champion: Genes,
    pub ticks: u64,
    pub extra: Vec<(String, f32)>,
}

/// Everything a trainer needs for one generation.
pub struct StepEnv<'a> {
    pub cfg: &'a TrainConfig,
    pub generation: u32,
    pub opponents: Vec<Opponent>,
    pub terrains: Vec<crate::sim::Terrain>,
    pub shaping: Shaping,
    pub rng: &'a mut Rng,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Trainer {
    Ga(ga::Ga),
    Neat(neat_evo::Neat),
    Ppo(Box<ppo::Ppo>),
}

impl Trainer {
    fn new(cfg: &TrainConfig, rng: &mut Rng) -> Trainer {
        match cfg.method {
            Method::Ga => Trainer::Ga(ga::Ga::new(cfg, rng)),
            Method::Neat => Trainer::Neat(neat_evo::Neat::new(cfg, rng)),
            Method::Ppo => Trainer::Ppo(Box::new(ppo::Ppo::new(cfg, rng))),
        }
    }
    fn step(&mut self, env: StepEnv) -> StepReport {
        match self {
            Trainer::Ga(t) => t.step(env),
            Trainer::Neat(t) => t.step(env),
            Trainer::Ppo(t) => t.step(env),
        }
    }
}

/// Complete resumable state of a run (saved as `run.state`).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RunState {
    pub cfg: TrainConfig,
    pub generation: u32,
    pub steps: u64,
    pub seconds: f64,
    pub rng: Rng,
    pub hof: Vec<Genes>,
    pub best_score: f32,
    /// Current scripted-opponent curriculum level.
    pub level: u8,
    pub history: Vec<GenStats>,
    pub trainer: Trainer,
}

impl RunState {
    pub fn path(dir: &std::path::Path) -> PathBuf {
        dir.join("run.state")
    }
    pub fn load(dir: &std::path::Path) -> Result<RunState, String> {
        let s = std::fs::read_to_string(Self::path(dir)).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }
    fn save(&self) -> std::io::Result<()> {
        let dir = self.cfg.run_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("run.state.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_string(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, Self::path(&dir))
    }
}

/// Runs that can be resumed (have a `run.state`).
pub fn list_runs() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(models_dir()) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| RunState::path(p).exists())
        .collect();
    v.sort();
    v.reverse();
    v
}

pub enum Cmd {
    Pause,
    Resume,
    Stop,
}

pub enum Msg {
    /// Full history so far (sent once at start, e.g. after resuming).
    History(Vec<GenStats>),
    Stats(GenStats),
    Log(String),
    /// Training thread ended (budget reached, stopped, or error).
    Finished,
}

pub struct TrainHandle {
    pub cmd: Sender<Cmd>,
    pub msg: Receiver<Msg>,
    pub cfg: TrainConfig,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TrainHandle {
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
}

impl Drop for TrainHandle {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Start a new run, or resume one when `resume` is given.
pub fn spawn(cfg: TrainConfig, resume: Option<RunState>) -> TrainHandle {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (msg_tx, msg_rx) = crossbeam_channel::unbounded();
    let cfg_copy = resume
        .as_ref()
        .map_or_else(|| cfg.clone(), |r| r.cfg.clone());
    let thread = std::thread::Builder::new()
        .name("trainer".into())
        .spawn(move || {
            let threads = cfg_copy_threads(&cfg, resume.as_ref());
            match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
                Ok(pool) => pool.install(|| run(cfg, resume, cmd_rx, &msg_tx)),
                Err(e) => {
                    let _ = msg_tx.send(Msg::Log(format!("thread pool error: {e}")));
                }
            }
            let _ = msg_tx.send(Msg::Finished);
        })
        .expect("spawn trainer thread");
    TrainHandle {
        cmd: cmd_tx,
        msg: msg_rx,
        cfg: cfg_copy,
        thread: Some(thread),
    }
}

fn cfg_copy_threads(cfg: &TrainConfig, resume: Option<&RunState>) -> usize {
    resume.map_or(cfg.threads, |r| r.cfg.threads).max(1)
}

/// Pick opponents for one generation: scripted bots plus hall-of-fame members.
fn opponents(cfg: &TrainConfig, hof: &[Genes], level: u8, rng: &mut Rng) -> Vec<Opponent> {
    (0..cfg.matches)
        .map(|_| {
            if hof.is_empty() || rng.chance(cfg.scripted_share) {
                Opponent::Scripted(rng.next_u64(), level)
            } else {
                // Favour recent champions.
                let n = hof.len();
                let k = n - 1 - rng.below(n).min(rng.below(n));
                Opponent::Net(cfg.arch, Arc::new(hof[k].clone()))
            }
        })
        .collect()
}

fn model(cfg: &TrainConfig, genes: Genes, s: &GenStats) -> ModelFile {
    ModelFile {
        name: cfg.run_name.clone(),
        arch: cfg.arch,
        method: cfg.method.name().into(),
        generation: s.generation,
        fitness: s.best_fitness,
        vs_scripted: s.vs_scripted.win,
        genes,
    }
}

fn run(cfg: TrainConfig, resume: Option<RunState>, cmd: Receiver<Cmd>, tx: &Sender<Msg>) {
    let mut st = match resume {
        Some(st) => {
            let _ = tx.send(Msg::Log(format!(
                "Resumed {} at generation {}",
                st.cfg.run_name, st.generation
            )));
            st
        }
        None => {
            let mut rng = Rng::new(cfg.seed);
            let trainer = Trainer::new(&cfg, &mut rng);
            let _ = tx.send(Msg::Log(format!(
                "Started {} ({} / {}, {} params)",
                cfg.run_name,
                cfg.arch.name(),
                cfg.method.name(),
                cfg.arch.param_count()
            )));
            RunState {
                cfg,
                generation: 0,
                steps: 0,
                seconds: 0.0,
                rng,
                hof: Vec::new(),
                best_score: f32::NEG_INFINITY,
                level: 0,
                history: Vec::new(),
                trainer,
            }
        }
    };
    let _ = tx.send(Msg::History(st.history.clone()));
    let dir = st.cfg.run_dir();
    let mut paused = false;
    loop {
        // Commands.
        loop {
            let c = if paused {
                cmd.recv().ok()
            } else {
                cmd.try_recv().ok()
            };
            match c {
                Some(Cmd::Pause) => {
                    paused = true;
                    let _ = st.save();
                    let _ = tx.send(Msg::Log("Paused (state saved)".into()));
                }
                Some(Cmd::Resume) => {
                    paused = false;
                    let _ = tx.send(Msg::Log("Resumed".into()));
                }
                Some(Cmd::Stop) | None if paused => {
                    let _ = st.save();
                    return;
                }
                Some(Cmd::Stop) => {
                    let _ = st.save();
                    let _ = tx.send(Msg::Log("Stopped (state saved)".into()));
                    return;
                }
                None => break,
            }
        }
        let cfg = st.cfg.clone();
        if (cfg.max_steps > 0 && st.steps >= cfg.max_steps)
            || (cfg.max_generations > 0 && st.generation >= cfg.max_generations)
        {
            let _ = st.save();
            let _ = tx.send(Msg::Log("Budget reached".into()));
            return;
        }

        let t0 = Instant::now();
        let shaping = cfg.shaping(st.generation);
        let opps = opponents(&cfg, &st.hof, st.level, &mut st.rng);
        let terrains = eval::terrains(&mut st.rng, cfg.matches, cfg.density);
        let rep = st.trainer.step(StepEnv {
            cfg: &cfg,
            generation: st.generation,
            opponents: opps,
            terrains,
            shaping,
            rng: &mut st.rng,
        });

        let champ = rep.champion.clone();
        let arch = cfg.arch;
        let make =
            || -> Box<dyn crate::sim::world::Controller> { Box::new(Brain::new(arch, &champ)) };
        let (record, mut eval_ticks) = eval::vs_scripted(&make, cfg.eval_matches, MAX_LEVEL);
        let level = st.level;
        let mut promoted = None;
        if st.level < MAX_LEVEL {
            let (r, t) = eval::vs_scripted(&make, cfg.eval_matches, st.level);
            eval_ticks += t;
            if r.win >= PROMOTE_WIN_RATE {
                st.level += 1;
                promoted = Some(format!(
                    "Gen {}: curriculum -> {} (beat {} {:.0}%)",
                    st.generation,
                    LEVELS[st.level as usize],
                    LEVELS[level as usize],
                    r.win * 100.0
                ));
            }
        }
        st.steps += rep.ticks + eval_ticks;
        st.seconds += t0.elapsed().as_secs_f64();

        // Hall of fame: keep the most recent champions, thinning older ones.
        st.hof.push(rep.champion.clone());
        if st.hof.len() > cfg.hof_size.max(1) {
            let k = st.rng.below(st.hof.len() - 1);
            st.hof.remove(k);
        }

        let stats = GenStats {
            generation: st.generation,
            steps: st.steps,
            seconds: st.seconds,
            best_fitness: rep.best_fitness,
            mean_fitness: rep.mean_fitness,
            shaping: shaping.weight,
            level,
            vs_scripted: record,
            extra: rep.extra,
        };
        let m = model(&cfg, rep.champion, &stats);
        let score = record.win - record.loss;
        let mut log = None;
        if score > st.best_score {
            st.best_score = score;
            if m.save(&dir.join("best.json")).is_ok() {
                log = Some(format!(
                    "Gen {}: new best vs scripted (win {:.0}%, loss {:.0}%) saved",
                    st.generation,
                    record.win * 100.0,
                    record.loss * 100.0
                ));
            }
        }
        let _ = m.save(&dir.join("latest.json"));
        if st.generation > 0 && st.generation.is_multiple_of(25) {
            let _ = m.save(&dir.join(format!("gen_{:05}.json", st.generation)));
        }
        st.history.push(stats.clone());
        st.generation += 1;
        if st.generation.is_multiple_of(10) {
            let _ = st.save();
        }
        let _ = tx.send(Msg::Stats(stats));
        for l in [promoted, log].into_iter().flatten() {
            let _ = tx.send(Msg::Log(l));
        }
    }
}
