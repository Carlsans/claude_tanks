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
}

impl Arch {
    pub const ALL: [Arch; 7] = [
        Arch::MlpS,
        Arch::MlpL,
        Arch::Elman,
        Arch::Gru,
        Arch::GruL,
        Arch::GruClock,
        Arch::Neat,
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
