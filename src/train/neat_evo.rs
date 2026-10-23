//! NEAT: speciation, fitness sharing, structural mutations.

use super::eval::eval_population;
use super::{StepEnv, StepReport, TrainConfig};
use crate::nn::neat::{Genome, Innovations, NeatNet};
use crate::nn::{Brain, Genes};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct NeatParams {
    pub c1: f32,
    pub c3: f32,
    pub target_species: usize,
    pub stagnation: u32,
    /// Fraction of each species allowed to reproduce.
    pub survival: f32,
    pub crossover: f32,
    pub weight_power: f32,
    pub weight_replace: f32,
    pub add_conn: f32,
    pub add_node: f32,
}

impl Default for NeatParams {
    fn default() -> Self {
        NeatParams {
            c1: 1.0,
            c3: 0.4,
            target_species: 8,
            stagnation: 20,
            survival: 0.3,
            crossover: 0.6,
            weight_power: 0.3,
            weight_replace: 0.03,
            add_conn: 0.2,
            add_node: 0.06,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Species {
    rep: Genome,
    members: Vec<usize>,
    best: f32,
    stagnant: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Neat {
    pop: Vec<Genome>,
    inn: Innovations,
    species: Vec<Species>,
    threshold: f32,
}

impl Neat {
    pub fn new(cfg: &TrainConfig, rng: &mut Rng) -> Neat {
        let mut inn = Innovations::new();
        let pop = (0..cfg.population.max(4))
            .map(|_| Genome::initial(rng, &mut inn))
            .collect();
        Neat {
            pop,
            inn,
            species: Vec::new(),
            threshold: 1.5,
        }
    }

    pub fn step(&mut self, env: StepEnv) -> StepReport {
        let p = env.cfg.neat;
        let rng = env.rng;
        let pop = &self.pop;
        let out = eval_population(
            &|i| Box::new(Brain::Neat(NeatNet::new(&pop[i]))),
            pop.len(),
            &env.opponents,
            &env.terrains,
            env.shaping,
        );
        let f = out.fitness;
        let n = pop.len();
        let best_i = (0..n).max_by(|&a, &b| f[a].total_cmp(&f[b])).unwrap();
        let champion = pop[best_i].clone();
        let mean = f.iter().sum::<f32>() / n as f32;

        // Speciate.
        for s in &mut self.species {
            s.members.clear();
        }
        for (i, g) in pop.iter().enumerate() {
            let found = self
                .species
                .iter_mut()
                .find(|s| g.distance(&s.rep, p.c1, p.c3) < self.threshold);
            match found {
                Some(s) => s.members.push(i),
                None => self.species.push(Species {
                    rep: g.clone(),
                    members: vec![i],
                    best: f32::NEG_INFINITY,
                    stagnant: 0,
                }),
            }
        }
        self.species.retain(|s| !s.members.is_empty());
        if self.species.len() > p.target_species {
            self.threshold += 0.1;
        } else if self.species.len() < p.target_species {
            self.threshold = (self.threshold - 0.1).max(0.3);
        }

        // Stagnation (never drop the species holding the champion).
        for s in &mut self.species {
            let m = s
                .members
                .iter()
                .map(|&i| f[i])
                .fold(f32::NEG_INFINITY, f32::max);
            if m > s.best {
                s.best = m;
                s.stagnant = 0;
            } else {
                s.stagnant += 1;
            }
        }
        if self.species.len() > 2 {
            self.species
                .retain(|s| s.stagnant <= p.stagnation || s.members.contains(&best_i));
        }

        // Offspring counts from shared fitness.
        let min_f = f.iter().copied().fold(f32::INFINITY, f32::min);
        let shares: Vec<f32> = self
            .species
            .iter()
            .map(|s| {
                s.members.iter().map(|&i| f[i] - min_f + 0.05).sum::<f32>() / s.members.len() as f32
            })
            .collect();
        let total: f32 = shares.iter().sum();
        let mut counts: Vec<usize> = shares
            .iter()
            .map(|s| ((s / total) * n as f32).floor() as usize)
            .collect();
        let mut k = 0;
        while counts.iter().sum::<usize>() < n {
            let idx = (0..shares.len())
                .max_by(|&a, &b| shares[a].total_cmp(&shares[b]))
                .map(|best| {
                    if k == 0 {
                        best
                    } else {
                        (best + k) % shares.len()
                    }
                })
                .unwrap();
            counts[idx] += 1;
            k += 1;
        }

        let mut next = Vec::with_capacity(n);
        next.push(champion.clone());
        for (s, &count) in self.species.iter_mut().zip(&counts) {
            let mut members = s.members.clone();
            members.sort_by(|&a, &b| f[b].total_cmp(&f[a]));
            let parents = &members[..((members.len() as f32 * p.survival).ceil() as usize).max(1)];
            for c in 0..count {
                if next.len() >= n {
                    break;
                }
                if c == 0 && members.len() >= 5 && members[0] != best_i {
                    next.push(pop[members[0]].clone());
                    continue;
                }
                let a = parents[rng.below(parents.len())];
                let mut child = if parents.len() > 1 && rng.chance(p.crossover) {
                    let b = parents[rng.below(parents.len())];
                    let (fit, other) = if f[a] >= f[b] { (a, b) } else { (b, a) };
                    pop[fit].crossover(&pop[other], rng)
                } else {
                    pop[a].clone()
                };
                if rng.chance(0.8) {
                    child.mutate_weights(rng, p.weight_power, p.weight_replace);
                }
                if rng.chance(p.add_conn) {
                    child.mutate_add_conn(rng, &mut self.inn);
                }
                if rng.chance(p.add_node) {
                    child.mutate_add_node(rng, &mut self.inn);
                }
                next.push(child);
            }
            s.rep = pop[members[rng.below(members.len())]].clone();
        }
        while next.len() < n {
            let mut g = champion.clone();
            g.mutate_weights(rng, p.weight_power, p.weight_replace);
            next.push(g);
        }
        self.pop = next;

        StepReport {
            best_fitness: f[best_i],
            mean_fitness: mean,
            extra: vec![
                ("species".into(), self.species.len() as f32),
                ("champion hidden nodes".into(), champion.hidden.len() as f32),
                (
                    "champion connections".into(),
                    champion.enabled_conns() as f32,
                ),
                ("compat threshold".into(), self.threshold),
            ],
            champion: Genes::Neat(champion),
            ticks: out.ticks,
        }
    }
}
