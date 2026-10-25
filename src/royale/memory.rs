//! What a pilot learns about its opponent over a battle and keeps from one
//! round to the next, as Robocode robots do in static variables: who wins the
//! rounds, how often each side hits, and where each side's bullets find the
//! other (as guess factors). `Arch::GruMem` networks read it as extra inputs,
//! so that training can teach them to tell weak and evasive opponents apart.

use super::battle::Event;
use super::pilot::View;
use super::*;

/// Inputs it adds to the 34 observations.
pub const N_MEM: usize = 10;
/// Guess-factor bins, from -1 to 1.
const BINS: usize = 31;
/// Each new guess factor first scales the old ones by this, so the last few
/// dozen count most (robots that adapt change where they go).
const FADE: f32 = 0.97;
/// Turns after the last scan during which the enemy's position is trusted.
const FRESH: u32 = 10;

/// What the pilot knows about the enemy this turn (see `Adapter::observe`).
#[derive(Clone, Copy, Debug)]
pub struct Sighting {
    pub x: f64,
    pub y: f64,
    /// Heading (degrees) and speed at the last scan.
    pub direction: f64,
    pub speed: f64,
    /// Turns since the last scan (0: scanned this turn).
    pub age: u32,
}

/// A bullet in flight, fired at turn `turn` from (x, y).
#[derive(Clone, Copy, Debug)]
struct Wave {
    x: f64,
    y: f64,
    turn: u32,
    speed: f64,
    /// Bearing from the shooter to the target when fired (radians).
    bearing: f64,
    /// +1 when the target was moving counter-clockwise around the shooter.
    lateral: f64,
}

impl Wave {
    fn escape_angle(&self) -> f64 {
        (MAX_SPEED / self.speed).asin()
    }

    /// Guess factor of a direction `a` (radians) from the shooter: where the
    /// target went within its reach, -1 to 1 in its direction of motion.
    fn guess_factor(&self, a: f64) -> f64 {
        let off = normalize_relative((a - self.bearing).to_degrees()).to_radians();
        (off / self.escape_angle() * self.lateral).clamp(-1.0, 1.0)
    }
}

fn bin(gf: f64) -> usize {
    (((gf + 1.0) / 2.0 * (BINS - 1) as f64).round() as usize).min(BINS - 1)
}

fn add(hist: &mut [f32; BINS], gf: f64) {
    hist.iter_mut().for_each(|v| *v *= FADE);
    let c = bin(gf) as f32;
    for (k, v) in hist.iter_mut().enumerate() {
        let d = k as f32 - c;
        *v += 1.0 / (1.0 + d * d);
    }
}

/// +1 when something at (x, y) moving along `direction` (degrees) at `speed`
/// goes counter-clockwise around (ox, oy); `last` when it barely moves.
fn lateral(ox: f64, oy: f64, x: f64, y: f64, direction: f64, speed: f64, last: f64) -> f64 {
    let bearing = (y - oy).atan2(x - ox);
    let v = speed * (direction.to_radians() - bearing).sin();
    if v.abs() < 0.1 { last } else { v.signum() }
}

#[derive(Clone, Debug)]
pub struct BattleMemory {
    pub rounds: u32,
    pub won: u32,
    won_this_round: bool,
    playing: bool,
    /// Our bullets that hit the enemy or a wall, and those that hit it.
    pub resolved: u32,
    pub hits: u32,
    /// Enemy shots seen (energy drops) and its bullets that hit us.
    pub enemy_shots: u32,
    pub hit_by: u32,
    pub dealt: f64,
    pub taken: f64,
    enemy_speed: f64,
    scans: u32,
    waves: Vec<Wave>,
    enemy_waves: Vec<Wave>,
    /// Where the enemy was when our waves passed it, and where its bullets
    /// that hit us were aimed, in guess factors.
    gf: [f32; BINS],
    gf_waves: u32,
    enemy_gf_sum: f64,
    enemy_gf_hits: u32,
    enemy_lateral: f64,
    my_lateral: f64,
    last_power: f64,
}

