//! Genetic algorithm over flat parameter vectors (fixed-topology networks).

use super::eval::eval_population;
use super::{StepEnv, StepReport, TrainConfig};
use crate::nn::{Brain, Genes, fixed};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GaParams {
    /// Individuals copied unchanged into the next generation.
    pub elite: usize,
    pub tournament: usize,
    /// Std-dev of the Gaussian noise added to every parameter.
    pub sigma: f32,
    /// Mutation std-dev when starting from imitation weights: the noise accumulates
    /// over generations, so it must stay small to keep the pretrained skill.
    pub pretrained_sigma: f32,
    /// Probability that a child comes from uniform crossover of two parents.
    pub crossover: f32,
}

impl Default for GaParams {
    fn default() -> Self {
        GaParams {
            elite: 4,
            tournament: 4,
            sigma: 0.04,
            pretrained_sigma: 0.005,
            crossover: 0.3,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ga {
    pop: Vec<Vec<f32>>,
    #[serde(default)]
    pretrained: bool,
}

impl Ga {
    /// Random population, or mutated copies of `init` (the first copy unchanged).
    pub fn new(cfg: &TrainConfig, rng: &mut Rng, init: Option<Vec<f32>>) -> Ga {
        let sigma = cfg.ga.pretrained_sigma;
        let pop = (0..cfg.population.max(2))
            .map(|i| match &init {
                Some(p) if i == 0 => p.clone(),
                Some(p) => p.iter().map(|v| v + rng.gauss() * sigma).collect(),
                None => fixed::init_params(cfg.arch, rng),
            })
            .collect();
        Ga {
            pop,
            pretrained: init.is_some(),
        }
    }

    pub fn step(&mut self, env: StepEnv) -> StepReport {
        let p = env.cfg.ga;
        let sigma = if self.pretrained {
            p.pretrained_sigma
        } else {
            p.sigma
        };
        let arch = env.cfg.arch;
        let pop = &self.pop;
        let out = eval_population(
            &|i| Box::new(Brain::new(arch, &Genes::Fixed(pop[i].clone()))),
            pop.len(),
            &env.opponents,
            &env.terrains,
            env.shaping,
        );
        let f = &out.fitness;
        let mut order: Vec<usize> = (0..pop.len()).collect();
        order.sort_by(|&a, &b| f[b].total_cmp(&f[a]));
        let champion = pop[order[0]].clone();
        let mean = f.iter().sum::<f32>() / f.len() as f32;

        let rng = env.rng;
        let mut next: Vec<Vec<f32>> = order
            .iter()
            .take(p.elite.min(pop.len()))
            .map(|&i| pop[i].clone())
            .collect();
        let pick = |rng: &mut Rng| {
            (0..p.tournament.max(1))
                .map(|_| rng.below(pop.len()))
                .max_by(|&a, &b| f[a].total_cmp(&f[b]))
                .unwrap()
        };
        while next.len() < pop.len() {
            let a = pick(rng);
            let mut child = pop[a].clone();
            if rng.chance(p.crossover) {
                let b = &pop[pick(rng)];
                for (c, bv) in child.iter_mut().zip(b) {
                    if rng.chance(0.5) {
                        *c = *bv;
                    }
                }
            }
            for c in &mut child {
                *c += rng.gauss() * sigma;
            }
            next.push(child);
        }
        self.pop = next;
        StepReport {
            best_fitness: f[order[0]],
            mean_fitness: mean,
            champion: Genes::Fixed(champion),
            ticks: out.ticks,
            extra: vec![],
        }
    }
}
