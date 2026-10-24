//! Warm start by imitation: behaviour cloning on scripted-bot demonstrations,
//! followed by DAgger rounds (the network drives, the scripted bot labels).

use crate::bots::ScriptedBot;
use crate::bots::scripted::{ACE, LEVELS, MAX_LEVEL};
use crate::nn::Arch;
use crate::nn::fixed::{FixedNet, Layout, N_IN, N_OUT, init_params};
use crate::nn::grad::{last_hidden, seq_backward, seq_forward};
use crate::rng::Rng;
use crate::sim::world::{Action, Controller, World};
use crate::sim::{ACTION_REPEAT, Terrain};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ImitationParams {
    pub enabled: bool,
    /// Scripted level that demonstrates (and labels the DAgger rounds).
    pub teacher: u8,
    /// Demonstration matches per round.
    pub matches: usize,
    pub epochs: usize,
    pub dagger_rounds: usize,
    pub lr: f32,
}

impl Default for ImitationParams {
    fn default() -> Self {
        ImitationParams {
            enabled: true,
            teacher: ACE,
            matches: 300,
            epochs: 20,
            dagger_rounds: 3,
            lr: 2e-3,
        }
    }
}

struct Seq {
    obs: Vec<[f32; N_IN]>,
    target: Vec<[f32; N_OUT]>,
}

fn target(a: &Action) -> [f32; N_OUT] {
    let k = 0.95;
    [
        a.throttle * k,
        a.hull * k,
        a.turret * k,
        a.radar * k,
        if a.fire { k } else { -k },
    ]
}

/// Record one match: tank 0 is driven by the learner (if given) or the expert;
/// the expert's action is always the label. Opponents range from the rookie
/// to the ace, so the demonstrations include shells to dodge.
fn record(arch: Arch, teacher: u8, learner: Option<&[f32]>, seed: u64, density: f32) -> Seq {
    let mut rng = Rng::new(seed);
    let terrain = Terrain::generate(seed, density);
    let mut w = World::new(terrain, seed % 2 == 1);
    let mut expert = ScriptedBot::with_level(seed ^ 0x55, teacher);
    let mut opp = ScriptedBot::with_level(seed ^ 0xAA, 2 + rng.below(3) as u8);
    let mut net = learner.map(|p| FixedNet::new(arch, p.to_vec()));
    let mut seq = Seq {
        obs: Vec::new(),
        target: Vec::new(),
    };
    let mut acts = [Action::default(); 2];
    while !w.done() {
        if w.tick.is_multiple_of(ACTION_REPEAT) {
            let obs = w.observe(0);
            let label = expert.act(&obs);
            acts[0] = match &mut net {
                Some(n) => Action::from_outputs(&n.forward(&obs)),
                None => label,
            };
            acts[1] = opp.act(&w.observe(1));
            seq.obs.push(obs);
            seq.target.push(target(&label));
        }
        w.step(acts);
    }
    seq
}

/// Gradient of the weighted squared error over one sequence (hidden state carried across chunks).
fn seq_grad(layout: &Layout, p: &[f32], s: &Seq, chunk: usize, g: &mut [f32]) -> f32 {
    let mut h = vec![0.0; layout.hidden()];
    let mut loss = 0.0;
    for start in (0..s.obs.len()).step_by(chunk) {
        let end = (start + chunk).min(s.obs.len());
        let xs = &s.obs[start..end];
        let (outs, cache) = seq_forward(layout, p, &h, xs);
        let mut d = vec![[0.0f32; N_OUT]; xs.len()];
        for (j, t) in (start..end).enumerate() {
            for k in 0..N_OUT {
                // Firing is rare: weight positive fire labels more.
                let w = if k == 4 && s.target[t][4] > 0.0 {
                    10.0
                } else {
                    1.0
                };
                let e = outs[j][k] - s.target[t][k];
                loss += w * e * e;
                d[j][k] = 2.0 * w * e;
            }
        }
        seq_backward(layout, p, xs, &outs, &cache, &d, g);
        h = last_hidden(&cache);
    }
    loss
}

