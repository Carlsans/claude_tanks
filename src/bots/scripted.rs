//! Hand-written baseline pilot. It reads the same observation vector as the networks,
//! so it doubles as a sanity check that the sensors are sufficient to play well.

use crate::rng::Rng;
use crate::sim::sensors::{N_RAYS, ray_angle};
use crate::sim::world::Controller;
use crate::sim::{
    ACTION_REPEAT, ARENA_DIAG, Action, DT, HULL_ROT, MAX_AMMO, MAX_SPEED, RADAR_HALF_ANGLE,
    RADAR_ROT, RAY_RANGE, SHELL_SPEED, TANK_RADIUS, TURRET_ROT, wrap_angle,
};
use std::f32::consts::{FRAC_PI_2, PI};

/// Curriculum levels, from harmless to the strongest bots.
pub const LEVELS: [&str; 8] = [
    "sitting duck",
    "wanderer",
    "rookie",
    "veteran",
    "ace",
    "hunter",
    "brawler",
    "stalker",
];
pub const MAX_LEVEL: u8 = 7;
/// The fixed yardstick every model is scored against.
pub const VETERAN: u8 = 3;
/// The first "pro" bot: exact aim, radar lock, strafing and dodging.
pub const ACE: u8 = 4;

/// How a pro bot (the ace and up) fights. Every pro bot shares the ace's aim,
/// radar control, strafing and dodging; the style sets the range it keeps, how
/// picky it is about shots and whether it goes looking for the enemy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// Back away while strafing when closer than this.
    pub min_range: f32,
    /// Close in while strafing when farther than this.
    pub max_range: f32,
    /// How far (radians) the strafe direction tilts to back away or close in.
    pub tilt: f32,
    /// Fire when the shell would pass within this fraction of the target's radius.
    pub window: f32,
    /// With the last shell in the magazine, fire only closer than this.
    pub last_shell_range: f32,
    /// When the enemy has been out of sight for a while, drive to where it was
    /// last seen (steering around walls) instead of wandering.
    pub hunt: bool,
    /// Strafe this many actions (plus up to `jink_span`) before switching side.
    pub jink_min: u32,
    pub jink_span: u32,
    /// After dodging a shell, hold the new direction this many actions.
    pub dodge_hold: u32,
}

impl Style {
    pub const ACE: Style = Style {
        min_range: 220.0,
        max_range: f32::INFINITY,
        tilt: 0.5,
        window: 0.7,
        last_shell_range: f32::INFINITY,
        hunt: false,
        jink_min: 12,
        jink_span: 30,
        dodge_hold: 40,
    };
    /// Goes looking for the enemy, closes in from far away, and changes
    /// direction less often than the ace: after a dodge it holds its course
    /// for over four seconds, which beats nets that learned to expect the ace's
    /// quick flips.
    pub const HUNTER: Style = Style {
        max_range: 800.0,
        tilt: 0.3,
        hunt: true,
        jink_min: 30,
        jink_span: 60,
        dodge_hold: 140,
        ..Style::ACE
    };
    /// The hunter's movement, but it fights up close.
    pub const BRAWLER: Style = Style {
        min_range: 0.0,
        max_range: 200.0,
        tilt: 0.5,
        ..Style::HUNTER
    };
    /// The hunter, but it dodges only the first shell and then holds its
    /// course; picky shots.
    pub const STALKER: Style = Style {
        window: 0.9,
        dodge_hold: 1000,
        ..Style::HUNTER
    };

    /// The style of a curriculum level (`None` below the ace).
    pub fn of_level(level: u8) -> Option<Style> {
        match level {
            4 => Some(Style::ACE),
            5 => Some(Style::HUNTER),
            6 => Some(Style::BRAWLER),
            7 => Some(Style::STALKER),
            _ => None,
        }
    }
}

pub struct ScriptedBot {
    level: u8,
    style: Option<Style>,
    rng: Rng,
    seed: u64,
    wander: f32,
    reverse: u32,
    sweep: f32,
    // Pro bots: strafing side (+1 / -1), actions until the next jink,
    // actions spent stuck, actions until the next shell dodge is allowed.
    side: f32,
    jink: u32,
    stuck: u32,
    dodge: u32,
    // Hunting: own heading integrated from the hull commands (the sensors are
    // all relative), the chosen travel direction in that frame, and how many
    // more actions to keep it.
    heading: f32,
    travel: f32,
    commit: u32,
}

impl ScriptedBot {
    /// The veteran, the reference opponent.
    pub fn new(seed: u64) -> Self {
        Self::with_level(seed, VETERAN)
    }

