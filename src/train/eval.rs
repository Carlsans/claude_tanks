//! Match evaluation shared by every trainer: opponents, terrains, fitness.

use crate::bots::ScriptedBot;
use crate::nn::{Arch, Brain, Genes};
use crate::rng::Rng;
use crate::sim::world::{Controller, MatchResult, run_match};
#[cfg(feature = "gpu")]
use crate::sim::{ACTION_REPEAT, Action, World};
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

/// Population of fixed-topology networks (`members` are flat parameter vectors).
/// Batched on the GPU when `use_gpu` is set and an adapter exists, otherwise one
/// CPU network per match. Same matches and fitness either way.
pub fn eval_fixed(
    arch: Arch,
    members: &[Vec<f32>],
    use_gpu: bool,
    opps: &[Opponent],
    terrains: &[Terrain],
    shaping: Shaping,
) -> EvalOutcome {
    #[cfg(feature = "gpu")]
    if use_gpu && let Some(g) = crate::gpu::get() {
        match eval_population_gpu(g, arch, members, opps, terrains, shaping) {
            Ok(out) => return out,
            Err(e) => eprintln!("GPU evaluation failed, falling back to the CPU: {e}"),
        }
    }
    let _ = use_gpu;
    eval_population(
        &|i| Box::new(Brain::new(arch, &Genes::Fixed(members[i].clone()))),
        members.len(),
        opps,
        terrains,
        shaping,
    )
}

#[cfg(feature = "gpu")]
struct GpuJob {
    world: World,
    /// Opponent played on the CPU (scripted bot, or a network of another
    /// architecture); `None` when the opponent is a GPU row.
    cpu_op: Option<Box<dyn Controller + Send>>,
    acts: [Action; 2],
}

/// Matches sharing one GPU batch; rows 2j and 2j+1 belong to job j.
#[cfg(feature = "gpu")]
struct Group<'a, 'g> {
    jobs: &'a mut [GpuJob],
    batch: crate::gpu::Batch<'g>,
    obs: Vec<f32>,
    out: Vec<f32>,
    first: bool,
    live: usize,
    pending: Option<wgpu::SubmissionIndex>,
    /// Index of this group's first job in the whole population.
    offset: usize,
}

#[cfg(feature = "gpu")]
impl<'a, 'g> Group<'a, 'g> {
    fn new(jobs: &'a mut [GpuJob], batch: crate::gpu::Batch<'g>) -> Self {
        use crate::nn::fixed::{N_IN, N_OUT};
        let n = jobs.len();
        Group {
            jobs,
            batch,
            obs: vec![0.0; 2 * n * N_IN],
            out: vec![0.0; 2 * n * N_OUT],
            first: true,
            live: 0,
            pending: None,
            offset: 0,
        }
    }

    /// One parallel pass: apply the latest outputs, advance each world to its next
    /// decision tick, and write the next observations (and CPU opponent actions).
    fn advance(&mut self) {
        use crate::nn::fixed::{N_IN, N_OUT};
        let first = self.first;
        self.first = false;
        self.live = self
            .jobs
            .par_iter_mut()
            .zip(self.obs.par_chunks_mut(2 * N_IN))
            .zip(self.out.par_chunks(2 * N_OUT))
            .with_min_len(4)
            .map(|((jb, ob), o)| {
                if jb.world.done() {
                    return 0;
                }
                if !first {
                    jb.acts[0] = Action::from_outputs(&o[..N_OUT]);
                    if jb.cpu_op.is_none() {
                        jb.acts[1] = Action::from_outputs(&o[N_OUT..]);
                    }
                    for _ in 0..ACTION_REPEAT {
                        if jb.world.done() {
                            return 0;
                        }
                        jb.world.step(jb.acts);
                    }
                    if jb.world.done() {
                        return 0;
                    }
                }
                ob[..N_IN].copy_from_slice(&jb.world.observe(0));
                let o1 = jb.world.observe(1);
                match &mut jb.cpu_op {
                    Some(op) => jb.acts[1] = op.act(&o1),
                    None => ob[N_IN..].copy_from_slice(&o1),
                }
                1
            })
            .sum();
    }

    /// Hand the remaining matches to CPU networks carrying the GPU's recurrent
    /// state, and play them out independently.
    fn finish_on_cpu(
        &mut self,
        arch: Arch,
        sets: &[&[f32]],
        row_param: &[u32],
    ) -> Result<(), String> {
        use crate::nn::fixed::{FixedNet, N_IN};
        let hidden = self.batch.hidden()?;
        let h = hidden.len() / (2 * self.jobs.len()).max(1);
        let obs = &self.obs;
        let offset = self.offset;
        self.jobs.par_iter_mut().enumerate().for_each(|(j, jb)| {
            if jb.world.done() {
                return;
            }
            let net = |r: usize| {
                let mut n = FixedNet::new(arch, sets[row_param[2 * offset + r] as usize].to_vec());
                n.set_hidden_state(&hidden[r * h..(r + 1) * h]);
                n
            };
            let mut me = net(2 * j);
            let mut op = jb.cpu_op.is_none().then(|| net(2 * j + 1));
            // Observations for this decision tick are already in `obs`.
            let mut o0: [f32; N_IN] = obs[2 * j * N_IN..(2 * j + 1) * N_IN].try_into().unwrap();
            let mut o1: [f32; N_IN] = obs[(2 * j + 1) * N_IN..(2 * j + 2) * N_IN]
                .try_into()
                .unwrap();
            loop {
                jb.acts[0] = Action::from_outputs(&me.forward(&o0));
                if let Some(op) = &mut op {
                    jb.acts[1] = Action::from_outputs(&op.forward(&o1));
                }
                for _ in 0..ACTION_REPEAT {
                    if jb.world.done() {
                        return;
                    }
                    jb.world.step(jb.acts);
                }
                if jb.world.done() {
                    return;
                }
                o0 = jb.world.observe(0);
                o1 = jb.world.observe(1);
                if let Some(cpu_op) = &mut jb.cpu_op {
                    jb.acts[1] = cpu_op.act(&o1);
                }
            }
        });
        self.live = 0;
        Ok(())
    }
}