fn train(arch: Arch, p: &mut [f32], data: &[Seq], params: &ImitationParams, rng: &mut Rng) -> f32 {
    let (layout, n) = Layout::new(arch);
    let mut m = vec![0.0f32; n];
    let mut v = vec![0.0f32; n];
    let mut t = 0;
    let samples: usize = data.iter().map(|s| s.obs.len()).sum();
    let mut last_loss = 0.0;
    for _ in 0..params.epochs {
        let mut order: Vec<usize> = (0..data.len()).collect();
        rng.shuffle(&mut order);
        let mut epoch_loss = 0.0;
        for batch in order.chunks(16) {
            let pr = &*p;
            let (g, loss, count) = batch
                .par_iter()
                .map(|&i| {
                    let mut g = vec![0.0f32; n];
                    let l = seq_grad(&layout, pr, &data[i], 32, &mut g);
                    (g, l, data[i].obs.len())
                })
                .reduce(
                    || (vec![0.0; n], 0.0, 0),
                    |mut a, b| {
                        a.0.iter_mut().zip(&b.0).for_each(|(x, y)| *x += y);
                        (a.0, a.1 + b.1, a.2 + b.2)
                    },
                );
            epoch_loss += loss;
            let k = count.max(1) as f32;
            let norm = (g.iter().map(|x| (x / k) * (x / k)).sum::<f32>()).sqrt();
            let clip = if norm > 1.0 { 1.0 / norm } else { 1.0 };
            t += 1;
            let (b1, b2) = (0.9f32, 0.999f32);
            let c1 = 1.0 - b1.powi(t);
            let c2 = 1.0 - b2.powi(t);
            for i in 0..n {
                let gi = g[i] / k * clip;
                m[i] = b1 * m[i] + (1.0 - b1) * gi;
                v[i] = b2 * v[i] + (1.0 - b2) * gi * gi;
                p[i] -= params.lr * (m[i] / c1) / ((v[i] / c2).sqrt() + 1e-8);
            }
        }
        last_loss = epoch_loss / samples.max(1) as f32;
    }
    last_loss
}

/// Pretrained weights are cached in `models/pretrained/` per architecture and settings.
pub fn pretrain_cached(
    arch: Arch,
    params: &ImitationParams,
    rng: &mut Rng,
    log: &dyn Fn(String),
) -> Vec<f32> {
    let name = format!(
        "{}_{}_m{}_e{}_d{}_lr{}.json",
        arch.short(),
        LEVELS[params.teacher.min(MAX_LEVEL) as usize].replace(' ', "_"),
        params.matches,
        params.epochs,
        params.dagger_rounds,
        params.lr
    );
    let path = crate::nn::models_dir().join("pretrained").join(name);
    if let Ok(m) = crate::nn::ModelFile::load(&path)
        && let crate::nn::Genes::Fixed(p) = m.genes
        && p.len() == arch.param_count()
    {
        log(format!("Using cached imitation weights {}", path.display()));
        return p;
    }
    let p = pretrain(arch, params, rng, log);
    let m = crate::nn::ModelFile {
        name: format!(
            "{} imitating the {}",
            arch.name(),
            LEVELS[params.teacher.min(MAX_LEVEL) as usize]
        ),
        arch,
        method: "Imitation".into(),
        generation: 0,
        fitness: 0.0,
        vs_scripted: 0.0,
        genes: crate::nn::Genes::Fixed(p.clone()),
    };
    let _ = m.save(&path);
    p
}

/// Behaviour cloning + DAgger. Returns the pretrained parameters.
pub fn pretrain(
    arch: Arch,
    params: &ImitationParams,
    rng: &mut Rng,
    log: &dyn Fn(String),
) -> Vec<f32> {
    let mut p = init_params(arch, rng);
    let mut data: Vec<Seq> = Vec::new();
    for round in 0..=params.dagger_rounds {
        let learner = (round > 0).then(|| p.clone());
        let seeds: Vec<(u64, f32)> = (0..params.matches)
            .map(|_| (rng.next_u64() % 1_000_000_000, rng.f32()))
            .collect();
        let new: Vec<Seq> = seeds
            .into_par_iter()
            .map(|(s, d)| record(arch, params.teacher, learner.as_deref(), s, d))
            .collect();
        data.extend(new);
        let loss = train(arch, &mut p, &data, params, rng);
        log(format!(
            "Imitation round {}/{}: {} demonstrations, loss {:.4}",
            round,
            params.dagger_rounds,
            data.len(),
            loss
        ));
    }
    p
}
