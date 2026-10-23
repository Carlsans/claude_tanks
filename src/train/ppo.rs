//! Proximal Policy Optimization with self-play, on CPU, with hand-written gradients.
//!
//! The policy is a `FixedNet` whose tanh outputs are the means of a diagonal Gaussian
//! (learned log-std); the env clamps actions and fires when the sampled fire value > 0.
//! At test time the deterministic mean is used, so trained policies load like any model.
//! The value function is a separate 64x64 MLP on the observation.

use super::eval::{Opponent, fitness};
use super::{StepEnv, StepReport, TrainConfig};
use crate::nn::Genes;
use crate::nn::fixed::{FixedNet, Layout, N_IN, N_OUT, init_params};
use crate::nn::grad::{dense_backward, dense_forward, seq_backward, seq_forward};
use crate::rng::Rng;
use crate::sim::world::{Action, World};
use crate::sim::{ACTION_REPEAT, Terrain, wrap_angle};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct PpoParams {
    pub lr: f32,
    pub vf_lr: f32,
    pub epochs: usize,
    /// Samples per gradient step (rounded to whole chunks).
    pub minibatch: usize,
    pub clip: f32,
    pub gamma: f32,
    pub lambda: f32,
    pub ent_coef: f32,
    /// Truncated BPTT length (and minibatch granularity).
    pub chunk: usize,
    pub init_log_std: f32,
    pub max_grad_norm: f32,
}

impl Default for PpoParams {
    fn default() -> Self {
        PpoParams {
            lr: 3e-4,
            vf_lr: 1e-3,
            epochs: 4,
            minibatch: 2048,
            clip: 0.2,
            gamma: 0.995,
            lambda: 0.98,
            ent_coef: 0.001,
            chunk: 32,
            init_log_std: -0.5,
            max_grad_norm: 0.5,
        }
    }
}

const VALUE_DIMS: [usize; 4] = [N_IN, 64, 64, 1];

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Adam {
    m: Vec<f32>,
    v: Vec<f32>,
    t: u32,
}