impl Default for BattleMemory {
    fn default() -> Self {
        BattleMemory {
            rounds: 0,
            won: 0,
            won_this_round: false,
            playing: false,
            resolved: 0,
            hits: 0,
            enemy_shots: 0,
            hit_by: 0,
            dealt: 0.0,
            taken: 0.0,
            enemy_speed: 0.0,
            scans: 0,
            waves: Vec::new(),
            enemy_waves: Vec::new(),
            gf: [0.0; BINS],
            gf_waves: 0,
            enemy_gf_sum: 0.0,
            enemy_gf_hits: 0,
            enemy_lateral: 1.0,
            my_lateral: 1.0,
            last_power: 2.0,
        }
    }
}

impl BattleMemory {
    /// A new round: counts the one that ended. Bullets in flight are gone.
    pub fn new_round(&mut self) {
        if self.playing {
            self.rounds += 1;
            self.won += u32::from(self.won_this_round);
        }
        self.playing = false;
        self.won_this_round = false;
        self.waves.clear();
        self.enemy_waves.clear();
    }

    /// Share of our resolved bullets that hit, with a weak prior.
    pub fn hit_rate(&self) -> f64 {
        (self.hits as f64 + 0.5) / (self.resolved as f64 + 3.0)
    }

    /// Share of the enemy's shots that hit us, with a weak prior.
    pub fn enemy_hit_rate(&self) -> f64 {
        (self.hit_by as f64 + 0.5) / (self.enemy_shots.max(self.hit_by) as f64 + 3.0)
    }

    /// Takes in one turn. `enemy`: where it was last seen; `enemy_fired`: the
    /// power of a shot its energy drop revealed this turn.
    pub fn update(&mut self, v: &View, enemy: Option<Sighting>, enemy_fired: Option<f64>) {
        let me = v.me;
        self.playing = true;
        let fresh = enemy.filter(|e| e.age <= FRESH);
        if let Some(e) = enemy.filter(|e| e.age == 0) {
            self.enemy_speed += e.speed.abs();
            self.scans += 1;
            self.enemy_lateral = lateral(
                me.x,
                me.y,
                e.x,
                e.y,
                e.direction,
                e.speed,
                self.enemy_lateral,
            );
        }
        if let Some(e) = fresh {
            self.my_lateral = lateral(
                e.x,
                e.y,
                me.x,
                me.y,
                me.direction,
                me.speed,
                self.my_lateral,
            );
        }
        for ev in v.events {
            match ev {
                Event::WonRound => self.won_this_round = true,
                Event::BulletFired { bullet } if bullet.owner == me.id => {
                    self.last_power = bullet.power;
                    if let Some(e) = fresh {
                        self.waves.push(Wave {
                            x: bullet.x,
                            y: bullet.y,
                            turn: v.turn,
                            speed: bullet_speed(bullet.power),
                            bearing: (e.y - bullet.y).atan2(e.x - bullet.x),
                            lateral: self.enemy_lateral,
                        });
                    }
                }
                Event::BulletHitBot {
                    bullet,
                    victim,
                    damage,
                    ..
                } => {
                    if bullet.owner == me.id {
                        self.resolved += 1;
                        self.hits += 1;
                        self.dealt += damage;
                    } else if *victim == me.id {
                        self.hit_by += 1;
                        self.taken += damage;
                        self.enemy_hit(v.turn, bullet.x, bullet.y, bullet.direction);
                    }
                }
                Event::BulletHitWall { bullet } if bullet.owner == me.id => self.resolved += 1,
                _ => {}
            }
        }
        if let Some(e) = fresh
            && let Some(power) = enemy_fired
        {
            self.enemy_shots += 1;
            // It fired last turn, from about where it was last seen.
            self.enemy_waves.push(Wave {
                x: e.x,
                y: e.y,
                turn: v.turn.saturating_sub(1),
                speed: bullet_speed(power),
                bearing: (me.y - e.y).atan2(me.x - e.x),
                lateral: self.my_lateral,
            });
        }
        // Our waves that reached the enemy tell where it went.
        let diag = (v.setup.arena_width as f64).hypot(v.setup.arena_height as f64);
        let turn = v.turn;
        let mut passed = Vec::new();
        self.waves.retain(|w| {
            let r = turn.saturating_sub(w.turn) as f64 * w.speed;
            match enemy.filter(|e| e.age == 0) {
                Some(e) if r >= (e.x - w.x).hypot(e.y - w.y) => {
                    passed.push(w.guess_factor((e.y - w.y).atan2(e.x - w.x)));
                    false
                }
                _ => r < diag,
            }
        });
        for gf in passed {
            add(&mut self.gf, gf);
            self.gf_waves += 1;
        }
        self.enemy_waves
            .retain(|w| (turn.saturating_sub(w.turn) as f64) * w.speed < diag);
    }

