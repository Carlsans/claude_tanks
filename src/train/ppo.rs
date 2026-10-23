//! PPO (placeholder until implemented).

use super::{StepEnv, StepReport, TrainConfig};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PpoParams {}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ppo {}

impl Ppo {
    pub fn new(_cfg: &TrainConfig, _rng: &mut Rng) -> Ppo {
        Ppo {}
    }
    pub fn step(&mut self, _env: StepEnv) -> StepReport {
        unimplemented!("PPO")
    }
}
