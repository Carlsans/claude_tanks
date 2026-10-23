//! Hand-written baseline pilot. It reads the same observation vector as the networks,
//! so it doubles as a sanity check that the sensors are sufficient to play well.

use crate::rng::Rng;
use crate::sim::world::Controller;
use crate::sim::{Action, MAX_SPEED, RADAR_HALF_ANGLE, SHELL_SPEED, wrap_angle};

/// Curriculum levels, from harmless to the full bot.
pub const LEVELS: [&str; 4] = ["sitting duck", "wanderer", "rookie", "veteran"];
pub const MAX_LEVEL: u8 = 3;

pub struct ScriptedBot {
    level: u8,
    rng: Rng,
    seed: u64,
    wander: f32,
    reverse: u32,
    sweep: f32,
}

impl ScriptedBot {
    /// Full-strength bot.
    pub fn new(seed: u64) -> Self {
        Self::with_level(seed, MAX_LEVEL)
    }

    pub fn with_level(seed: u64, level: u8) -> Self {
        ScriptedBot {
            level: level.min(MAX_LEVEL),
            rng: Rng::new(seed),
            seed,
            wander: 0.0,
            reverse: 0,
            sweep: 1.0,
        }
    }
}

impl Controller for ScriptedBot {
    fn reset(&mut self) {
        self.rng = Rng::new(self.seed);
        self.wander = 0.0;
        self.reverse = 0;
        self.sweep = 1.0;
    }

    fn act(&mut self, o: &[f32]) -> Action {
        let radar_rel = o[3].atan2(o[4]);
        let seen = o[16] > 0.5;
        let known = o[23] != 0.0 || o[24] != 0.0;
        let last_vs_turret = o[23].atan2(o[24]);

        // Radar: lock on when seen, sweep around the last known bearing otherwise.
        let radar = if seen {
            (o[18] * 3.0).clamp(-1.0, 1.0)
        } else if known && o[22] < 0.3 {
            // Sweep back and forth across the last known bearing.
            let err = wrap_angle(last_vs_turret - radar_rel);
            if err.abs() > 0.4 {
                self.sweep = err.signum();
            }
            self.sweep
        } else {
            1.0
        };

        // Turret: aim at the enemy with a small lead.
        let mut turret = 0.0;
        let mut aim_err = f32::MAX;
        if seen {
            let lateral = o[21] * MAX_SPEED * o[19];
            let lead = (lateral / SHELL_SPEED).clamp(-1.0, 1.0).asin();
            let target = radar_rel + o[18] * RADAR_HALF_ANGLE + lead;
            aim_err = wrap_angle(target);
            turret = (aim_err * 6.0).clamp(-1.0, 1.0);
        } else if known {
            turret = (last_vs_turret * 3.0).clamp(-1.0, 1.0);
        }
        let clear_shot = o[32] > o[17] - 0.01;
        let fire = match self.level {
            0 | 1 => false,
            // Rookie: sloppy aim and slow trigger.
            2 => seen && aim_err.abs() < 0.25 && o[6] <= 0.0 && self.rng.chance(0.15),
            _ => seen && aim_err.abs() < 0.04 && clear_shot && o[6] <= 0.0,
        };
        if self.level == 2 {
            turret *= 0.4;
        }
        if self.level == 0 {
            return Action {
                radar: 1.0,
                ..Action::default()
            };
        }

        // Movement: wander, avoid walls, back off when stuck.
        let front = o[8];
        let (left, right) = (o[9], o[15]);
        self.wander = (self.wander + self.rng.range(-0.3, 0.3)).clamp(-1.0, 1.0);
        if self.reverse > 0 {
            self.reverse -= 1;
        } else if front < 0.08 && o[0].abs() < 0.1 {
            self.reverse = 20;
        }
        let (throttle, hull) = if self.reverse > 0 {
            (-1.0, if left > right { -1.0 } else { 1.0 })
        } else {
            let avoid = if front < 0.4 {
                if left > right { 1.0 } else { -1.0 }
            } else {
                0.0
            };
            let shell_dodge = if o[28] > 0.5 { 0.6 } else { 0.0 };
            (
                0.8,
                (avoid + self.wander * 0.5 + shell_dodge).clamp(-1.0, 1.0),
            )
        };
        Action {
            throttle,
            hull,
            turret,
            radar,
            fire,
        }
    }
}