/// GPU version of `eval_population`: every match advances in lockstep, and one
/// GPU dispatch per decision step computes all the networks. Every match owns two
/// GPU rows (candidate, opponent); the opponent row is unused when the opponent
/// runs on the CPU (scripted bots, networks of another architecture).
#[cfg(feature = "gpu")]
pub fn eval_population_gpu(
    gpu: &crate::gpu::Gpu,
    arch: Arch,
    members: &[Vec<f32>],
    opps: &[Opponent],
    terrains: &[Terrain],
    shaping: Shaping,
) -> Result<EvalOutcome, String> {
    if !crate::gpu::supports(arch) {
        return Err(format!("no GPU kernel for {}", arch.name()));
    }
    let (n, k) = (members.len(), opps.len());
    let mut sets: Vec<&[f32]> = members.iter().map(|m| m.as_slice()).collect();
    // Opponent networks of the same architecture run on the GPU too.
    let mut opp_set: Vec<Option<u32>> = vec![None; k];
    for (m, o) in opps.iter().enumerate() {
        if let Opponent::Net(a, g) = o
            && let Genes::Fixed(p) = &**g
            && *a == arch
        {
            opp_set[m] = Some(sets.len() as u32);
            sets.push(p);
        }
    }
    let mut row_param = Vec::with_capacity(2 * n * k);
    let mut jobs: Vec<GpuJob> = (0..n * k)
        .map(|j| {
            let m = j % k;
            row_param.extend([(j / k) as u32, opp_set[m].unwrap_or(0)]);
            let cpu_op: Option<Box<dyn Controller + Send>> = match &opps[m] {
                _ if opp_set[m].is_some() => None,
                Opponent::Scripted(s, l) => Some(Box::new(ScriptedBot::with_level(*s, *l))),
                Opponent::Net(a, g) => Some(Box::new(Brain::new(*a, g))),
            };
            GpuJob {
                world: World::new(terrains[m].clone(), m % 2 == 1),
                cpu_op,
                acts: [Action::default(); 2],
            }
        })
        .collect();
    // Two halves in flight: the CPU advances one while the GPU computes the other.
    let params = gpu.upload(arch, &sets);
    let half = jobs.len().div_ceil(2);
    let (ja, jb) = jobs.split_at_mut(half);
    let mut groups = [
        Group::new(ja, params.batch(&row_param[..2 * half])),
        Group::new(jb, params.batch(&row_param[2 * half..])),
    ];
    groups[1].offset = half;
    // Few matches left: the GPU's fixed per-step latency is no longer worth it.
    let tail = 8 * rayon::current_num_threads();
    for g in &mut groups {
        g.advance();
        g.pending = (g.live > 0).then(|| g.batch.submit(&g.obs));
    }
    while groups.iter().any(|g| g.pending.is_some()) {
        for g in &mut groups {
            let Some(i) = g.pending.take() else { continue };
            g.batch.wait(i, &mut g.out)?;
            g.advance();
            if g.live > tail {
                g.pending = Some(g.batch.submit(&g.obs));
            } else if g.live > 0 {
                g.finish_on_cpu(arch, &sets, &row_param)?;
            }
        }
    }
    drop(groups);
    let mut fitness_sum = vec![0.0; n];
    let mut ticks = 0;
    for (j, jb) in jobs.iter().enumerate() {
        let r = jb.world.result.as_ref().ok_or("match without result")?;
        fitness_sum[j / k] += fitness(r, 0, shaping) / k as f32;
        ticks += r.ticks as u64;
    }
    Ok(EvalOutcome {
        fitness: fitness_sum,
        ticks,
    })
}

/// Win / draw / loss rates of a pilot against the scripted bot on a fixed benchmark set.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub win: f32,
    pub draw: f32,
    pub loss: f32,
}

impl Record {
    /// Average of several records (all zero when there are none).
    pub fn mean(rs: &[Record]) -> Record {
        let n = rs.len().max(1) as f32;
        Record {
            win: rs.iter().map(|r| r.win).sum::<f32>() / n,
            draw: rs.iter().map(|r| r.draw).sum::<f32>() / n,
            loss: rs.iter().map(|r| r.loss).sum::<f32>() / n,
        }
    }
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