impl Adam {
    fn new(n: usize) -> Adam {
        Adam {
            m: vec![0.0; n],
            v: vec![0.0; n],
            t: 0,
        }
    }
    fn step(&mut self, p: &mut [f32], g: &[f32], lr: f32) {
        let (b1, b2, eps) = (0.9f32, 0.999f32, 1e-8f32);
        self.t += 1;
        let c1 = 1.0 - b1.powi(self.t as i32);
        let c2 = 1.0 - b2.powi(self.t as i32);
        for i in 0..p.len() {
            self.m[i] = b1 * self.m[i] + (1.0 - b1) * g[i];
            self.v[i] = b2 * self.v[i] + (1.0 - b2) * g[i] * g[i];
            p[i] -= lr * (self.m[i] / c1) / ((self.v[i] / c2).sqrt() + eps);
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ppo {
    /// Policy network parameters followed by N_OUT log-stds.
    policy: Vec<f32>,
    value: Vec<f32>,
    adam_p: Adam,
    adam_v: Adam,
}

struct Episode {
    obs: Vec<[f32; N_IN]>,
    act: Vec<[f32; N_OUT]>,
    logp: Vec<f32>,
    val: Vec<f32>,
    rew: Vec<f32>,
    /// Hidden state at the start of each chunk.
    h0: Vec<Vec<f32>>,
    adv: Vec<f32>,
    ret: Vec<f32>,
    fitness: f32,
    ticks: u64,
}

fn log_prob(a: &[f32; N_OUT], mu: &[f32; N_OUT], log_std: &[f32]) -> f32 {
    let mut lp = 0.0;
    for k in 0..N_OUT {
        let s = log_std[k].exp();
        let z = (a[k] - mu[k]) / s;
        lp += -0.5 * z * z - log_std[k] - 0.918_938_5;
    }
    lp
}

fn value_of(vp: &[f32], layers: &[(usize, usize, usize, usize)], x: &[f32]) -> f32 {
    dense_forward(layers, vp, x, true).last().unwrap()[0]
}

impl Ppo {
    pub fn new(cfg: &TrainConfig, rng: &mut Rng) -> Ppo {
        let mut policy = init_params(cfg.arch, rng);
        // Small output layer so initial actions are near zero.
        let (layout, _) = Layout::new(cfg.arch);
        scale_output(&layout, &mut policy, 0.1);
        policy.extend(std::iter::repeat_n(cfg.ppo.init_log_std, N_OUT));
        let (vl, vn) = Layout::mlp(&VALUE_DIMS);
        let mut value = vec![0.0; vn];
        if let Layout::Mlp(layers) = &vl {
            for &(i, o, w, _) in layers {
                let s = 1.0 / (i as f32).sqrt();
                for v in &mut value[w..w + i * o] {
                    *v = rng.gauss() * s;
                }
            }
        }
        let np = policy.len();
        Ppo {
            policy,
            value,
            adam_p: Adam::new(np),
            adam_v: Adam::new(vn),
        }
    }

    fn net_len(&self) -> usize {
        self.policy.len() - N_OUT
    }

    fn rollout(
        &self,
        cfg: &TrainConfig,
        terrain: Terrain,
        swap: bool,
        opp: &Opponent,
        seed: u64,
        shaping: f32,
    ) -> Episode {
        let p = cfg.ppo;
        let n = self.net_len();
        let log_std = &self.policy[n..];
        let mut net = FixedNet::new(cfg.arch, self.policy[..n].to_vec());
        let (vl, _) = Layout::mlp(&VALUE_DIMS);
        let Layout::Mlp(vlayers) = vl else {
            unreachable!()
        };
        let mut rng = Rng::new(seed);
        let mut opp = opp.controller();
        let mut w = World::new(terrain, swap);
        let mut ep = Episode {
            obs: Vec::new(),
            act: Vec::new(),
            logp: Vec::new(),
            val: Vec::new(),
            rew: Vec::new(),
            h0: Vec::new(),
            adv: Vec::new(),
            ret: Vec::new(),
            fitness: 0.0,
            ticks: 0,
        };
        let mut acts = [Action::default(); 2];
        let mut prev = w.tanks[0].stats;
        let mut phi = potential(&w, shaping);
        while !w.done() {
            if w.tick.is_multiple_of(ACTION_REPEAT) {
                if !ep.obs.is_empty() {
                    let s = w.tanks[0].stats;
                    let phi2 = potential(&w, shaping);
                    ep.rew
                        .push(p.gamma * phi2 - phi + near_reward(&prev, &s, shaping));
                    phi = phi2;
                    prev = s;
                }
                let obs = w.observe(0);
                if ep.obs.len().is_multiple_of(p.chunk) {
                    ep.h0.push(net.hidden_state().to_vec());
                }
                let mu = net.forward(&obs);
                let mut a = [0.0; N_OUT];
                for k in 0..N_OUT {
                    a[k] = mu[k] + log_std[k].exp() * rng.gauss();
                }
                ep.logp.push(log_prob(&a, &mu, log_std));
                ep.val.push(value_of(&self.value, &vlayers, &obs));
                ep.obs.push(obs);
                ep.act.push(a);
                acts[0] = Action::from_outputs(&a);
                acts[1] = opp.act(&w.observe(1));
            }
            w.step(acts);
        }
        let r = w.result.unwrap();
        let s = w.tanks[0].stats;
        let outcome = match r.winner {
            Some(0) => 1.0,
            Some(_) => -1.0,
            None => 0.0,
        };
        // Terminal: potential drops to 0.
        ep.rew.push(near_reward(&prev, &s, shaping) - phi + outcome);
        ep.fitness = fitness(&r, 0, super::eval::Shaping { weight: shaping });
        ep.ticks = r.ticks as u64;

        // GAE.
        let t_len = ep.rew.len();
        ep.adv = vec![0.0; t_len];
        let mut last = 0.0;
        for t in (0..t_len).rev() {
            let next_v = if t + 1 < t_len { ep.val[t + 1] } else { 0.0 };
            let delta = ep.rew[t] + p.gamma * next_v - ep.val[t];
            last = delta + p.gamma * p.lambda * last;
            ep.adv[t] = last;
        }
        ep.ret = ep.adv.iter().zip(&ep.val).map(|(a, v)| a + v).collect();
        ep
    }

    pub fn step(&mut self, env: StepEnv) -> StepReport {
        let cfg = env.cfg;
        let p = cfg.ppo;
        let rng = env.rng;
        let n_env = cfg.population.max(1);
        let jobs: Vec<(Terrain, bool, Opponent, u64)> = (0..n_env)
            .map(|e| {
                let seed = rng.next_u64() % 1_000_000_000;
                let t = Terrain::generate(seed, rng.range(cfg.density.0, cfg.density.1));
                (
                    t,
                    e % 2 == 1,
                    env.opponents[e % env.opponents.len()].clone(),
                    rng.next_u64(),
                )
            })
            .collect();
        let shaping = env.shaping.weight;
        let mut eps: Vec<Episode> = jobs
            .into_par_iter()
            .map(|(t, swap, opp, seed)| self.rollout(cfg, t, swap, &opp, seed, shaping))
            .collect();

        let ticks: u64 = eps.iter().map(|e| e.ticks).sum();
        let fits: Vec<f32> = eps.iter().map(|e| e.fitness).collect();
        let best = fits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean = fits.iter().sum::<f32>() / fits.len() as f32;

        // Normalize advantages over the batch.
        let all: Vec<f32> = eps.iter().flat_map(|e| e.adv.iter().copied()).collect();
        let m = all.iter().sum::<f32>() / all.len() as f32;
        let sd =
            (all.iter().map(|a| (a - m) * (a - m)).sum::<f32>() / all.len() as f32).sqrt() + 1e-6;
        for e in &mut eps {
            e.adv.iter_mut().for_each(|a| *a = (*a - m) / sd);
        }

        let chunks: Vec<(usize, usize)> = eps
            .iter()
            .enumerate()
            .flat_map(|(ei, e)| (0..e.obs.len().div_ceil(p.chunk)).map(move |c| (ei, c)))
            .collect();
        let (layout, n_net) = Layout::new(cfg.arch);
        let (vl, n_val) = Layout::mlp(&VALUE_DIMS);
        let Layout::Mlp(vlayers) = vl else {
            unreachable!()
        };
        let (mut pl_sum, mut vl_sum, mut kl_sum, mut clip_sum, mut batches) =
            (0.0, 0.0, 0.0, 0.0, 0);
        for _ in 0..p.epochs {
            let mut order: Vec<usize> = (0..chunks.len()).collect();
            rng.shuffle(&mut order);
            let per = (p.minibatch / p.chunk.max(1)).max(1);
            for mb in order.chunks(per.max(1)) {
                let policy = &self.policy;
                let value = &self.value;
                let res = mb
                    .par_iter()
                    .map(|&ci| {
                        let (ei, c) = chunks[ci];
                        let e = &eps[ei];
                        let s = c * p.chunk;
                        let end = (s + p.chunk).min(e.obs.len());
                        chunk_grads(
                            &layout,
                            n_net,
                            &vlayers,
                            n_val,
                            policy,
                            value,
                            e,
                            c,
                            s..end,
                            p,
                        )
                    })
                    .reduce(
                        || ChunkOut::zero(policy.len(), n_val),
                        |mut a, b| {
                            a.add(&b);
                            a
                        },
                    );
                let k = res.samples.max(1) as f32;
                let mut gp: Vec<f32> = res.gp.iter().map(|g| g / k).collect();
                let mut gv: Vec<f32> = res.gv.iter().map(|g| g / k).collect();
                clip_norm(&mut gp, p.max_grad_norm);
                clip_norm(&mut gv, p.max_grad_norm);
                self.adam_p.step(&mut self.policy, &gp, p.lr);
                self.adam_v.step(&mut self.value, &gv, p.vf_lr);
                for v in &mut self.policy[n_net..] {
                    *v = v.clamp(-3.0, 0.5);
                }
                pl_sum += res.pl / k;
                vl_sum += res.vl / k;
                kl_sum += res.kl / k;
                clip_sum += res.clipped / k;
                batches += 1;
            }
        }
        let b = batches.max(1) as f32;
        let mean_std = self.policy[n_net..].iter().map(|v| v.exp()).sum::<f32>() / N_OUT as f32;
        StepReport {
            best_fitness: best,
            mean_fitness: mean,
            champion: Genes::Fixed(self.policy[..n_net].to_vec()),
            ticks,
            extra: vec![
                ("policy loss".into(), pl_sum / b),
                ("value loss".into(), vl_sum / b),
                ("approx KL x100".into(), kl_sum / b * 100.0),
                ("clip fraction".into(), clip_sum / b),
                ("action std".into(), mean_std),
            ],
        }
    }
}

/// Shaping potential: how well tank 0 aims at and tracks the enemy.
/// Potential-based shaping (gamma*phi' - phi) leaves the optimal policy unchanged.
fn potential(w: &World, weight: f32) -> f32 {
    let (me, en) = (&w.tanks[0], &w.tanks[1]);
    if !me.alive || !en.alive {
        return 0.0;
    }
    let bearing = (en.pos - me.pos).angle();
    let aim = wrap_angle(bearing - me.turret_abs()).cos().max(0.0).powi(4);
    let radar = wrap_angle(bearing - me.radar_abs()).cos().max(0.0).powi(8);
    weight * (0.3 * aim + 0.2 * radar)
}

/// Reward for shells passing close to the enemy (bounded: ammo is limited).
fn near_reward(a: &crate::sim::world::TankStats, b: &crate::sim::world::TankStats, w: f32) -> f32 {
    w * 0.05 * (b.near - a.near)
}

fn scale_output(layout: &Layout, p: &mut [f32], k: f32) {
    let (w, n) = match layout {
        Layout::Mlp(l) => {
            let &(i, o, w, _) = l.last().unwrap();
            (w, i * o)
        }
        Layout::Elman { h, wo, .. } | Layout::Gru { h, wo, .. } => (*wo, N_OUT * h),
    };
    p[w..w + n].iter_mut().for_each(|v| *v *= k);
}

fn clip_norm(g: &mut [f32], max: f32) {
    let n = g.iter().map(|v| v * v).sum::<f32>().sqrt();
    if n > max {
        let k = max / n;
        g.iter_mut().for_each(|v| *v *= k);
    }
}

struct ChunkOut {
    gp: Vec<f32>,
    gv: Vec<f32>,
    pl: f32,
    vl: f32,
    kl: f32,
    clipped: f32,
    samples: usize,
}

impl ChunkOut {
    fn zero(np: usize, nv: usize) -> ChunkOut {
        ChunkOut {
            gp: vec![0.0; np],
            gv: vec![0.0; nv],
            pl: 0.0,
            vl: 0.0,
            kl: 0.0,
            clipped: 0.0,
            samples: 0,
        }
    }
    fn add(&mut self, o: &ChunkOut) {
        self.gp.iter_mut().zip(&o.gp).for_each(|(a, b)| *a += b);
        self.gv.iter_mut().zip(&o.gv).for_each(|(a, b)| *a += b);
        self.pl += o.pl;
        self.vl += o.vl;
        self.kl += o.kl;
        self.clipped += o.clipped;
        self.samples += o.samples;
    }
}

/// Clipped-surrogate, entropy and value gradients for one chunk (summed, not averaged).
#[allow(clippy::too_many_arguments)]
fn chunk_grads(
    layout: &Layout,
    n_net: usize,
    vlayers: &[(usize, usize, usize, usize)],
    n_val: usize,
    policy: &[f32],
    value: &[f32],
    e: &Episode,
    c: usize,
    range: std::ops::Range<usize>,
    p: PpoParams,
) -> ChunkOut {
    let mut out = ChunkOut::zero(policy.len(), n_val);
    let xs = &e.obs[range.clone()];
    let (mus, cache) = seq_forward(layout, &policy[..n_net], &e.h0[c], xs);
    let log_std = &policy[n_net..];
    let mut d_out = vec![[0.0f32; N_OUT]; xs.len()];
    for (j, t) in range.clone().enumerate() {
        let a = &e.act[t];
        let lp = log_prob(a, &mus[j], log_std);
        let ratio = (lp - e.logp[t]).exp();
        let adv = e.adv[t];
        let unclipped = ratio * adv;
        let clipped = ratio.clamp(1.0 - p.clip, 1.0 + p.clip) * adv;
        out.pl += -unclipped.min(clipped);
        out.kl += e.logp[t] - lp;
        let active = unclipped <= clipped;
        if !active {
            out.clipped += 1.0;
        }
        // d(-min(.))/d(logp) = -adv * ratio when the unclipped branch is active.
        let dlogp = if active { -adv * ratio } else { 0.0 };
        for k in 0..N_OUT {
            let var = (2.0 * log_std[k]).exp();
            let diff = a[k] - mus[j][k];
            d_out[j][k] = dlogp * diff / var;
            // log-std: policy term minus entropy bonus (dH/dlogstd = 1).
            out.gp[n_net + k] += dlogp * (diff * diff / var - 1.0) - p.ent_coef;
        }
        // Value.
        let acts = dense_forward(vlayers, value, &xs[j], true);
        let v = acts.last().unwrap()[0];
        let err = v - e.ret[t];
        out.vl += 0.5 * err * err;
        dense_backward(vlayers, value, &acts, &[err], true, &mut out.gv);
    }
    seq_backward(
        layout,
        &policy[..n_net],
        xs,
        &mus,
        &cache,
        &d_out,
        &mut out.gp[..n_net],
    );
    out.samples = xs.len();
    out
}