    pub fn with_level(seed: u64, level: u8) -> Self {
        let level = level.min(MAX_LEVEL);
        ScriptedBot {
            level,
            style: Style::of_level(level),
            rng: Rng::new(seed),
            seed,
            wander: 0.0,
            reverse: 0,
            sweep: 1.0,
            side: 1.0,
            jink: 0,
            stuck: 0,
            dodge: 0,
            heading: 0.0,
            travel: 0.0,
            commit: 0,
        }
    }

    /// A pro bot with a custom style (for tuning).
    pub fn with_style(seed: u64, style: Style) -> Self {
        ScriptedBot {
            style: Some(style),
            ..Self::with_level(seed, ACE)
        }
    }

    /// Pro bots: exact beam geometry, turret and radar compensate for the hull's
    /// turn, fire only when the shell would pass close enough to the target, and
    /// strafe across the line of sight with random direction changes (jinks)
    /// that break the opponent's lead.
    fn act_pro(&mut self, o: &[f32], s: Style) -> Action {
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
                self.jink = s.jink_min + self.rng.below(s.jink_span as usize) as u32;
                self.stuck = 0;
                if incoming {
                    // Hold the new direction until the shell has gone by.
                    self.dodge = s.dodge_hold;
                }
            }
            // Strafe across the line of sight, backing off when close and
            // closing in when far.
            let tilt = if dist < s.min_range {
                -s.tilt
            } else if dist > s.max_range {
                s.tilt
            } else {
                0.0
            };
            let m = wrap_angle(vs_hull + self.side * (FRAC_PI_2 - tilt));
            let back_m = wrap_angle(m + PI);
            if m.abs() <= back_m.abs() {
                (1.0, m)
            } else {
                (-1.0, back_m)
            }
        } else if s.hunt {
            let goal = (known && o[27] * ARENA_DIAG > 60.0).then(|| o[25].atan2(o[26]));
            self.steer(o, goal)
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
        self.heading = wrap_angle(self.heading + hull_turn);

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

        // Fire when the shell would pass within the window around the target.
        let window = (s.window * TANK_RADIUS / dist.max(1.0)).atan();
        let clear_shot = o[32] * ARENA_DIAG > dist - 10.0;
        let last_shell = o[5] * MAX_AMMO as f32 <= 1.5;
        let fire = seen
            && o[6] <= 0.0
            && clear_shot
            && wrap_angle(aim_err - turret_turn).abs() < window
            && dist < RAY_RANGE * 2.0
            && (!last_shell || dist < s.last_shell_range);
        Action {
            throttle,
            hull,
            turret,
            radar,
            fire,
        }
    }

    /// Pick a travel direction among the eight wall rays: towards `goal`
    /// (hull-relative bearing) when there is one, otherwise onwards into open
    /// space, always away from walls. The direction is held for a while in the
    /// bot's own heading frame so it does not dither between rays.
    fn steer(&mut self, o: &[f32], goal: Option<f32>) -> (f32, f32) {
        self.commit = self.commit.saturating_sub(1);
        if o[0].abs() < 0.05 {
            self.stuck += 1;
        } else {
            self.stuck = 0;
        }
        let rel = wrap_angle(self.travel - self.heading);
        let ahead = ((rel / (PI / 4.0)).round() as i32).rem_euclid(N_RAYS as i32) as usize;
        if self.commit == 0 || o[8 + ahead] < 0.15 || self.stuck > 12 {
            let mut best = (f32::MIN, 0.0);
            for k in 0..N_RAYS {
                let a = wrap_angle(ray_angle(k));
                let free = o[8 + k];
                let toward = match goal {
                    Some(g) => wrap_angle(a - g).cos(),
                    // Keep going roughly the same way; a little noise breaks loops.
                    None => 0.5 * wrap_angle(a - rel).cos() + self.rng.range(0.0, 0.6),
                };
                let score =
                    toward + 1.2 * (free / 0.6).min(1.0) - if free < 0.15 { 3.0 } else { 0.0 };
                if score > best.0 {
                    best = (score, a);
                }
            }
            self.travel = wrap_angle(self.heading + best.1);
            self.commit = if goal.is_some() { 10 } else { 40 };
            self.stuck = 0;
        }
        let rel = wrap_angle(self.travel - self.heading);
        // Drive forwards or backwards, whichever needs less turning, and slow
        // down for sharp turns.
        let (dir, turn) = if rel.abs() <= FRAC_PI_2 {
            (1.0, rel)
        } else {
            (-1.0, wrap_angle(rel + PI))
        };
        (dir * (1.0 - turn.abs() / FRAC_PI_2).max(0.25), turn)
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
        self.heading = 0.0;
        self.travel = 0.0;
        self.commit = 0;
    }

    fn act(&mut self, o: &[f32]) -> Action {
        if let Some(s) = self.style {
            return self.act_pro(o, s);
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
