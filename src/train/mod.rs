//! Training runs on a background thread; the GUI talks to it through channels.

pub mod benchmark;
pub mod es;
pub mod eval;
pub mod ga;
pub mod imitation;
pub mod neat_evo;
pub mod ppo;

use crate::bots::scripted::{ACE, LEVELS, MAX_LEVEL, VETERAN};
use crate::nn::{Arch, Brain, Genes, ModelFile, models_dir};
use crate::rng::Rng;
use crate::royale::pool::RobotPool;
use crate::royale::server::{BotDir, sandbox_name};
use crate::royale::sparring::Sparring;
use crate::sim::Rules;
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
    Es,
    Neat,
    Ppo,
}

impl Method {
    pub const ALL: [Method; 4] = [Method::Ga, Method::Es, Method::Neat, Method::Ppo];
    pub fn name(self) -> &'static str {
        match self {
            Method::Ga => "GA",
            Method::Es => "ES",
            Method::Neat => "NEAT",
            Method::Ppo => "PPO",
        }
    }
    /// Whether this method can train under these rules (Robocode duels are
    /// only wired into the population methods).
    pub fn supports_rules(self, rules: Rules) -> bool {
        rules == Rules::Tanks || matches!(self, Method::Ga | Method::Es)
    }

    /// Architectures this method can train.
    pub fn supports(self, arch: Arch) -> bool {
        match self {
            Method::Ga | Method::Es => arch != Arch::Neat,
            Method::Neat => arch == Arch::Neat,
            Method::Ppo => arch != Arch::Neat,
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
    /// Stop after this many seconds of training (0 = no limit).
    #[serde(default)]
    pub max_seconds: f64,
    /// Batch GA/ES network evaluation on the GPU when one is available.
    #[serde(default = "yes")]
    pub use_gpu: bool,
    /// Start from this saved model (same architecture) instead of imitation.
    #[serde(default)]
    pub init_model: String,
    /// Curriculum level to start at (e.g. `MAX_LEVEL` to fine-tune a trained
    /// model against the full mix of bots from the veteran up).
    #[serde(default)]
    pub start_level: u8,
    /// Highest curriculum level (the newer bots can be left out).
    #[serde(default = "max_level")]
    pub top_level: u8,
    /// Past this level, scripted opponents mix every level from here up to the
    /// current one.
    #[serde(default = "veteran")]
    pub mix_from: u8,
    /// Draw the scripted bots the network loses to more often: each level's
    /// weight is (1 - recent score)^bot_focus, with score = win + draw / 2.
    /// 0 draws them evenly.
    #[serde(default)]
    pub bot_focus: f32,
    /// Fraction of opponents that are the scripted bot (rest: league and hall of fame).
    pub scripted_share: f32,
    pub hof_size: usize,
    /// Fixed opponents: saved models of any architecture (the "league").
    #[serde(default)]
    pub league: Vec<String>,
    /// Fraction of the non-scripted opponents drawn from the league (rest: hall of fame).
    #[serde(default)]
    pub league_share: f32,
    /// ES: also keep an exponential moving average of the weights (saved as
    /// `ema.json`); 0 turns it off.
    #[serde(default = "ema_default")]
    pub ema_decay: f32,
    pub shaping_start: f32,
    pub shaping_end: f32,
    pub shaping_gens: u32,
    /// Matches vs the scripted bot used to score the champion each generation.
    pub eval_matches: usize,
    /// Our game, or Robocode's (GA and ES only; no imitation, no GPU).
    #[serde(default)]
    pub rules: Rules,
    /// Robocode rules: real bots (bot directories) to battle in training, each
    /// kept running in `robot_slots` processes. Empty: sample-bot ports only.
    #[serde(default)]
    pub robots: Vec<String>,
    /// Battles per candidate and generation against the robots (both members
    /// of an ES pair meet the same robot).
    #[serde(default = "one")]
    pub robot_battles: usize,
    /// Rounds per robot battle; each round weighs as much as one duel.
    #[serde(default = "three")]
    pub robot_rounds: u32,
    #[serde(default = "two")]
    pub robot_slots: usize,
    /// Robots running at once (0: all). When fewer than `robots`, the run
    /// moves to the next group every `robot_rotate` generations.
    #[serde(default)]
    pub robot_active: usize,
    #[serde(default = "twenty_five")]
    pub robot_rotate: u32,
    /// Warm start fixed-topology nets by imitating the scripted bot.
    pub imitation: imitation::ImitationParams,
    pub ga: ga::GaParams,
    pub es: es::EsParams,
    pub neat: neat_evo::NeatParams,
    pub ppo: ppo::PpoParams,
}

fn yes() -> bool {
    true
}

fn ema_default() -> f32 {
    0.99
}

fn max_level() -> u8 {
    MAX_LEVEL
}

fn veteran() -> u8 {
    VETERAN
}

fn one() -> usize {
    1
}

fn two() -> usize {
    2
}

fn three() -> u32 {
    3
}

fn twenty_five() -> u32 {
    25
}

impl Default for TrainConfig {
    fn default() -> Self {
        TrainConfig {
            run_name: String::new(),
            arch: Arch::MlpS,
            method: Method::Ga,
            seed: 1,
            population: 96,
            matches: 12,
            density: (0.0, 1.0),
            threads: std::thread::available_parallelism().map_or(4, |n| n.get()),
            max_steps: 0,
            max_generations: 0,
            max_seconds: 0.0,
            use_gpu: true,
            init_model: String::new(),
            start_level: 0,
            top_level: MAX_LEVEL,
            mix_from: VETERAN,
            bot_focus: 0.0,
            scripted_share: 0.5,
            hof_size: 20,
            league: Vec::new(),
            league_share: 0.0,
            ema_decay: ema_default(),
            shaping_start: 0.6,
            shaping_end: 0.15,
            shaping_gens: 300,
            eval_matches: 96,
            rules: Rules::Tanks,
            robots: Vec::new(),
            robot_battles: one(),
            robot_rounds: three(),
            robot_slots: two(),
            robot_active: 0,
            robot_rotate: twenty_five(),
            imitation: imitation::ImitationParams::default(),
            ga: ga::GaParams::default(),
            es: es::EsParams::default(),
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
    /// Champion vs the ace on the same benchmark set.
    #[serde(default)]
    pub vs_ace: Record,
    /// Champion vs the bots above the ace that the curriculum has reached
    /// (average record; empty before then).
    #[serde(default)]
    pub vs_top: Record,
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
    /// Evaluate this generation's networks on the GPU (chosen by `DeviceChoice`).
    pub use_gpu: bool,
    pub generation: u32,
    pub opponents: Vec<Opponent>,
    pub terrains: Vec<crate::sim::Terrain>,
    pub shaping: Shaping,
    pub rng: &'a mut Rng,
    /// Real bots to battle (Robocode rules), and the robots each member pair
    /// meets this generation: pair p plays `robot_plan[p * robot_battles..][..robot_battles]`.
    pub robots: Option<&'a crate::royale::pool::RobotPool>,
    pub robot_plan: Vec<usize>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub enum Trainer {
    Ga(ga::Ga),
    Es(es::Es),
    Neat(neat_evo::Neat),
    Ppo(Box<ppo::Ppo>),
}

impl Trainer {
    fn new(cfg: &TrainConfig, rng: &mut Rng, init: Option<Vec<f32>>) -> Trainer {
        match cfg.method {
            Method::Ga => Trainer::Ga(ga::Ga::new(cfg, rng, init)),
            Method::Es => Trainer::Es(es::Es::new(cfg, rng, init)),
            Method::Neat => Trainer::Neat(neat_evo::Neat::new(cfg, rng)),
            Method::Ppo => Trainer::Ppo(Box::new(ppo::Ppo::new(cfg, rng, init))),
        }
    }
    fn step(&mut self, env: StepEnv) -> StepReport {
        match self {
            Trainer::Ga(t) => t.step(env),
            Trainer::Es(t) => t.step(env),
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
    /// Moving average of the ES weights (see `TrainConfig::ema_decay`).
    #[serde(default)]
    pub ema: Option<Vec<f32>>,
    /// Recent score (win + draw / 2, smoothed) against each scripted level.
    #[serde(default)]
    pub bot_scores: Vec<f32>,
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

/// Which group of `robot_active` robots runs at `generation`.
fn robot_group_at(cfg: &TrainConfig, generation: u32) -> usize {
    if cfg.robot_active == 0 || cfg.robot_active >= cfg.robots.len() {
        0
    } else {
        (generation / cfg.robot_rotate.max(1)) as usize
    }
}

/// The robots of group `group`: consecutive slices of the robots in a fixed
/// shuffled order, wrapping around at the end.
fn robot_group_dirs(cfg: &TrainConfig, group: usize) -> Vec<String> {
    let n = cfg.robots.len();
    if cfg.robot_active == 0 || cfg.robot_active >= n {
        return cfg.robots.clone();
    }
    let mut order: Vec<usize> = (0..n).collect();
    let mut rng = Rng::new(cfg.seed ^ 0x0B07_5EED);
    for i in (1..n).rev() {
        order.swap(i, rng.below(i + 1));
    }
    let from = group * cfg.robot_active;
    (from..from + cfg.robot_active)
        .map(|k| cfg.robots[order[k % n]].clone())
        .collect()
}

/// Start real bots (Robocode rules); unreadable directories are reported and
/// skipped.
fn start_robots(cfg: &TrainConfig, dirs: &[String], tx: &Sender<Msg>) -> Option<RobotPool> {
    if cfg.rules != Rules::Robocode || dirs.is_empty() {
        return None;
    }
    let bots: Vec<BotDir> = dirs
        .iter()
        .filter_map(|d| {
            let b = BotDir::read(d.as_ref());
            if b.is_none() {
                let _ = tx.send(Msg::Log(format!("Robots: not a bot directory: {d}")));
            }
            b
        })
        .collect();
    if bots.is_empty() {
        return None;
    }
    let n = bots.len();
    match RobotPool::start(bots, cfg.robot_slots) {
        Ok(p) => {
            let _ = tx.send(Msg::Log(format!(
                "Robots: {n} of {} bots running, {} processes, sandbox: {}",
                cfg.robots.len(),
                p.slots(),
                sandbox_name().unwrap_or("none")
            )));
            Some(p)
        }
        Err(e) => {
            let _ = tx.send(Msg::Log(format!("Robots: cannot start: {e}")));
            None
        }
    }
}

/// Load the league models; unreadable files are reported and skipped.
fn load_league(cfg: &TrainConfig, tx: &Sender<Msg>) -> Vec<Opponent> {
    cfg.league
        .iter()
        .filter_map(|p| match ModelFile::load(p.as_ref()) {
            Ok(m) => Some(Opponent::Net(m.arch, Arc::new(m.genes))),
            Err(e) => {
                let _ = tx.send(Msg::Log(format!("League: cannot load {p}: {e}")));
                None
            }
        })
        .collect()
}

/// Pick opponents for one generation: scripted bots, league models and
/// hall-of-fame members.
fn opponents(
    cfg: &TrainConfig,
    hof: &[Genes],
    league: &[Opponent],
    level: u8,
    bot_scores: &[f32],
    rng: &mut Rng,
) -> Vec<Opponent> {
    (0..cfg.matches)
        .map(|_| {
            let scripted = (hof.is_empty() && league.is_empty()) || rng.chance(cfg.scripted_share);
            if scripted && cfg.rules == Rules::Robocode {
                // The sample-bot ports; `bot_scores` is indexed like `Sparring::ALL`.
                let k = if cfg.bot_focus > 0.0 {
                    pick_weighted(&focus_weights(cfg, bot_scores, 0..Sparring::ALL.len()), rng)
                } else {
                    rng.below(Sparring::ALL.len())
                };
                Opponent::Sparring(Sparring::ALL[k])
            } else if scripted {
                // Past the veteran, keep playing every level from `mix_from` up:
                // training against the ace alone made nets forget the veteran.
                let from = cfg.mix_from.min(level);
                let lvl = if level > from && cfg.bot_focus > 0.0 {
                    let levels = from as usize..=level as usize;
                    from + pick_weighted(&focus_weights(cfg, bot_scores, levels), rng) as u8
                } else if level > from {
                    from + rng.below((level - from + 1) as usize) as u8
                } else {
                    level
                };
                Opponent::Scripted(rng.next_u64(), lvl)
            } else if !league.is_empty() && (hof.is_empty() || rng.chance(cfg.league_share)) {
                league[rng.below(league.len())].clone()
            } else {
                // Favour recent champions.
                let n = hof.len();
                let k = n - 1 - rng.below(n).min(rng.below(n));
                Opponent::Net(cfg.arch, Arc::new(hof[k].clone()))
            }
        })
        .collect()
}

/// `bot_focus` weights of the bots at these indices of `bot_scores`.
fn focus_weights(
    cfg: &TrainConfig,
    bot_scores: &[f32],
    idx: impl Iterator<Item = usize>,
) -> Vec<f32> {
    idx.map(|i| {
        let score = bot_scores.get(i).copied().unwrap_or(0.5);
        (1.0 - score).max(0.02).powf(cfg.bot_focus)
    })
    .collect()
}

/// Index drawn with probability proportional to its weight.
fn pick_weighted(w: &[f32], rng: &mut Rng) -> usize {
    let mut x = rng.f32() * w.iter().sum::<f32>();
    let mut k = 0;
    while k + 1 < w.len() && x >= w[k] {
        x -= w[k];
        k += 1;
    }
    k
}

fn model(cfg: &TrainConfig, genes: Genes, s: &GenStats) -> ModelFile {
    ModelFile {
        name: cfg.run_name.clone(),
        arch: cfg.arch,
        method: cfg.method.name().into(),
        generation: s.generation,
        fitness: s.best_fitness,
        vs_scripted: s.vs_scripted.win,
        rules: cfg.rules,
        genes,
    }
}

/// Picks the faster device for population evaluation. The GPU only pays off when
/// the networks are large relative to the simulation, so with `use_gpu` the first
/// generations alternate GPU and CPU, the faster per simulated tick wins, and the
/// trial repeats every `RETRY` generations.
struct DeviceChoice {
    possible: bool,
    trial: Vec<(bool, f64)>,
    chosen: bool,
    since: u32,
}

impl DeviceChoice {
    const TRIAL: usize = 5;
    const RETRY: u32 = 200;

    fn new(cfg: &TrainConfig) -> DeviceChoice {
        #[cfg(feature = "gpu")]
        let possible = cfg.use_gpu
            && matches!(cfg.method, Method::Ga | Method::Es)
            && cfg.rules == Rules::Tanks
            && crate::gpu::supports(cfg.arch)
            && crate::gpu::get().is_some();
        #[cfg(not(feature = "gpu"))]
        let possible = {
            let _ = cfg;
            false
        };
        DeviceChoice {
            possible,
            trial: Vec::new(),
            chosen: false,
            since: 0,
        }
    }

    fn pick(&mut self) -> bool {
        if !self.possible {
            return false;
        }
        if self.trial.len() < Self::TRIAL {
            // GPU, CPU, GPU, CPU, GPU: the first GPU sample also pays the
            // one-off start-up and is left out of the comparison.
            return self.trial.len().is_multiple_of(2);
        }
        self.chosen
    }

    /// Returns a log line when a trial concludes.
    fn record(&mut self, gpu: bool, seconds: f64, ticks: u64) -> Option<String> {
        if !self.possible {
            return None;
        }
        if self.trial.len() < Self::TRIAL {
            self.trial.push((gpu, seconds / ticks.max(1) as f64));
            if self.trial.len() < Self::TRIAL {
                return None;
            }
            // Skip the first GPU sample (start-up) when comparing.
            let mean = |g: bool| {
                let v: Vec<f64> = self.trial[1..]
                    .iter()
                    .filter(|t| t.0 == g)
                    .map(|t| t.1)
                    .collect();
                v.iter().sum::<f64>() / v.len().max(1) as f64
            };
            let (tg, tc) = (mean(true), mean(false));
            self.chosen = tg < tc;
            self.since = 0;
            return Some(format!(
                "evaluation on {} (GPU {:.0} ns/tick, CPU {:.0} ns/tick)",
                if self.chosen { "GPU" } else { "CPU" },
                tg * 1e9,
                tc * 1e9
            ));
        }
        self.since += 1;
        if self.since >= Self::RETRY {
            self.trial.clear();
        }
        None
    }
}

fn run(cfg: TrainConfig, resume: Option<RunState>, cmd: Receiver<Cmd>, tx: &Sender<Msg>) {
    if !cfg.method.supports_rules(cfg.rules) {
        let _ = tx.send(Msg::Log(format!(
            "{} cannot train under {} rules (GA and ES can)",
            cfg.method.name(),
            cfg.rules.name()
        )));
        return;
    }
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
            let from_model = (!cfg.init_model.is_empty())
                .then(|| ModelFile::load(cfg.init_model.as_ref()).ok())
                .flatten()
                .and_then(|m| match m.genes {
                    Genes::Fixed(p) if m.arch == cfg.arch => Some(p),
                    // A smaller architecture embedded in a bigger one.
                    Genes::Fixed(p) => crate::nn::fixed::transplant(m.arch, &p, cfg.arch, &mut rng),
                    _ => None,
                });
            if !cfg.init_model.is_empty() {
                let _ = tx.send(Msg::Log(match from_model {
                    Some(_) => format!("Starting from {}", cfg.init_model),
                    None => format!(
                        "Cannot start from {}: not a {}",
                        cfg.init_model,
                        cfg.arch.name()
                    ),
                }));
            }
            let init = from_model.or_else(|| {
                (cfg.imitation.enabled && cfg.arch != Arch::Neat && cfg.rules == Rules::Tanks).then(
                    || {
                        let _ = tx.send(Msg::Log("Imitation warm start...".into()));
                        imitation::pretrain_cached(cfg.arch, &cfg.imitation, &mut rng, &|l| {
                            let _ = tx.send(Msg::Log(l));
                        })
                    },
                )
            });
            let trainer = Trainer::new(&cfg, &mut rng, init);
            let level = cfg.start_level.min(cfg.top_level.min(MAX_LEVEL));
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
                level,
                history: Vec::new(),
                trainer,
                ema: None,
                bot_scores: Vec::new(),
            }
        }
    };
    let _ = tx.send(Msg::History(st.history.clone()));
    let dir = st.cfg.run_dir();
    let mut device = DeviceChoice::new(&st.cfg);
    let league = load_league(&st.cfg, tx);
    if !league.is_empty() {
        let _ = tx.send(Msg::Log(format!("League: {} models", league.len())));
    }
    let (mut robots, mut robot_group) = (None, None);
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
            || (cfg.max_seconds > 0.0 && st.seconds >= cfg.max_seconds)
        {
            let _ = st.save();
            let _ = tx.send(Msg::Log("Budget reached".into()));
            return;
        }

        let t0 = Instant::now();
        let shaping = cfg.shaping(st.generation);
        let opps = opponents(
            &cfg,
            &st.hof,
            &league,
            st.level,
            &st.bot_scores,
            &mut st.rng,
        );
        let terrains = eval::terrains(&mut st.rng, cfg.matches, cfg.density);
        let group = robot_group_at(&cfg, st.generation);
        if robot_group != Some(group) {
            // Stop the old group before starting the next.
            drop(robots.take());
            robots = start_robots(&cfg, &robot_group_dirs(&cfg, group), tx);
            robot_group = Some(group);
        }
        let use_gpu = device.pick();
        let t_step = Instant::now();
        // Every working robot in turn, from a random one: pairs next to each
        // other meet different robots, so their battles run side by side.
        let robot_plan = match &robots {
            Some(p) => {
                let alive: Vec<usize> = (0..p.bots().len()).filter(|&b| p.alive(b)).collect();
                let from = st.rng.below(alive.len().max(1));
                let n = if alive.is_empty() {
                    0
                } else {
                    cfg.population.div_ceil(2) * cfg.robot_battles
                };
                (0..n).map(|k| alive[(from + k) % alive.len()]).collect()
            }
            None => Vec::new(),
        };
        let rep = st.trainer.step(StepEnv {
            cfg: &cfg,
            use_gpu,
            generation: st.generation,
            opponents: opps,
            terrains,
            shaping,
            rng: &mut st.rng,
            robots: robots.as_ref(),
            robot_plan,
        });
        if let Some(l) = device.record(use_gpu, t_step.elapsed().as_secs_f64(), rep.ticks) {
            let _ = tx.send(Msg::Log(format!("Gen {}: {l}", st.generation)));
        }

        let champ = rep.champion.clone();
        let arch = cfg.arch;
        let make =
            || -> Box<dyn crate::sim::world::Controller> { Box::new(Brain::new(arch, &champ)) };
        let mut eval_ticks = 0;
        // Records against every bot this generation scores on, with their
        // `bot_scores` index.
        let (record, vs_ace, tops, all) = match cfg.rules {
            Rules::Tanks => {
                let (record, t) = eval::vs_scripted(&make, cfg.eval_matches, VETERAN);
                let (vs_ace, t2) = eval::vs_scripted(&make, cfg.eval_matches, ACE);
                eval_ticks += t + t2;
                // Every bot above the ace that the curriculum has reached.
                let tops: Vec<Record> = (ACE + 1..=st.level)
                    .map(|l| {
                        let (r, t) = eval::vs_scripted(&make, cfg.eval_matches, l);
                        eval_ticks += t;
                        r
                    })
                    .collect();
                let all: Vec<(usize, Record)> =
                    [(VETERAN as usize, record), (ACE as usize, vs_ace)]
                        .into_iter()
                        .chain((ACE as usize + 1..).zip(tops.iter().copied()))
                        .collect();
                (record, vs_ace, tops, all)
            }
            Rules::Robocode => {
                let Genes::Fixed(p) = &champ else {
                    unreachable!("Robocode rules train fixed-topology nets")
                };
                let make = || eval::net_pilot(arch, p);
                let all: Vec<(usize, Record)> = Sparring::ALL
                    .iter()
                    .enumerate()
                    .map(|(k, s)| {
                        let (r, t) = eval::vs_sparring(&make, cfg.eval_matches, *s);
                        eval_ticks += t;
                        (k, r)
                    })
                    .collect();
                let recs: Vec<Record> = all.iter().map(|(_, r)| *r).collect();
                (Record::mean(&recs), Record::default(), Vec::new(), all)
            }
        };
        // Smoothed score against every bot evaluated this generation.
        st.bot_scores.resize(MAX_LEVEL as usize + 1, 0.5);
        for (i, r) in &all {
            let s = &mut st.bot_scores[*i];
            *s = 0.8 * *s + 0.2 * (r.win + 0.5 * r.draw);
        }
        if cfg.bot_focus > 0.0 && cfg.rules == Rules::Robocode && st.generation.is_multiple_of(50) {
            let scores: Vec<String> = Sparring::ALL
                .iter()
                .enumerate()
                .map(|(k, s)| format!("{} {:.2}", s.name(), st.bot_scores[k]))
                .collect();
            let _ = tx.send(Msg::Log(format!(
                "Gen {}: bot scores {}",
                st.generation,
                scores.join(", ")
            )));
        } else if cfg.bot_focus > 0.0 && st.generation.is_multiple_of(50) {
            let scores: Vec<String> = (cfg.mix_from.min(st.level)..=st.level)
                .map(|l| format!("{} {:.2}", LEVELS[l as usize], st.bot_scores[l as usize]))
                .collect();
            let _ = tx.send(Msg::Log(format!(
                "Gen {}: bot scores {}",
                st.generation,
                scores.join(", ")
            )));
        }
        let level = st.level;
        let mut promoted = None;
        if cfg.rules == Rules::Tanks && st.level < cfg.top_level.min(MAX_LEVEL) {
            let r = if st.level == VETERAN {
                record
            } else if st.level == ACE {
                vs_ace
            } else if st.level > ACE {
                tops[(st.level - ACE - 1) as usize]
            } else {
                let (r, t) = eval::vs_scripted(&make, cfg.eval_matches, st.level);
                eval_ticks += t;
                r
            };
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

        let vs_top = Record::mean(&tops);
        let stats = GenStats {
            generation: st.generation,
            steps: st.steps,
            seconds: st.seconds,
            best_fitness: rep.best_fitness,
            mean_fitness: rep.mean_fitness,
            shaping: shaping.weight,
            level,
            vs_scripted: record,
            vs_ace,
            vs_top,
            extra: rep.extra,
        };
        let m = model(&cfg, rep.champion, &stats);
        // Champion selection on every bot from the veteran up (or every
        // sample bot), so it cannot specialise on one style.
        let score = all.iter().map(|(_, r)| r.win - r.loss).sum::<f32>() / all.len() as f32;
        let mut log = None;
        if score > st.best_score {
            st.best_score = score;
            if m.save(&dir.join("best.json")).is_ok() {
                let top = if level > ACE {
                    format!(
                        ", stronger bots W {:.0}% L {:.0}%",
                        vs_top.win * 100.0,
                        vs_top.loss * 100.0
                    )
                } else {
                    String::new()
                };
                log = Some(match cfg.rules {
                    Rules::Tanks => format!(
                        "Gen {}: new best (veteran W {:.0}% L {:.0}%, ace W {:.0}% L {:.0}%{top}) saved",
                        st.generation,
                        record.win * 100.0,
                        record.loss * 100.0,
                        vs_ace.win * 100.0,
                        vs_ace.loss * 100.0
                    ),
                    Rules::Robocode => format!(
                        "Gen {}: new best (sample bots W {:.0}% L {:.0}%) saved",
                        st.generation,
                        record.win * 100.0,
                        record.loss * 100.0
                    ),
                });
            }
        }
        let _ = m.save(&dir.join("latest.json"));
        if cfg.method == Method::Es
            && cfg.ema_decay > 0.0
            && let Genes::Fixed(p) = &m.genes
        {
            let ema = st.ema.get_or_insert_with(|| p.clone());
            for (e, v) in ema.iter_mut().zip(p) {
                *e += (1.0 - cfg.ema_decay) * (v - *e);
            }
            if st.generation.is_multiple_of(10) {
                let mut e = m.clone();
                e.genes = Genes::Fixed(ema.clone());
                let _ = e.save(&dir.join("ema.json"));
            }
        }
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