    /// A bullet that hit us at (x, y) heading `direction`: which of the
    /// enemy's waves it was, and where in our reach it was aimed.
    fn enemy_hit(&mut self, turn: u32, x: f64, y: f64, direction: f64) {
        let best = self.enemy_waves.iter().min_by(|a, b| {
            let miss = |w: &Wave| {
                let r = turn.saturating_sub(w.turn) as f64 * w.speed;
                ((x - w.x).hypot(y - w.y) - r).abs()
            };
            miss(a).total_cmp(&miss(b))
        });
        if let Some(w) = best {
            self.enemy_gf_sum += w.guess_factor(direction.to_radians());
            self.enemy_gf_hits += 1;
        }
    }

    /// The most visited guess factor so far, if any wave passed the enemy.
    pub fn best_guess_factor(&self) -> Option<f64> {
        (self.gf_waves > 0).then(|| {
            let k = (0..BINS)
                .max_by(|&a, &b| self.gf[a].total_cmp(&self.gf[b]))
                .unwrap_or(BINS / 2);
            k as f64 / (BINS - 1) as f64 * 2.0 - 1.0
        })
    }

    /// The network inputs, each in [-1, 1]. `enemy`: as for `update`.
    pub fn inputs(&self, v: &View, enemy: Option<Sighting>) -> [f32; N_MEM] {
        let me = v.me;
        let mut m = [0.0f32; N_MEM];
        m[0] = ((2 * self.won) as f64 - self.rounds as f64) as f32 / (self.rounds + 2) as f32;
        m[1] = self.hit_rate() as f32;
        m[2] = self.enemy_hit_rate().min(1.0) as f32;
        m[3] = ((self.dealt - self.taken) / (self.dealt + self.taken + 50.0)) as f32;
        m[4] = self.resolved as f32 / (self.resolved + 20) as f32;
        // Aim: where the guess factors say to shoot, relative to the gun.
        if let Some(e) = enemy {
            let bearing = (e.y - me.y).atan2(e.x - me.x);
            let reach = (MAX_SPEED / bullet_speed(self.last_power)).asin();
            let lead = self.best_guess_factor().unwrap_or(0.0) * reach * self.enemy_lateral;
            m[5] = (bearing + lead - me.gun_direction.to_radians()).sin() as f32;
        }
        m[6] = self.gf_waves as f32 / (self.gf_waves + 10) as f32;
        m[7] = (self.enemy_gf_sum / (self.enemy_gf_hits as f64 + 2.0)) as f32;
        m[8] = self.enemy_gf_hits as f32 / (self.enemy_gf_hits + 5) as f32;
        m[9] = ((self.enemy_speed + 4.0) / (self.scans as f64 + 1.0) / MAX_SPEED) as f32;
        m
    }
}
