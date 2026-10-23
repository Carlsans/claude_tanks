//! Match evaluation shared by every trainer: opponents, terrains, fitness.

use crate::bots::ScriptedBot;
use crate::nn::{Arch, Brain, Genes};
use crate::rng::Rng;
use crate::sim::world::{Controller, MatchResult, run_match};
use crate::sim::{MATCH_TIME, Terrain};
use rayon::prelude::*;
use std::sync::Arc;

/// An opponent a candidate is evaluated against.
#[derive(Clone)]
pub enum Opponent {
    Scripted(u64, u8),
    Net(Arch, Arc<Genes>),
}

impl Opponent {
    pub fn controller(&self) -> Box<dyn Controller> {
        match self {
            Opponent::Scripted(s, l) => Box::new(ScriptedBot::with_level(*s, *l)),
            Opponent::Net(a, g) => Box::new(Brain::new(*a, g)),
        }
    }
}

/// How much shaping is added to the win/draw/loss outcome.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Shaping {
    pub weight: f32,
}

/// Fitness of tank `side` in a finished match: +1 win, 0 draw, -1 loss,
/// plus shaping for radar tracking, aiming, accurate fire and survival.
pub fn fitness(r: &MatchResult, side: usize, shaping: Shaping) -> f32 {
    let outcome = match r.winner {
        Some(w) if w == side => 1.0,
        Some(_) => -1.0,
        None => 0.0,
    };
    let s = &r.stats[side];
    let elapsed = (r.ticks as f32 * crate::sim::DT).max(1.0);
    let radar = (s.radar_time / elapsed).min(1.0);
    let aim = (s.aim / elapsed).min(1.0);
    // Average closeness of shots, scaled down when only a couple were fired.
    let near = if s.shots > 0 {
        s.near / s.shots as f32 * (s.shots as f32 / 3.0).min(1.0)
    } else {
        0.0
    };
    let survive = if r.winner == Some(side) {
        1.0
    } else {
        r.ticks as f32 * crate::sim::DT / MATCH_TIME
    };
    let shape = 0.25 * radar + 0.25 * aim + 0.3 * near + 0.2 * survive;
    outcome + shaping.weight * shape
}

/// A fixed set of terrains for one evaluation round.
pub fn terrains(rng: &mut Rng, n: usize, density: (f32, f32)) -> Vec<Terrain> {
    (0..n)
        .map(|_| {
            let seed = rng.next_u64() % 1_000_000_000;
            Terrain::generate(seed, rng.range(density.0, density.1))
        })
        .collect()
}

pub struct EvalOutcome {
    pub fitness: Vec<f32>,
    pub ticks: u64,
}

/// Every candidate plays match k against `opps[k]` on `terrains[k]`
/// (sides alternate with k). All candidates face the same set, which keeps
/// the comparison fair within a generation.
pub fn eval_population(
    make: &(dyn Fn(usize) -> Box<dyn Controller> + Sync),
    n: usize,
    opps: &[Opponent],
    terrains: &[Terrain],
    shaping: Shaping,
) -> EvalOutcome {
    let k = opps.len();
    let results: Vec<(usize, f32, u64)> = (0..n * k)
        .into_par_iter()
        .map(|job| {
            let (i, m) = (job / k, job % k);
            let mut me = make(i);
            let mut op = opps[m].controller();
            let swap = m % 2 == 1;
            let r = run_match(terrains[m].clone(), swap, me.as_mut(), op.as_mut());
            (i, fitness(&r, 0, shaping), r.ticks as u64)
        })
        .collect();
    let mut fitness = vec![0.0; n];
    let mut ticks = 0;
    for (i, f, t) in results {
        fitness[i] += f / k as f32;
        ticks += t;
    }
    EvalOutcome { fitness, ticks }
}

/// Win / draw / loss rates of a pilot against the scripted bot on a fixed benchmark set.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub win: f32,
    pub draw: f32,
    pub loss: f32,
}

pub fn vs_scripted(
    make: &(dyn Fn() -> Box<dyn Controller> + Sync),
    matches: usize,
    level: u8,
) -> (Record, u64) {
    let res: Vec<(Option<usize>, u64)> = (0..matches)
        .into_par_iter()
        .map(|j| {
            let t = Terrain::generate(900_000_000 + j as u64, (j % 11) as f32 / 10.0);
            let mut me = make();
            let mut op = ScriptedBot::with_level(j as u64 + 17, level);
            let r = run_match(t, j % 2 == 1, me.as_mut(), &mut op);
            (r.winner, r.ticks as u64)
        })
        .collect();
    let mut rec = Record::default();
    let mut ticks = 0;
    for (w, t) in res {
        ticks += t;
        match w {
            Some(0) => rec.win += 1.0,
            Some(_) => rec.loss += 1.0,
            None => rec.draw += 1.0,
        }
    }
    let n = matches as f32;
    rec.win /= n;
    rec.draw /= n;
    rec.loss /= n;
    (rec, ticks)
}

/// Head-to-head between two pilots over `matches` matches (sides alternate).
pub fn head_to_head(
    a: &(dyn Fn() -> Box<dyn Controller> + Sync),
    b: &(dyn Fn() -> Box<dyn Controller> + Sync),
    matches: usize,
    seed: u64,
) -> Record {
    let res: Vec<Option<usize>> = (0..matches)
        .into_par_iter()
        .map(|j| {
            let t = Terrain::generate(seed + j as u64, ((j / 2) % 11) as f32 / 10.0);
            run_match(t, j % 2 == 1, a().as_mut(), b().as_mut()).winner
        })
        .collect();
    let n = matches as f32;
    Record {
        win: res.iter().filter(|w| **w == Some(0)).count() as f32 / n,
        loss: res.iter().filter(|w| **w == Some(1)).count() as f32 / n,
        draw: res.iter().filter(|w| w.is_none()).count() as f32 / n,
    }
}
