//! Our pilots (networks and scripted bots) inside a Tank Royale battle. They
//! see only what a Tank Royale bot sees (its own state, its own bullets and
//! its events) and get it as our usual 34 observations, with a few slots
//! taking the nearest Royale meaning (see `observe`).

use super::battle::{Bot, BulletState, Event, Intent};
use super::*;
use crate::bots::ScriptedBot;
use crate::nn::Brain;
use crate::sim::sensors::N_OBS;
use crate::sim::world::Controller;

/// What a bot receives with one tick.
pub struct View<'a> {
    pub setup: &'a Setup,
    pub turn: u32,
    pub me: &'a Bot,
    pub bullets: &'a [BulletState],
    pub events: &'a [Event],
}

/// A pilot running inside this program.
pub trait Pilot: Send {
    fn reset(&mut self);
    /// Intent for the next turn, from the tick just received.
    fn act(&mut self, v: &View) -> Intent;
}

pub enum Driver {
    Net(Brain),
    Scripted(ScriptedBot),
}

/// Turns our pilots' observations and outputs into Tank Royale terms.
pub struct Adapter {
    pub driver: Driver,
    last_seen: Option<(f64, f64)>,
    since_seen: u32,
    enemy_energy: Option<f64>,
    /// Turns since the enemy's energy dropped as if it fired, and the drop.
    fired: Option<(u32, f64)>,
}

/// Turns over which the "since seen" input saturates (5 s at 30 turns/s).
const SEEN_HORIZON: f64 = 150.0;
/// Turns over which the "time" input saturates.
const TIME_HORIZON: f64 = 3000.0;
const RAY_RANGE: f64 = 400.0;

impl Adapter {
    pub fn new(driver: Driver) -> Adapter {
        Adapter {
            driver,
            last_seen: None,
            since_seen: u32::MAX / 2,
            enemy_energy: None,
            fired: None,
        }
    }

    fn outputs(&mut self, o: &[f32; N_OBS]) -> [f32; 5] {
        match &mut self.driver {
            Driver::Net(b) => b.outputs(o),
            Driver::Scripted(s) => {
                let a = s.act(o);
                [
                    a.throttle,
                    a.hull,
                    a.turret,
                    a.radar,
                    if a.fire { 0.65 } else { -1.0 },
                ]
            }
        }
    }

