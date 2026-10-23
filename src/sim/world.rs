//! Tanks, shells, and the fixed-step match update.

use super::*;
use crate::sim::sensors;

/// Controls for one tank; every field is in [-1, 1].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Action {
    pub throttle: f32,
    pub hull: f32,
    pub turret: f32,
    pub radar: f32,
    pub fire: bool,
}

impl Action {
    pub const OUTPUTS: usize = 5;

    pub fn from_outputs(o: &[f32]) -> Action {
        Action {
            throttle: o[0].clamp(-1.0, 1.0),
            hull: o[1].clamp(-1.0, 1.0),
            turret: o[2].clamp(-1.0, 1.0),
            radar: o[3].clamp(-1.0, 1.0),
            fire: o[4] > 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TankStats {
    pub shots: u32,
    pub hits: u32,
    /// Integral over time of max(0, cos(turret error))^4 (true bearing, walls ignored).
    pub aim: f32,
    /// Seconds the enemy was inside the radar beam.
    pub radar_time: f32,
    pub distance: f32,
    /// Sum over finished shots of closeness (1 = hit, 0 = never within NEAR_MISS of the enemy).
    pub near: f32,
}

/// Distance under which a passing shell still earns some aiming credit.
pub const NEAR_MISS: f32 = 150.0;

#[derive(Clone, Debug)]
pub struct Tank {
    pub pos: Vec2,
    pub heading: f32,
    /// Turret angle relative to hull.
    pub turret: f32,
    /// Radar angle relative to turret.
    pub radar: f32,
    pub speed: f32,
    pub ammo: u32,
    pub cooldown: f32,
    pub regen: f32,
    pub alive: bool,
    /// Enemy currently inside the radar beam.
    pub seen: Option<sensors::Contact>,
    /// Last position the enemy was seen at, and seconds since.
    pub last_seen: Option<Vec2>,
    pub since_seen: f32,
    pub last_action: Action,
    pub stats: TankStats,
}

impl Tank {
    pub fn new(pos: Vec2, heading: f32) -> Tank {
        Tank {
            pos,
            heading,
            turret: 0.0,
            radar: 0.0,
            speed: 0.0,
            ammo: MAX_AMMO,
            cooldown: 0.0,
            regen: 0.0,
            alive: true,
            seen: None,
            last_seen: None,
            since_seen: MATCH_TIME,
            last_action: Action::default(),
            stats: TankStats::default(),
        }
    }
    pub fn turret_abs(&self) -> f32 {
        wrap_angle(self.heading + self.turret)
    }
    pub fn radar_abs(&self) -> f32 {
        wrap_angle(self.heading + self.turret + self.radar)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Shell {
    pub pos: Vec2,
    pub vel: Vec2,
    pub owner: usize,
    /// Closest distance to the enemy so far.
    pub closest: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum Event {
    Fire { tank: usize, pos: Vec2 },
    WallHit { pos: Vec2 },
    TankHit { tank: usize, pos: Vec2 },
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct MatchResult {
    /// Index of the winning tank, `None` for a draw.
    pub winner: Option<usize>,
    pub ticks: u32,
    pub stats: [TankStats; 2],
}

#[derive(Clone, Debug)]
pub struct World {
    pub terrain: Terrain,
    pub tanks: [Tank; 2],
    pub shells: Vec<Shell>,
    pub tick: u32,
    pub result: Option<MatchResult>,
    /// Events produced by the last step (for display effects).
    pub events: Vec<Event>,
}

impl World {
    /// New match; `swap` exchanges the two spawn points.
    pub fn new(terrain: Terrain, swap: bool) -> World {
        let (a, b) = if swap { (1, 0) } else { (0, 1) };
        let tanks = [
            Tank::new(terrain.spawns[a].0, terrain.spawns[a].1),
            Tank::new(terrain.spawns[b].0, terrain.spawns[b].1),
        ];
        let mut w = World {
            terrain,
            tanks,
            shells: Vec::new(),
            tick: 0,
            result: None,
            events: Vec::new(),
        };
        w.update_radar();
        w
    }

    pub fn time(&self) -> f32 {
        self.tick as f32 * DT
    }

    pub fn done(&self) -> bool {
        self.result.is_some()
    }

    pub fn observe(&self, i: usize) -> [f32; sensors::N_OBS] {
        sensors::observe(self, i)
    }

    pub fn step(&mut self, actions: [Action; 2]) {
        if self.done() {
            return;
        }
        self.events.clear();
        for (i, act) in actions.iter().enumerate() {
            if self.tanks[i].alive {
                self.step_tank(i, act);
            }
        }
        self.collide_tanks();
        self.step_shells();
        self.update_radar();
        self.tick += 1;

        let alive = [self.tanks[0].alive, self.tanks[1].alive];
        let winner = match alive {
            [true, false] => Some(Some(0)),
            [false, true] => Some(Some(1)),
            [false, false] => Some(None),
            _ if self.time() >= MATCH_TIME => Some(None),
            _ => None,
        };
        if let Some(winner) = winner {
            self.result = Some(MatchResult {
                winner,
                ticks: self.tick,
                stats: [self.tanks[0].stats, self.tanks[1].stats],
            });
        }
    }

    fn step_tank(&mut self, i: usize, act: &Action) {
        let terrain = &self.terrain;
        let t = &mut self.tanks[i];
        let a = Action::from_outputs(&[
            act.throttle,
            act.hull,
            act.turret,
            act.radar,
            if act.fire { 1.0 } else { -1.0 },
        ]);
        t.last_action = a;
        t.heading = wrap_angle(t.heading + a.hull * HULL_ROT * DT);
        t.turret = wrap_angle(t.turret + a.turret * TURRET_ROT * DT);
        t.radar = wrap_angle(t.radar + a.radar * RADAR_ROT * DT);

        t.speed += (a.throttle * ACCEL - DRAG * t.speed) * DT;
        t.speed = t.speed.clamp(-MAX_SPEED, MAX_SPEED);
        let dir = Vec2::from_angle(t.heading);
        let before = t.pos;
        let mut p = t.pos + dir * (t.speed * DT);
        let moved = p;
        let mut hit = false;
        for w in &terrain.walls {
            hit |= w.push_out(&mut p, TANK_RADIUS);
        }
        if hit {
            // Lose the velocity component going into the wall.
            let push = p - moved;
            let l = push.len();
            if l > 1e-6 {
                let n = push * (1.0 / l);
                t.speed *= 1.0 - dir.dot(n).abs();
            }
        }
        t.stats.distance += (p - before).len();
        t.pos = p;

        // Gun.
        t.cooldown = (t.cooldown - DT).max(0.0);
        if t.ammo < MAX_AMMO {
            t.regen += DT;
            if t.regen >= AMMO_REGEN {
                t.ammo += 1;
                t.regen = if t.ammo < MAX_AMMO {
                    t.regen - AMMO_REGEN
                } else {
                    0.0
                };
            }
        } else {
            t.regen = 0.0;
        }
        if a.fire && t.ammo > 0 && t.cooldown <= 0.0 {
            let d = Vec2::from_angle(t.turret_abs());
            let pos = t.pos + d * (TANK_RADIUS + SHELL_RADIUS + 2.0);
            t.ammo -= 1;
            t.cooldown = FIRE_COOLDOWN;
            t.stats.shots += 1;
            self.shells.push(Shell {
                pos,
                vel: d * SHELL_SPEED,
                owner: i,
                closest: f32::MAX,
            });
            self.events.push(Event::Fire { tank: i, pos });
        }
    }

    fn collide_tanks(&mut self) {
        let [a, b] = &mut self.tanks;
        if !a.alive || !b.alive {
            return;
        }
        let d = b.pos - a.pos;
        let dist = d.len();
        let min = 2.0 * TANK_RADIUS;
        if dist < min && dist > 1e-6 {
            let push = d * ((min - dist) / dist * 0.5);
            a.pos = a.pos - push;
            b.pos = b.pos + push;
            a.speed *= 0.5;
            b.speed *= 0.5;
        }
    }

    fn step_shells(&mut self) {
        let mut kills = [false; 2];
        let mut i = 0;
        while i < self.shells.len() {
            let s = self.shells[i];
            let step = SHELL_SPEED * DT;
            let dir = s.vel * (1.0 / SHELL_SPEED);
            let enemy = &self.tanks[1 - s.owner];
            let closest = s.closest.min(segment_dist(s.pos, dir, step, enemy.pos));
            let wall_t = self.terrain.ray(s.pos, dir, step);
            let mut tank_hit: Option<(usize, f32)> = None;
            for (k, t) in self.tanks.iter().enumerate() {
                if k == s.owner || !t.alive {
                    continue;
                }
                if let Some(th) = ray_circle(s.pos, dir, t.pos, TANK_RADIUS + SHELL_RADIUS)
                    && th <= step
                    && th <= wall_t
                {
                    tank_hit = Some((k, th));
                }
            }
            if let Some((k, th)) = tank_hit {
                kills[k] = true;
                self.tanks[s.owner].stats.hits += 1;
                self.tanks[s.owner].stats.near += 1.0;
                self.events.push(Event::TankHit {
                    tank: k,
                    pos: s.pos + dir * th,
                });
                self.shells.swap_remove(i);
            } else if wall_t < step {
                self.tanks[s.owner].stats.near += (1.0 - closest / NEAR_MISS).max(0.0);
                self.events.push(Event::WallHit {
                    pos: s.pos + dir * wall_t,
                });
                self.shells.swap_remove(i);
            } else {
                self.shells[i].pos = s.pos + s.vel * DT;
                self.shells[i].closest = closest;
                i += 1;
            }
        }
        for (k, killed) in kills.into_iter().enumerate() {
            if killed {
                self.tanks[k].alive = false;
                self.tanks[k].speed = 0.0;
            }
        }
    }

    fn update_radar(&mut self) {
        for i in 0..2 {
            let contact = sensors::radar_contact(self, i);
            let t = &mut self.tanks[i];
            t.seen = contact;
            if let Some(c) = contact {
                t.last_seen = Some(c.pos);
                t.since_seen = 0.0;
                t.stats.radar_time += DT;
            } else {
                t.since_seen += DT;
            }
        }
        // Training-only signal: how well the gun points at the enemy (walls ignored).
        if self.tanks[0].alive && self.tanks[1].alive {
            for i in 0..2 {
                let bearing = (self.tanks[1 - i].pos - self.tanks[i].pos).angle();
                let t = &mut self.tanks[i];
                let c = wrap_angle(bearing - t.turret_abs()).cos().max(0.0);
                t.stats.aim += c.powi(4) * DT;
            }
        }
    }
}

/// Distance from point `c` to the segment o + dir * [0, len].
fn segment_dist(o: Vec2, dir: Vec2, len: f32, c: Vec2) -> f32 {
    let t = (c - o).dot(dir).clamp(0.0, len);
    (o + dir * t - c).len()
}

/// Distance along unit `dir` from `o` to a circle, if the ray (t >= 0) hits it.
pub fn ray_circle(o: Vec2, dir: Vec2, c: Vec2, r: f32) -> Option<f32> {
    let oc = o - c;
    let b = oc.dot(dir);
    let cc = oc.dot(oc) - r * r;
    if cc <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t >= 0.0).then_some(t)
}

/// Anything that can drive a tank from observations.
pub trait Controller {
    fn reset(&mut self) {}
    fn act(&mut self, obs: &[f32]) -> Action;
}

/// Play a full match between two controllers, headless.
pub fn run_match(
    terrain: Terrain,
    swap: bool,
    a: &mut dyn Controller,
    b: &mut dyn Controller,
) -> MatchResult {
    let mut w = World::new(terrain, swap);
    a.reset();
    b.reset();
    let mut acts = [Action::default(); 2];
    while !w.done() {
        if w.tick.is_multiple_of(ACTION_REPEAT) {
            acts = [a.act(&w.observe(0)), b.act(&w.observe(1))];
        }
        w.step(acts);
    }
    w.result.unwrap()
}
