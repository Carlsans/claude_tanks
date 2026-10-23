//! NEAT genomes (topology + weights) and their recurrent-capable phenotype.

use super::fixed::{N_IN, N_OUT};
use crate::rng::Rng;
use std::collections::HashMap;

pub const BIAS: u32 = N_IN as u32;
pub const FIRST_OUT: u32 = N_IN as u32 + 1;
pub const FIRST_HIDDEN: u32 = FIRST_OUT + N_OUT as u32;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConnGene {
    pub innov: u32,
    pub from: u32,
    pub to: u32,
    pub w: f32,
    pub enabled: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Genome {
    /// Hidden node ids (inputs, bias and outputs are implicit).
    pub hidden: Vec<u32>,
    /// Sorted by innovation number.
    pub conns: Vec<ConnGene>,
}

/// Global innovation bookkeeping so identical mutations share numbers.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Innovations {
    pub next_innov: u32,
    pub next_node: u32,
    conn: HashMap<(u32, u32), u32>,
    /// Splitting connection `innov` creates this node id.
    split: HashMap<u32, u32>,
}

impl Innovations {
    pub fn new() -> Self {
        Innovations {
            next_innov: 0,
            next_node: FIRST_HIDDEN,
            ..Default::default()
        }
    }
    pub fn conn(&mut self, from: u32, to: u32) -> u32 {
        let next = &mut self.next_innov;
        *self.conn.entry((from, to)).or_insert_with(|| {
            *next += 1;
            *next - 1
        })
    }
    fn split(&mut self, innov: u32) -> u32 {
        let next = &mut self.next_node;
        *self.split.entry(innov).or_insert_with(|| {
            *next += 1;
            *next - 1
        })
    }
}

impl Genome {
    /// Inputs and bias fully connected to outputs with small random weights.
    pub fn initial(rng: &mut Rng, inn: &mut Innovations) -> Genome {
        let mut conns = Vec::new();
        for from in 0..=BIAS {
            for to in FIRST_OUT..FIRST_HIDDEN {
                conns.push(ConnGene {
                    innov: inn.conn(from, to),
                    from,
                    to,
                    w: rng.gauss() * 0.3,
                    enabled: true,
                });
            }
        }
        Genome {
            hidden: Vec::new(),
            conns,
        }
    }

    pub fn enabled_conns(&self) -> usize {
        self.conns.iter().filter(|c| c.enabled).count()
    }

    pub fn mutate_weights(&mut self, rng: &mut Rng, power: f32, replace_p: f32) {
        for c in &mut self.conns {
            if rng.chance(replace_p) {
                c.w = rng.gauss();
            } else {
                c.w = (c.w + rng.gauss() * power).clamp(-8.0, 8.0);
            }
        }
    }

    pub fn mutate_add_conn(&mut self, rng: &mut Rng, inn: &mut Innovations) {
        let sources: Vec<u32> = (0..=BIAS)
            .chain(FIRST_OUT..FIRST_HIDDEN)
            .chain(self.hidden.iter().copied())
            .collect();
        let targets: Vec<u32> = (FIRST_OUT..FIRST_HIDDEN)
            .chain(self.hidden.iter().copied())
            .collect();
        for _ in 0..20 {
            let from = sources[rng.below(sources.len())];
            let to = targets[rng.below(targets.len())];
            if let Some(c) = self.conns.iter_mut().find(|c| c.from == from && c.to == to) {
                if !c.enabled {
                    c.enabled = true;
                    return;
                }
                continue;
            }
            let innov = inn.conn(from, to);
            self.insert(ConnGene {
                innov,
                from,
                to,
                w: rng.gauss() * 0.5,
                enabled: true,
            });
            return;
        }
    }

    pub fn mutate_add_node(&mut self, rng: &mut Rng, inn: &mut Innovations) {
        let enabled: Vec<usize> = (0..self.conns.len())
            .filter(|&i| self.conns[i].enabled)
            .collect();
        if enabled.is_empty() {
            return;
        }
        let i = enabled[rng.below(enabled.len())];
        let old = self.conns[i].clone();
        let node = inn.split(old.innov);
        if self.hidden.contains(&node) {
            return;
        }
        self.conns[i].enabled = false;
        self.hidden.push(node);
        let a = ConnGene {
            innov: inn.conn(old.from, node),
            from: old.from,
            to: node,
            w: 1.0,
            enabled: true,
        };
        let b = ConnGene {
            innov: inn.conn(node, old.to),
            from: node,
            to: old.to,
            w: old.w,
            enabled: true,
        };
        self.insert(a);
        self.insert(b);
    }

    fn insert(&mut self, c: ConnGene) {
        let pos = self.conns.partition_point(|x| x.innov < c.innov);
        if self.conns.get(pos).is_some_and(|x| x.innov == c.innov) {
            self.conns[pos] = c;
        } else {
            self.conns.insert(pos, c);
        }
    }

