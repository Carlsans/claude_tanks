//! Neural pilots: architectures, inference, and saved models.

pub mod fixed;
pub mod grad;
pub mod neat;

use crate::sim::world::{Action, Controller};
use fixed::FixedNet;
use neat::{Genome, NeatNet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Arch {
    /// MLP, 1 hidden layer of 32.
    MlpS,
    /// MLP, 2 hidden layers of 64.
    MlpL,
    /// Elman recurrent net, 32 hidden.
    Elman,
    /// Gated recurrent unit, 32 hidden.
    Gru,
    /// Gated recurrent unit, 64 hidden.
    GruL,
    /// GRU (32) plus 16 clocks with fixed time constants from 0.1 s to 30 s,
    /// and a direct path from the inputs to the outputs.
    GruClock,
    /// Evolving topology (NEAT), recurrent connections allowed.
    Neat,
    /// GRU (32) that also reads the battle memory (Robocode rules: who wins
    /// the rounds, hit rates, where bullets find each side; see
    /// `royale::memory`). Elsewhere those inputs stay at zero.
    GruMem,
    /// Mixture of experts: two `GruMem` networks and a gate that reads the
    /// battle memory and weighs their outputs, so that what the battle has
    /// shown of the opponent can pick the expert.
    Moe,
}

impl Arch {
    pub const ALL: [Arch; 9] = [
        Arch::MlpS,
        Arch::MlpL,
        Arch::Elman,
        Arch::Gru,
        Arch::GruL,
        Arch::GruClock,
        Arch::Neat,
        Arch::GruMem,
        Arch::Moe,
    ];
    pub const FIXED: [Arch; 6] = [
        Arch::MlpS,
        Arch::MlpL,
        Arch::Elman,
        Arch::Gru,
        Arch::GruL,
        Arch::GruClock,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Arch::MlpS => "MLP-S (32)",
            Arch::MlpL => "MLP-L (64x64)",
            Arch::Elman => "Elman RNN (32)",
            Arch::Gru => "GRU (32)",
            Arch::GruL => "GRU (64)",
            Arch::GruClock => "GRU + clocks (32+16)",
            Arch::Neat => "NEAT",
            Arch::GruMem => "GRU + battle memory (32)",
            Arch::Moe => "Mixture of 2 experts (GRU + battle memory)",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Arch::MlpS => "mlp_s",
            Arch::MlpL => "mlp_l",
            Arch::Elman => "elman",
            Arch::Gru => "gru",
            Arch::GruL => "gru_l",
            Arch::GruClock => "gru_clock",
            Arch::Neat => "neat",
            Arch::GruMem => "gru_mem",
            Arch::Moe => "moe",
        }
    }

    pub fn param_count(self) -> usize {
        match self {
            Arch::Neat => 0,
            a => fixed::Layout::new(a).1,
        }
    }

    pub fn recurrent(self) -> bool {
        !matches!(self, Arch::MlpS | Arch::MlpL)
    }

    /// Battle-memory inputs read after the usual observations.
    pub fn memory_inputs(self) -> usize {
        match self {
            Arch::GruMem | Arch::Moe => crate::royale::memory::N_MEM,
            _ => 0,
        }
    }
}

/// Network weights in either representation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Genes {
    Fixed(Vec<f32>),
    Neat(Genome),
}

/// A trained pilot as saved on disk.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ModelFile {
    pub name: String,
    pub arch: Arch,
    /// Training method ("GA", "NEAT", "PPO").
    pub method: String,
    pub generation: u32,
    pub fitness: f32,
    /// Win rate vs the scripted bot at save time.
    pub vs_scripted: f32,
    /// The rules it was trained under.
    #[serde(default)]
    pub rules: crate::sim::Rules,
    /// Robocode rules: the network's state carries over from one round of a
    /// battle to the next.
    #[serde(default)]
    pub keep_state: bool,
    pub genes: Genes,
}

impl ModelFile {
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &Path) -> std::io::Result<ModelFile> {
        let s = std::fs::read_to_string(path)?;
        serde_json::from_str(&s).map_err(std::io::Error::other)
    }

    pub fn brain(&self) -> Brain {
        Brain::new(self.arch, &self.genes)
    }
}

/// Executable network implementing `Controller`.
#[derive(Clone, Debug)]
pub enum Brain {
    Fixed(FixedNet),
    Neat(NeatNet),
}

impl Brain {
    pub fn new(arch: Arch, genes: &Genes) -> Brain {
        match genes {
            Genes::Fixed(p) => Brain::Fixed(FixedNet::new(arch, p.clone())),
            Genes::Neat(g) => Brain::Neat(NeatNet::new(g)),
        }
    }

    /// Battle-memory inputs the network reads after the observations.
    pub fn memory_inputs(&self) -> usize {
        match self {
            Brain::Fixed(n) => n.arch.memory_inputs(),
            Brain::Neat(_) => 0,
        }
    }

    /// Mixture of experts: the weight the gate gave each expert at the last
    /// step.
    pub fn gate(&self) -> Option<[f32; fixed::EXPERTS]> {
        match self {
            Brain::Fixed(n) => n.gate(),
            Brain::Neat(_) => None,
        }
    }

    pub fn outputs(&mut self, obs: &[f32]) -> [f32; fixed::N_OUT] {
        match self {
            Brain::Fixed(n) => n.forward(obs),
            Brain::Neat(n) => n.forward(obs),
        }
    }
}

impl Controller for Brain {
    fn reset(&mut self) {
        match self {
            Brain::Fixed(n) => n.reset(),
            Brain::Neat(n) => n.reset(),
        }
    }
    fn act(&mut self, obs: &[f32]) -> Action {
        Action::from_outputs(&self.outputs(obs))
    }
}

pub fn models_dir() -> PathBuf {
    PathBuf::from("models")
}

/// All `*.json` model files under `models/`, newest first.
pub fn list_models() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![models_dir()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p);
            }
        }
    }
    let mtime = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    out.sort_by_key(|p| std::cmp::Reverse(mtime(p)));
    out
}