    /// Our 34 observations. Slots that have no Royale equivalent change meaning:
    /// 5 = own energy (was ammo), 6 = gun heat (was cooldown), 7 = enemy energy
    /// (was ammo regen), 28-30 = the enemy fired, judged by its energy drop
    /// (was a shell in the radar beam: Royale bots never see enemy bullets).
    pub fn observe(&mut self, v: &View) -> [f32; N_OBS] {
        let me = v.me;
        let (w, h) = (v.setup.arena_width as f64, v.setup.arena_height as f64);
        let diag = (w * w + h * h).sqrt();
        let (body, gun, radar) = (
            me.direction.to_radians(),
            me.gun_direction.to_radians(),
            me.radar_direction.to_radians(),
        );
        let mut o = [0.0f32; N_OBS];
        o[0] = (me.speed / MAX_SPEED) as f32;
        o[1] = (gun - body).sin() as f32;
        o[2] = (gun - body).cos() as f32;
        o[3] = (radar - gun).sin() as f32;
        o[4] = (radar - gun).cos() as f32;
        o[5] = (me.energy / INITIAL_ENERGY).clamp(0.0, 1.0) as f32;
        o[6] = (me.gun_heat / 1.6).min(1.0) as f32;
        let ray = |a: f64| wall_distance(me.x, me.y, a, w, h);
        for k in 0..8 {
            o[8 + k] =
                (ray(body + k as f64 * std::f64::consts::FRAC_PI_4) / RAY_RANGE).min(1.0) as f32;
        }
        self.since_seen = self.since_seen.saturating_add(1);
        if let Some((age, _)) = &mut self.fired {
            *age += 1;
        }
        // My own hits explain part of an energy drop.
        let my_hits: f64 = v
            .events
            .iter()
            .filter_map(|e| match e {
                Event::BulletHitBot { bullet, damage, .. } if bullet.owner == me.id => {
                    Some(*damage)
                }
                _ => None,
            })
            .sum();
        let scan = v.events.iter().find_map(|e| match e {
            Event::ScannedBot {
                by,
                energy,
                x,
                y,
                direction,
                speed,
                ..
            } if *by == me.id => Some((*energy, *x, *y, *direction, *speed)),
            _ => None,
        });
        if let Some((energy, x, y, dir, speed)) = scan {
            let (dx, dy) = (x - me.x, y - me.y);
            let bearing = dy.atan2(dx);
            o[16] = 1.0;
            o[17] = ((dx * dx + dy * dy).sqrt() / diag) as f32;
            o[18] = (normalize_relative(bearing.to_degrees() - me.radar_direction)
                / MAX_RADAR_TURN_RATE)
                .clamp(-1.0, 1.0) as f32;
            let rel = dir.to_radians() - bearing;
            o[19] = rel.sin() as f32;
            o[20] = rel.cos() as f32;
            o[21] = (speed / MAX_SPEED) as f32;
            if let Some(prev) = self.enemy_energy {
                let drop = prev - energy - my_hits;
                if (MIN_FIREPOWER - 1e-9..=MAX_FIREPOWER + 1e-9).contains(&drop) {
                    self.fired = Some((0, drop));
                }
            }
            self.enemy_energy = Some(energy);
            self.last_seen = Some((x, y));
            self.since_seen = 0;
        }
        o[7] = self
            .enemy_energy
            .map_or(1.0, |e| (e / INITIAL_ENERGY).clamp(0.0, 1.0)) as f32;
        o[22] = (self.since_seen as f64 / SEEN_HORIZON).min(1.0) as f32;
        if let Some((x, y)) = self.last_seen {
            let (dx, dy) = (x - me.x, y - me.y);
            let b = dy.atan2(dx);
            o[23] = (b - gun).sin() as f32;
            o[24] = (b - gun).cos() as f32;
            o[25] = (b - body).sin() as f32;
            o[26] = (b - body).cos() as f32;
            o[27] = ((dx * dx + dy * dy).sqrt() / diag) as f32;
        }
        if let Some((age, power)) = self.fired {
            // The bullet's travelled distance; forget it once it must have passed.
            let travelled = age as f64 * bullet_speed(power);
            if travelled < diag {
                o[28] = 1.0;
                o[29] = (travelled / diag) as f32;
                o[30] = (power / MAX_FIREPOWER) as f32;
            }
        }
        o[31] = (ray(radar) / diag) as f32;
        o[32] = (ray(gun) / diag) as f32;
        o[33] = (v.turn as f64 / TIME_HORIZON).min(1.0) as f32;
        o
    }
}

impl Pilot for Adapter {
    fn reset(&mut self) {
        match &mut self.driver {
            Driver::Net(b) => b.reset(),
            Driver::Scripted(s) => s.reset(),
        }
        self.last_seen = None;
        self.since_seen = u32::MAX / 2;
        self.enemy_energy = None;
        self.fired = None;
    }

    fn act(&mut self, v: &View) -> Intent {
        let o = self.observe(v);
        let out = self.outputs(&o);
        let c = |x: f32| x.clamp(-1.0, 1.0) as f64;
        Intent {
            target_speed: MAX_SPEED * c(out[0]),
            turn_rate: MAX_TURN_RATE * c(out[1]),
            gun_turn_rate: MAX_GUN_TURN_RATE * c(out[2]),
            radar_turn_rate: MAX_RADAR_TURN_RATE * c(out[3]),
            firepower: if out[4] > 0.0 {
                MIN_FIREPOWER + (MAX_FIREPOWER - MIN_FIREPOWER) * c(out[4])
            } else {
                0.0
            },
            fire_assist: false,
            ..Intent::default()
        }
    }
}

/// Distance from (x, y) along angle `a` (radians) to the arena's edge.
pub fn wall_distance(x: f64, y: f64, a: f64, w: f64, h: f64) -> f64 {
    let (c, s) = (a.cos(), a.sin());
    let mut t = f64::INFINITY;
    if c > 1e-9 {
        t = t.min((w - x) / c);
    } else if c < -1e-9 {
        t = t.min(-x / c);
    }
    if s > 1e-9 {
        t = t.min((h - y) / s);
    } else if s < -1e-9 {
        t = t.min(-y / s);
    }
    t.max(0.0)
}
