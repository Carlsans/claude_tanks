//! Hand-written baseline pilot. It reads the same observation vector as the networks,
//! so it doubles as a sanity check that the sensors are sufficient to play well.

use crate::rng::Rng;
use crate::sim::world::Controller;
use crate::sim::{
    ACTION_REPEAT, ARENA_DIAG, Action, DT, HULL_ROT, MAX_SPEED, RADAR_HALF_ANGLE, RADAR_ROT,
    RAY_RANGE, SHELL_SPEED, TANK_RADIUS, TURRET_ROT, wrap_angle,
};
use std::f32::consts::{FRAC_PI_2, PI};

/// Curriculum levels, from harmless to the strongest bot.
pub const LEVELS: [&str; 5] = ["sitting duck", "wanderer", "rookie", "veteran", "ace"];
pub const MAX_LEVEL: u8 = 4;
/// The fixed yardstick every model is scored against.
pub const VETERAN: u8 = 3;

pub struct ScriptedBot {
    level: u8,
    rng: Rng,
    seed: u64,
    wander: f32,
    reverse: u32,
    sweep: f32,
    // Ace only: strafing side (+1 / -1), actions until the next jink,
    // actions spent stuck, actions until the next shell dodge is allowed.
    side: f32,
    jink: u32,
    stuck: u32,
    dodge: u32,
}

impl ScriptedBot {
    /// The veteran, the reference opponent.
    pub fn new(seed: u64) -> Self {
        Self::with_level(seed, VETERAN)
    }

    pub fn with_level(seed: u64, level: u8) -> Self {
        ScriptedBot {
            level: level.min(MAX_LEVEL),
            rng: Rng::new(seed),
            seed,
            wander: 0.0,
            reverse: 0,
            sweep: 1.0,
            side: 1.0,
            jink: 0,
            stuck: 0,
            dodge: 0,
        }
    }

    /// The ace: exact beam geometry, turret and radar compensate for the hull's
    /// turn, fire only when the shell would pass within the target's radius, and
    /// strafe across the line of sight with random direction changes (jinks)
    /// that break the opponent's lead.
    fn act_ace(&mut self, o: &[f32]) -> Action {
        let dt = ACTION_REPEAT as f32 * DT;
        let turret_rel = o[1].atan2(o[2]);
        let radar_rel = o[3].atan2(o[4]);
        let seen = o[16] > 0.5;
        let known = o[23] != 0.0 || o[24] != 0.0;
        let recent = known && o[22] < 0.6;

        // Enemy bearing relative to the turret and the hull, and its distance.
        let (vs_turret, dist, lead) = if seen {
            let d = (o[17] * ARENA_DIAG).max(1.0);
            let tol = RADAR_HALF_ANGLE + (TANK_RADIUS / d).min(1.0).asin();
            let lateral = o[21] * MAX_SPEED * o[19];
            let lead = (lateral / SHELL_SPEED).clamp(-1.0, 1.0).asin();
            (radar_rel + o[18] * tol, d, lead)
        } else {
            (o[23].atan2(o[24]), o[27] * ARENA_DIAG, 0.0)
        };
        let vs_hull = wrap_angle(vs_turret + turret_rel);

        // Movement.
        self.jink = self.jink.saturating_sub(1);
        self.dodge = self.dodge.saturating_sub(1);
        let front = o[8];
        let back = o[12];
        let (throttle, hull) = if recent {
            let moving_fwd = o[0] >= 0.0;
            let blocked = if moving_fwd { front } else { back } < 0.12;
            if o[0].abs() < 0.05 {
                self.stuck += 1;
            } else {
                self.stuck = 0;
            }
            let incoming = o[28] > 0.5 && self.dodge == 0;
            if (self.jink == 0 && self.dodge == 0) || blocked || self.stuck > 12 || incoming {
                self.side = -self.side;
                self.jink = 12 + self.rng.below(30) as u32;
                self.stuck = 0;
                if incoming {
                    // Hold the new direction until the shell has gone by.
                    self.dodge = 40;
                }
            }
            // Strafe across the line of sight, backing off when close.
            let tilt = if dist < 220.0 { -0.5 } else { 0.0 };
            let m = wrap_angle(vs_hull + self.side * (FRAC_PI_2 - tilt));
            let back_m = wrap_angle(m + PI);
            if m.abs() <= back_m.abs() {
                (1.0, m)
            } else {
                (-1.0, back_m)
            }
        } else {
            // Explore like the veteran.
            self.wander = (self.wander + self.rng.range(-0.3, 0.3)).clamp(-1.0, 1.0);
            if self.reverse > 0 {
                self.reverse -= 1;
            } else if front < 0.08 && o[0].abs() < 0.1 {
                self.reverse = 20;
            }
            if self.reverse > 0 {
                (-1.0, if o[9] > o[15] { -1.0 } else { 1.0 })
            } else {
                let avoid = if front < 0.4 {
                    if o[9] > o[15] { 1.0 } else { -1.0 }
                } else {
                    0.0
                };
                (0.9, avoid + self.wander * 0.5)
            }
        };
        let hull = (hull / (HULL_ROT * dt)).clamp(-1.0, 1.0);
        let hull_turn = hull * HULL_ROT * dt;

        // Turret: lead the target, compensating for the hull's own turn.
        let aim_err = wrap_angle(vs_turret + lead);
        let tracking = seen || (known && o[22] < 0.4);
        let turret = if tracking {
            ((aim_err - hull_turn) / (TURRET_ROT * dt)).clamp(-1.0, 1.0)
        } else {
            // Turn the turret with the radar: the beam sweeps faster.
            1.0
        };
        let turret_turn = turret * TURRET_ROT * dt;

        // Radar: keep the enemy centred, or sweep around where it was last seen.
        let radar = if seen {
            let off = wrap_angle(vs_turret - radar_rel);
            ((off - turret_turn - hull_turn) / (RADAR_ROT * dt)).clamp(-1.0, 1.0)
        } else if tracking {
            let err = wrap_angle(vs_turret - radar_rel);
            if err.abs() > 0.35 {
                self.sweep = err.signum();
            }
            self.sweep
        } else {
            1.0
        };

        // Fire when the shell would pass within ~70% of the target's radius.
        let window = (0.7 * TANK_RADIUS / dist.max(1.0)).atan();
        let clear_shot = o[32] * ARENA_DIAG > dist - 10.0;
        let fire = seen
            && o[6] <= 0.0
            && clear_shot
            && wrap_angle(aim_err - turret_turn).abs() < window
            && dist < RAY_RANGE * 2.0;
        Action {
            throttle,
            hull,
            turret,
            radar,
            fire,
        }
    }
}

impl Controller for ScriptedBot {
    fn reset(&mut self) {
        self.rng = Rng::new(self.seed);
        self.wander = 0.0;
        self.reverse = 0;
        self.sweep = 1.0;
        self.side = 1.0;
        self.jink = 0;
        self.stuck = 0;
        self.dodge = 0;
    }

    fn act(&mut self, o: &[f32]) -> Action {
        if self.level == MAX_LEVEL {
            return self.act_ace(o);
        }
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
