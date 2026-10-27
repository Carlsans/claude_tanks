//! OpenAI-style evolution strategy: antithetic Gaussian perturbations of one mean
//! parameter vector, centered-rank fitness shaping, Adam update.

use super::eval::{eval_members, robot_extra};
use super::{StepEnv, StepReport, TrainConfig};
use crate::nn::{Arch, Genes, fixed};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct EsParams {
    pub sigma: f32,
    pub lr: f32,
    pub weight_decay: f32,
    /// Mixture of experts only: train just the gate, keeping both experts as they are.
    #[serde(default)]
    pub gate_only: bool,
}

impl Default for EsParams {
    fn default() -> Self {
        EsParams {
            sigma: 0.01,
            lr: 0.003,
            weight_decay: 0.002,
            gate_only: false,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Es {
    theta: Vec<f32>,
    m: Vec<f32>,
    v: Vec<f32>,
    t: u32,
}

impl Es {
    pub fn new(cfg: &TrainConfig, rng: &mut Rng, init: Option<Vec<f32>>) -> Es {
        let theta = init.unwrap_or_else(|| fixed::init_params(cfg.arch, rng));
        let n = theta.len();
        Es {
            theta,
            m: vec![0.0; n],
            v: vec![0.0; n],
            t: 0,
        }
    }

    pub fn step(&mut self, env: StepEnv) -> StepReport {
        let p = env.cfg.es;
        let pairs = (env.cfg.population / 2).max(1);
        let n = self.theta.len();
        let rng = &mut *env.rng;
        // The parameters being trained: all of them, or the gate of a mixture of experts.
        let live = if p.gate_only {
            match fixed::Layout::new(Arch::Moe).0 {
                fixed::Layout::Moe { gw, gb, .. } if n == Arch::Moe.param_count() => {
                    gw..gb + fixed::EXPERTS
                }
                _ => panic!("es.gate_only needs a mixture of experts"),
            }
        } else {
            0..n
        };
        let noise: Vec<Vec<f32>> = (0..pairs)
            .map(|_| {
                (0..n)
                    .map(|i| if live.contains(&i) { rng.gauss() } else { 0.0 })
                    .collect()
            })
            .collect();
        let theta = &self.theta;
        let member = |i: usize| -> Vec<f32> {
            let (e, sign) = (&noise[i / 2], if i.is_multiple_of(2) { 1.0 } else { -1.0 });
            theta
                .iter()
                .zip(e)
                .map(|(t, e)| t + sign * p.sigma * e)
                .collect()
        };
        let members: Vec<Vec<f32>> = (0..pairs * 2).map(member).collect();
        let out = eval_members(&env, &members);
        let f = &out.fitness;
        // Centered ranks in [-0.5, 0.5].
        let mut order: Vec<usize> = (0..f.len()).collect();
        order.sort_by(|&a, &b| f[a].total_cmp(&f[b]));
        let mut rank = vec![0.0f32; f.len()];
        for (r, &i) in order.iter().enumerate() {
            rank[i] = r as f32 / (f.len() - 1).max(1) as f32 - 0.5;
        }
        let mut g = vec![0.0f32; n];
        for (k, e) in noise.iter().enumerate() {
            let w = rank[2 * k] - rank[2 * k + 1];
            for (gi, ei) in g.iter_mut().zip(e) {
                *gi += w * ei;
            }
        }
        let scale = 1.0 / (pairs as f32 * 2.0 * p.sigma);
        // Gradient ascent on fitness == descent on -g; Adam.
        self.t += 1;
        let (b1, b2) = (0.9f32, 0.999f32);
        let c1 = 1.0 - b1.powi(self.t as i32);
        let c2 = 1.0 - b2.powi(self.t as i32);
        for i in live {
            let gi = -g[i] * scale + p.weight_decay * self.theta[i];
            self.m[i] = b1 * self.m[i] + (1.0 - b1) * gi;
            self.v[i] = b2 * self.v[i] + (1.0 - b2) * gi * gi;
            self.theta[i] -= p.lr * (self.m[i] / c1) / ((self.v[i] / c2).sqrt() + 1e-8);
        }
        let best = f.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean = f.iter().sum::<f32>() / f.len() as f32;
        let norm = (self.theta.iter().map(|v| v * v).sum::<f32>() / n as f32).sqrt();
        StepReport {
            best_fitness: best,
            mean_fitness: mean,
            champion: Genes::Fixed(self.theta.clone()),
            ticks: out.ticks,
            extra: std::iter::once(("param rms".into(), norm))
                .chain(robot_extra(out.robots))
                .collect(),
        }
    }
}