    /// Crossover; `self` must be the fitter parent (its disjoint/excess genes are kept).
    pub fn crossover(&self, other: &Genome, rng: &mut Rng) -> Genome {
        let theirs: HashMap<u32, &ConnGene> = other.conns.iter().map(|c| (c.innov, c)).collect();
        let conns: Vec<ConnGene> = self
            .conns
            .iter()
            .map(|c| match theirs.get(&c.innov) {
                Some(o) => {
                    let mut g = if rng.chance(0.5) {
                        c.clone()
                    } else {
                        (*o).clone()
                    };
                    g.enabled = if !c.enabled || !o.enabled {
                        rng.chance(0.25)
                    } else {
                        true
                    };
                    g
                }
                None => c.clone(),
            })
            .collect();
        let mut hidden: Vec<u32> = conns
            .iter()
            .flat_map(|c| [c.from, c.to])
            .filter(|&id| id >= FIRST_HIDDEN)
            .collect();
        hidden.extend(self.hidden.iter().copied());
        hidden.sort_unstable();
        hidden.dedup();
        Genome { hidden, conns }
    }

    /// NEAT compatibility distance.
    pub fn distance(&self, other: &Genome, c1: f32, c3: f32) -> f32 {
        let (mut i, mut j) = (0, 0);
        let (mut mismatched, mut matched, mut wdiff) = (0usize, 0usize, 0.0f32);
        while i < self.conns.len() && j < other.conns.len() {
            let (a, b) = (&self.conns[i], &other.conns[j]);
            if a.innov == b.innov {
                matched += 1;
                wdiff += (a.w - b.w).abs();
                i += 1;
                j += 1;
            } else if a.innov < b.innov {
                mismatched += 1;
                i += 1;
            } else {
                mismatched += 1;
                j += 1;
            }
        }
        mismatched += self.conns.len() - i + other.conns.len() - j;
        let n = self.conns.len().max(other.conns.len()).max(1) as f32;
        c1 * mismatched as f32 / n + c3 * wdiff / matched.max(1) as f32
    }
}

/// Executable network. Nodes are evaluated in (approximate) topological order each step;
/// connections pointing backwards read the previous step's activation, giving recurrence.
#[derive(Clone, Debug)]
pub struct NeatNet {
    /// Activation of every node, indexed by dense slot.
    act: Vec<f32>,
    /// (slot, incoming (from_slot, weight)) in evaluation order.
    order: Vec<(usize, Vec<(usize, f32)>)>,
    out_slots: [usize; N_OUT],
}

impl NeatNet {
    pub fn new(g: &Genome) -> NeatNet {
        let mut slot: HashMap<u32, usize> = HashMap::new();
        for id in 0..FIRST_HIDDEN {
            slot.insert(id, id as usize);
        }
        for &h in &g.hidden {
            let n = slot.len();
            slot.entry(h).or_insert(n);
        }
        let mut incoming: HashMap<u32, Vec<(usize, f32)>> = HashMap::new();
        let mut indeg: HashMap<u32, usize> = HashMap::new();
        let mut fwd: HashMap<u32, Vec<u32>> = HashMap::new();
        for c in g.conns.iter().filter(|c| c.enabled) {
            incoming.entry(c.to).or_default().push((slot[&c.from], c.w));
            if c.from >= FIRST_OUT && c.from != c.to {
                *indeg.entry(c.to).or_default() += 1;
                fwd.entry(c.from).or_default().push(c.to);
            }
        }
        // Kahn's algorithm over computed nodes; leftovers (cycles) appended in id order.
        let computed: Vec<u32> = g
            .hidden
            .iter()
            .copied()
            .chain(FIRST_OUT..FIRST_HIDDEN)
            .collect();
        let mut ready: Vec<u32> = computed
            .iter()
            .copied()
            .filter(|n| indeg.get(n).copied().unwrap_or(0) == 0)
            .collect();
        let mut seq = Vec::new();
        let mut done = std::collections::HashSet::new();
        while let Some(n) = ready.pop() {
            if !done.insert(n) {
                continue;
            }
            seq.push(n);
            for &m in fwd.get(&n).map(|v| v.as_slice()).unwrap_or(&[]) {
                let d = indeg.get_mut(&m).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.push(m);
                }
            }
        }
        for n in computed {
            if !done.contains(&n) {
                seq.push(n);
            }
        }
        let order = seq
            .into_iter()
            .map(|n| (slot[&n], incoming.remove(&n).unwrap_or_default()))
            .collect();
        let mut out_slots = [0; N_OUT];
        for (k, s) in out_slots.iter_mut().enumerate() {
            *s = FIRST_OUT as usize + k;
        }
        NeatNet {
            act: vec![0.0; slot.len()],
            order,
            out_slots,
        }
    }

    pub fn reset(&mut self) {
        self.act.iter_mut().for_each(|v| *v = 0.0);
    }

    pub fn forward(&mut self, x: &[f32]) -> [f32; N_OUT] {
        self.act[..N_IN].copy_from_slice(&x[..N_IN]);
        self.act[BIAS as usize] = 1.0;
        for (s, inc) in &self.order {
            let sum: f32 = inc.iter().map(|&(f, w)| self.act[f] * w).sum();
            self.act[*s] = sum.tanh();
        }
        let mut out = [0.0; N_OUT];
        for (o, &s) in out.iter_mut().zip(&self.out_slots) {
            *o = self.act[s];
        }
        out
    }
}
