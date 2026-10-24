//! One battle under Tank Royale rules: rounds of turns, processed in the same
//! order as the official server (`TurnProcessor`), with the same events.

use super::*;
use crate::rng::Rng;

/// What a bot asks for this turn (the server's `BotIntent` with defaults applied).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    pub target_speed: f64,
    pub turn_rate: f64,
    pub gun_turn_rate: f64,
    pub radar_turn_rate: f64,
    pub firepower: f64,
    pub adjust_gun_for_body_turn: bool,
    pub adjust_radar_for_body_turn: bool,
    pub adjust_radar_for_gun_turn: bool,
    pub rescan: bool,
    pub fire_assist: bool,
}

impl Default for Intent {
    fn default() -> Self {
        Intent {
            target_speed: 0.0,
            turn_rate: 0.0,
            gun_turn_rate: 0.0,
            radar_turn_rate: 0.0,
            firepower: 0.0,
            adjust_gun_for_body_turn: false,
            adjust_radar_for_body_turn: false,
            adjust_radar_for_gun_turn: false,
            rescan: false,
            fire_assist: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bot {
    /// 1-based, in participant order.
    pub id: u32,
    pub energy: f64,
    pub x: f64,
    pub y: f64,
    pub direction: f64,
    pub gun_direction: f64,
    pub radar_direction: f64,
    /// Angle the radar swept this turn (signed).
    pub radar_sweep: f64,
    pub speed: f64,
    pub turn_rate: f64,
    pub gun_turn_rate: f64,
    pub radar_turn_rate: f64,
    pub gun_heat: f64,
    /// Standing intent: the last one received this round.
    pub intent: Option<Intent>,
}

impl Bot {
    pub fn alive(&self) -> bool {
        self.energy >= 0.0
    }
    /// Out of energy: alive but cannot move or fire.
    pub fn disabled(&self) -> bool {
        self.alive() && near_zero(self.energy)
    }
    pub fn enabled(&self) -> bool {
        self.alive() && !near_zero(self.energy)
    }
    fn pos(&self) -> Point {
        (self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Bullet {
    pub id: u32,
    pub owner: u32,
    pub power: f64,
    pub direction: f64,
    pub start: (f64, f64),
    /// Turns since firing.
    pub tick: u32,
}

impl Bullet {
    pub fn speed(&self) -> f64 {
        bullet_speed(self.power)
    }
    fn at(&self, tick: u32) -> Point {
        let a = self.direction.to_radians();
        let d = self.speed() * tick as f64;
        (self.start.0 + a.cos() * d, self.start.1 + a.sin() * d)
    }
    pub fn position(&self) -> Point {
        self.at(self.tick)
    }
    pub fn state(&self) -> BulletState {
        let (x, y) = self.position();
        BulletState {
            id: self.id,
            owner: self.owner,
            power: self.power,
            x,
            y,
            direction: normalize_absolute(self.direction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletState {
    pub id: u32,
    pub owner: u32,
    pub power: f64,
    pub x: f64,
    pub y: f64,
    pub direction: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    BotDeath {
        victim: u32,
    },
    BotHitBot {
        bot: u32,
        victim: u32,
        energy: f64,
        x: f64,
        y: f64,
        rammed: bool,
    },
    BotHitWall {
        victim: u32,
    },
    BulletFired {
        bullet: BulletState,
    },
    BulletHitBot {
        bullet: BulletState,
        victim: u32,
        damage: f64,
        energy: f64,
    },
    BulletHitBullet {
        bullet: BulletState,
        hit_bullet: BulletState,
    },
    BulletHitWall {
        bullet: BulletState,
    },
    ScannedBot {
        by: u32,
        scanned: u32,
        energy: f64,
        x: f64,
        y: f64,
        direction: f64,
        speed: f64,
    },
    WonRound,
}

/// Score of one participant (one round, or summed over the battle).
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub bullet_damage: f64,
    pub bullet_kill_bonus: f64,
    pub ram_damage: f64,
    pub ram_kill_bonus: f64,
    pub survival: f64,
    pub last_survivor_bonus: f64,
    pub first_places: u32,
    pub second_places: u32,
    pub third_places: u32,
}

impl Score {
    pub fn total(&self) -> f64 {
        self.bullet_damage
            + self.bullet_kill_bonus
            + self.ram_damage
            + self.ram_kill_bonus
            + self.survival
            + self.last_survivor_bonus
    }
}

/// Damage bookkeeping of one round (the server's `ScoreTracker`).
#[derive(Clone, Debug, Default)]
struct Tracker {
    /// [offender][victim] bullet damage and ram hits.
    bullet: Vec<Vec<f64>>,
    rams: Vec<Vec<u32>>,
    bullet_kills: Vec<Vec<bool>>,
    ram_kills: Vec<Vec<bool>>,
    survival: Vec<u32>,
    last_survivor: Vec<u32>,
    alive: Vec<bool>,
    last_awarded: bool,
}

impl Tracker {
    fn new(n: usize) -> Tracker {
        Tracker {
            bullet: vec![vec![0.0; n]; n],
            rams: vec![vec![0; n]; n],
            bullet_kills: vec![vec![false; n]; n],
            ram_kills: vec![vec![false; n]; n],
            survival: vec![0; n],
            last_survivor: vec![0; n],
            alive: vec![true; n],
            last_awarded: false,
        }
    }

    fn damage_to(&self, o: usize, v: usize) -> f64 {
        self.bullet[o][v] + self.rams[o][v] as f64 * RAM_DAMAGE
    }

    fn score(&self, o: usize) -> Score {
        let n = self.alive.len();
        let sum = |kills: &Vec<Vec<bool>>| -> f64 {
            (0..n)
                .filter(|&v| kills[o][v])
                .map(|v| self.damage_to(o, v))
                .sum()
        };
        Score {
            bullet_damage: self.bullet[o].iter().sum(),
            bullet_kill_bonus: 0.2 * sum(&self.bullet_kills),
            ram_damage: 2.0 * self.rams[o].iter().sum::<u32>() as f64 * RAM_DAMAGE,
            ram_kill_bonus: 0.3 * sum(&self.ram_kills),
            survival: 50.0 * self.survival[o] as f64,
            last_survivor_bonus: 10.0 * self.last_survivor[o] as f64,
            ..Score::default()
        }
    }

    fn deaths(&mut self, dead: &[usize]) {
        let newly: Vec<usize> = dead.iter().copied().filter(|&d| self.alive[d]).collect();
        if newly.is_empty() {
            return;
        }
        for &d in &newly {
            self.alive[d] = false;
        }
        for _ in &newly {
            for s in 0..self.alive.len() {
                if self.alive[s] {
                    self.survival[s] += 1;
                }
            }
        }
        let alive: Vec<usize> = (0..self.alive.len()).filter(|&i| self.alive[i]).collect();
        if !self.last_awarded && alive.len() == 1 {
            self.last_awarded = true;
            self.last_survivor[alive[0]] += self.alive.len() as u32 - 1;
        }
    }
}

/// Everything the server sends out after a turn.
#[derive(Clone, Debug)]
pub struct Turn {
    pub round: u32,
    pub turn: u32,
    /// Bots at the end of the turn, including those that died in it.
    pub bots: Vec<Bot>,
    pub bullets: Vec<Bullet>,
    /// Events per participant (index = id - 1).
    pub events: Vec<Vec<Event>>,
    pub round_ended: bool,
    pub game_ended: bool,
    /// Round winners (rank 1), when the round ended.
    pub winners: Vec<u32>,
}

impl Turn {
    pub fn bot(&self, id: u32) -> Option<&Bot> {
        self.bots.iter().find(|b| b.id == id)
    }
}

pub struct Battle {
    pub setup: Setup,
    pub participants: usize,
    rng: Rng,
    round_number: u32,
    turn_number: u32,
    round_ended: bool,
    game_ended: bool,
    bots: Vec<Bot>,
    bullets: Vec<Bullet>,
    next_bullet_id: u32,
    inactivity: u32,
    tracker: Tracker,
    /// Summed scores and placings over the finished rounds.
    pub results: Vec<Score>,
    /// The previous turn of this round (for first wall hits and fire assist).
    last: Option<Turn>,
}

impl Battle {
    pub fn new(setup: Setup, participants: usize, seed: u64) -> Battle {
        Battle {
            setup,
            participants,
            rng: Rng::new(seed),
            round_number: 0,
            turn_number: 0,
            round_ended: false,
            game_ended: false,
            bots: Vec::new(),
            bullets: Vec::new(),
            next_bullet_id: 0,
            inactivity: 0,
            tracker: Tracker::new(participants),
            results: vec![Score::default(); participants],
            last: None,
        }
    }

    pub fn game_ended(&self) -> bool {
        self.game_ended
    }

    /// Bots alive now.
    pub fn bots(&self) -> &[Bot] {
        &self.bots
    }

    /// Plays one turn. `intents` are those received since the last turn
    /// (bots that sent none keep their standing intent).
    pub fn step(&mut self, intents: &[(u32, Intent)]) -> Turn {
        assert!(!self.game_ended, "battle is over");
        for &(id, intent) in intents {
            if let Some(b) = self.bots.iter_mut().find(|b| b.id == id) {
                b.intent = Some(intent);
            }
        }
        if self.round_ended || self.round_number == 0 {
            self.next_round();
        }
        self.turn_number += 1;
        let t = self.process_turn();
        self.last = Some(t.clone());
        t
    }

    fn next_round(&mut self) {
        self.round_number += 1;
        self.turn_number = 0;
        self.round_ended = false;
        self.next_bullet_id = 0;
        self.bullets.clear();
        self.inactivity = 0;
        self.tracker = Tracker::new(self.participants);
        self.last = None;
        // Random spawn cells of 50 x 50, one bot per cell.
        let (w, h) = (
            self.setup.arena_width as usize,
            self.setup.arena_height as usize,
        );
        let (gw, gh) = (w / 50, h / 50);
        let (cw, ch) = (w / gw, h / gh);
        let mut free: Vec<usize> = (0..gw * gh).collect();
        self.bots = (1..=self.participants as u32)
            .map(|id| {
                let cell = free.swap_remove(self.rng.below(free.len()));
                let (row, col) = (cell / gw, cell % gw);
                let x = (col * cw) as f64
                    + BOT_RADIUS
                    + self.rng.f32() as f64 * (cw as f64 - BOT_DIAMETER);
                let y = (row * ch) as f64
                    + BOT_RADIUS
                    + self.rng.f32() as f64 * (ch as f64 - BOT_DIAMETER);
                let d = self.rng.f32() as f64 * 360.0;
                Bot {
                    id,
                    energy: INITIAL_ENERGY,
                    x,
                    y,
                    direction: d,
                    gun_direction: d,
                    radar_direction: d,
                    radar_sweep: 0.0,
                    speed: 0.0,
                    turn_rate: 0.0,
                    gun_turn_rate: 0.0,
                    radar_turn_rate: 0.0,
                    gun_heat: INITIAL_GUN_HEAT,
                    intent: None,
                }
            })
            .collect();
    }

    fn idx(id: u32) -> usize {
        id as usize - 1
    }

    fn process_turn(&mut self) -> Turn {
        let n = self.participants;
        let (w, h) = (
            self.setup.arena_width as f64,
            self.setup.arena_height as f64,
        );
        let mut events: Vec<Vec<Event>> = vec![Vec::new(); n];
        let prev: Vec<Bot> = self.bots.clone();
        let last_events = |id: u32| -> &[Event] {
            self.last
                .as_ref()
                .map_or(&[][..], |t| t.events[Self::idx(id)].as_slice())
        };

        // Guns cool down, or fire when cold.
        let mut new_bullets = Vec::new();
        for (k, b) in self.bots.iter_mut().enumerate() {
            if b.gun_heat == 0.0 && b.enabled() {
                let Some(intent) = b.intent else { continue };
                let firepower = intent.firepower;
                if firepower < MIN_FIREPOWER || b.energy <= firepower {
                    continue;
                }
                let power = firepower.min(MAX_FIREPOWER);
                let mut dir = b.gun_direction;
                let p = &prev[k];
                if intent.fire_assist
                    && b.gun_direction == b.radar_direction
                    && p.gun_direction == p.radar_direction
                    && let Some(Event::ScannedBot { x, y, .. }) = last_events(b.id)
                        .iter()
                        .find(|e| matches!(e, Event::ScannedBot { .. }))
                {
                    dir = angle_to(b.x, b.y, *x, *y);
                }
                self.next_bullet_id += 1;
                let bullet = Bullet {
                    id: self.next_bullet_id,
                    owner: b.id,
                    power,
                    direction: dir,
                    start: (b.x, b.y),
                    tick: 0,
                };
                b.gun_heat = gun_heat(power);
                b.energy -= firepower;
                events[Self::idx(b.id)].push(Event::BulletFired {
                    bullet: bullet.state(),
                });
                new_bullets.push(bullet);
            } else {
                b.gun_heat = (b.gun_heat - self.setup.gun_cooling_rate).max(0.0);
            }
        }
        self.bullets.extend(new_bullets);

        // Bots move and turn.
        for b in self.bots.iter_mut().filter(|b| b.enabled()) {
            let Some(i) = b.intent else { continue };
            b.speed = new_speed(b.speed, i.target_speed);
            let a = b.direction.to_radians();
            b.x += a.cos() * b.speed;
            b.y += a.sin() * b.speed;
            let mt = max_turn_rate(b.speed);
            let turn = clamp(i.turn_rate, -mt, mt);
            let gun = clamp(i.gun_turn_rate, -MAX_GUN_TURN_RATE, MAX_GUN_TURN_RATE);
            let radar = clamp(i.radar_turn_rate, -MAX_RADAR_TURN_RATE, MAX_RADAR_TURN_RATE);
            b.turn_rate = turn;
            b.gun_turn_rate = gun;
            b.radar_turn_rate = radar;
            let mut gun_adj = turn + gun;
            if i.adjust_gun_for_body_turn {
                gun_adj -= turn;
            }
            let mut radar_adj = gun_adj + radar;
            if i.adjust_radar_for_gun_turn {
                radar_adj -= gun;
                if i.adjust_gun_for_body_turn {
                    // Original Robocode compatibility.
                    radar_adj += turn;
                }
            }
            if i.adjust_radar_for_body_turn {
                radar_adj -= turn;
            }
            b.direction = normalize_absolute(b.direction + turn);
            b.gun_direction = normalize_absolute(b.gun_direction + gun_adj);
            b.radar_direction = normalize_absolute(b.radar_direction + radar_adj);
            b.radar_sweep = radar_adj;
        }

        // Walls (not on a round's first turn).
        if self.last.is_some() {
            for (k, b) in self.bots.iter_mut().enumerate() {
                let (nx, ny) = constrain(w, h, prev[k].pos(), b.pos());
                if nx != b.x || ny != b.y {
                    let first = !last_events(b.id)
                        .iter()
                        .any(|e| matches!(e, Event::BotHitWall { .. }));
                    b.x = nx;
                    b.y = ny;
                    if first {
                        events[Self::idx(b.id)].push(Event::BotHitWall { victim: b.id });
                        b.energy -= wall_damage(b.speed);
                    }
                    b.speed = 0.0;
                }
            }
        }

        // Bots colliding with bots: detect every pair, then apply.
        let mut collisions = Vec::new();
        for i in 0..self.bots.len() {
            for j in i + 1..self.bots.len() {
                let (a, b) = (&self.bots[i], &self.bots[j]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                if dx.abs() > BOT_DIAMETER || dy.abs() > BOT_DIAMETER {
                    continue;
                }
                if dx * dx + dy * dy <= BOT_DIAMETER * BOT_DIAMETER {
                    let (pa, pb) = match &self.last {
                        Some(t) if self.turn_number > 1 => (
                            t.bot(a.id).map_or(a.pos(), |o| o.pos()),
                            t.bot(b.id).map_or(b.pos(), |o| o.pos()),
                        ),
                        _ => (
                            a.pos(),
                            (
                                BOT_RADIUS + self.rng.f32() as f64 * (w - BOT_DIAMETER),
                                BOT_RADIUS + self.rng.f32() as f64 * (h - BOT_DIAMETER),
                            ),
                        ),
                    };
                    collisions.push((i, j, ramming(a, b), ramming(b, a), pa, pb));
                }
            }
        }
        for (i, j, ram_i, ram_j, pi, pj) in collisions {
            let (was_i, was_j) = (self.bots[i].alive(), self.bots[j].alive());
            self.bots[i].energy -= RAM_DAMAGE;
            self.bots[j].energy -= RAM_DAMAGE;
            let (ki, kj) = (
                !self.bots[i].alive() && was_i,
                !self.bots[j].alive() && was_j,
            );
            let (oi, oj) = (Self::idx(self.bots[i].id), Self::idx(self.bots[j].id));
            if ram_i {
                self.tracker.rams[oi][oj] += 1;
                self.tracker.ram_kills[oi][oj] |= kj;
            }
            if ram_j {
                self.tracker.rams[oj][oi] += 1;
                self.tracker.ram_kills[oj][oi] |= ki;
            }
            (self.bots[i].x, self.bots[i].y) = pi;
            (self.bots[j].x, self.bots[j].y) = pj;
            if ram_i {
                self.bots[i].speed = 0.0;
            }
            if ram_j {
                self.bots[j].speed = 0.0;
            }
            let (a, b) = (&self.bots[i], &self.bots[j]);
            events[oi].push(Event::BotHitBot {
                bot: a.id,
                victim: b.id,
                energy: b.energy,
                x: b.x,
                y: b.y,
                rammed: ram_i,
            });
            events[oj].push(Event::BotHitBot {
                bot: b.id,
                victim: a.id,
                energy: a.energy,
                x: a.x,
                y: a.y,
                rammed: ram_j,
            });
        }
        for k in 0..self.bots.len() {
            let (x, y) = constrain(w, h, prev[k].pos(), self.bots[k].pos());
            self.bots[k].x = x;
            self.bots[k].y = y;
        }

        // Radar scans: the arc swept this turn, by bots that moved or rescan.
        for i in 0..self.bots.len() {
            let s = &self.bots[i];
            let p = &prev[i];
            let active = s.intent.is_some_and(|it| it.rescan)
                || s.x != p.x
                || s.y != p.y
                || s.direction != p.direction
                || s.gun_direction != p.gun_direction
                || s.radar_direction != p.radar_direction;
            if !active {
                continue;
            }
            let (start, end) = if s.radar_sweep >= 0.0 {
                (
                    normalize_absolute(s.radar_direction - s.radar_sweep.abs()),
                    s.radar_direction,
                )
            } else {
                (
                    s.radar_direction,
                    normalize_absolute(s.radar_direction + s.radar_sweep.abs()),
                )
            };
            for j in 0..self.bots.len() {
                if i == j {
                    continue;
                }
                let t = &self.bots[j];
                let seen = if start == end {
                    let a = start.to_radians();
                    let end_pt = (s.x + a.cos() * RADAR_RADIUS, s.y + a.sin() * RADAR_RADIUS);
                    line_hits_circle(s.pos(), end_pt, t.pos(), BOT_RADIUS)
                } else {
                    circle_hits_sector(t.pos(), BOT_RADIUS, s.pos(), RADAR_RADIUS, start, end)
                };
                if seen {
                    events[Self::idx(s.id)].push(Event::ScannedBot {
                        by: s.id,
                        scanned: t.id,
                        energy: t.energy,
                        x: t.x,
                        y: t.y,
                        direction: t.direction,
                        speed: t.speed,
                    });
                }
            }
        }

        // Bullets advance; those outside the arena hit the wall.
        for b in &mut self.bullets {
            b.tick += 1;
        }
        self.bullets.retain(|b| {
            let (x, y) = b.position();
            let out = x <= 0.0 || y <= 0.0 || x >= w || y >= h;
            if out {
                events[Self::idx(b.owner)].push(Event::BulletHitWall { bullet: b.state() });
            }
            !out
        });

        // Bullets against bullets and bots, over the segment of this turn.
        let lines: Vec<(Point, Point)> = self
            .bullets
            .iter()
            .map(|b| (b.position(), b.at(b.tick + 1)))
            .collect();
        let reach = 2.0 * MAX_BULLET_SPEED;
        let mut bullet_pairs = Vec::new();
        for i in 0..lines.len() {
            for j in i + 1..lines.len() {
                let (e1, e2) = (lines[i].1, lines[j].1);
                let (dx, dy) = (e2.0 - e1.0, e2.1 - e1.1);
                if dx.abs() <= reach
                    && dy.abs() <= reach
                    && dx * dx + dy * dy <= reach * reach
                    && lines_intersect(lines[i].0, lines[i].1, lines[j].0, lines[j].1)
                {
                    bullet_pairs.push((self.bullets[i], self.bullets[j]));
                }
            }
        }
        let mut bot_hits = Vec::new();
        for (k, b) in self.bullets.iter().enumerate() {
            for v in &self.bots {
                if v.id != b.owner && line_hits_circle(lines[k].0, lines[k].1, v.pos(), BOT_RADIUS)
                {
                    bot_hits.push((*b, v.id));
                }
            }
        }
        let mut removed: Vec<u32> = Vec::new();
        for (b1, b2) in bullet_pairs {
            events[Self::idx(b1.owner)].push(Event::BulletHitBullet {
                bullet: b1.state(),
                hit_bullet: b2.state(),
            });
            events[Self::idx(b2.owner)].push(Event::BulletHitBullet {
                bullet: b2.state(),
                hit_bullet: b1.state(),
            });
            removed.extend([b1.id, b2.id]);
        }
        let any_hit = !bot_hits.is_empty();
        for (b, victim) in bot_hits {
            let damage = bullet_damage(b.power);
            let Some(v) = self.bots.iter_mut().find(|x| x.id == victim) else {
                continue;
            };
            let after = v.energy - damage;
            let killed = v.alive() && after < 0.0;
            let capped = if v.energy < damage {
                v.energy.max(0.0)
            } else {
                damage
            };
            v.energy -= damage;
            if let Some(s) = self.bots.iter_mut().find(|x| x.id == b.owner) {
                s.energy += BULLET_HIT_ENERGY_GAIN * b.power;
            }
            let (o, vi) = (Self::idx(b.owner), Self::idx(victim));
            self.tracker.bullet[o][vi] += capped;
            self.tracker.bullet_kills[o][vi] |= killed;
            let e = Event::BulletHitBot {
                bullet: b.state(),
                victim,
                damage,
                energy: after,
            };
            events[o].push(e.clone());
            events[vi].push(e);
            removed.push(b.id);
        }
        self.bullets.retain(|b| !removed.contains(&b.id));

        // Inactivity: when nobody has been hit for too long, everybody loses energy.
        if any_hit {
            self.inactivity = 0;
        }
        let inactive = self.inactivity > self.setup.max_inactivity_turns;
        self.inactivity += 1;
        if inactive {
            for b in &mut self.bots {
                b.energy -= INACTIVITY_DAMAGE;
            }
        }
        for b in &mut self.bots {
            if b.disabled()
                && let Some(i) = &mut b.intent
            {
                i.target_speed = 0.0;
                i.turn_rate = 0.0;
                i.gun_turn_rate = 0.0;
                i.radar_turn_rate = 0.0;
                i.firepower = 0.0;
            }
        }
        let dead: Vec<usize> = self
            .bots
            .iter()
            .filter(|b| !b.alive())
            .map(|b| Self::idx(b.id))
            .collect();
        self.tracker.deaths(&dead);

        // Round over: one bot left, or only disabled bots and no bullets.
        let alive: Vec<&Bot> = self.bots.iter().filter(|b| b.alive()).collect();
        let over =
            alive.len() <= 1 || (self.bullets.is_empty() && alive.iter().all(|b| b.disabled()));
        let mut winners = Vec::new();
        if over {
            self.round_ended = true;
            self.game_ended = self.round_number >= self.setup.rounds;
            let scores: Vec<Score> = (0..n).map(|o| self.tracker.score(o)).collect();
            let best = scores.iter().map(|s| s.total()).fold(f64::MIN, f64::max);
            for o in 0..n {
                let rank = 1 + scores
                    .iter()
                    .filter(|s| s.total() > scores[o].total())
                    .count();
                let r = &mut self.results[o];
                let s = scores[o];
                r.bullet_damage += s.bullet_damage;
                r.bullet_kill_bonus += s.bullet_kill_bonus;
                r.ram_damage += s.ram_damage;
                r.ram_kill_bonus += s.ram_kill_bonus;
                r.survival += s.survival;
                r.last_survivor_bonus += s.last_survivor_bonus;
                match rank {
                    1 => r.first_places += 1,
                    2 => r.second_places += 1,
                    3 => r.third_places += 1,
                    _ => {}
                }
                if scores[o].total() == best {
                    winners.push(o as u32 + 1);
                }
            }
            for &wid in &winners {
                events[Self::idx(wid)].push(Event::WonRound);
            }
        }

        let snapshot = self.bots.clone();
        for &d in &dead {
            for b in &snapshot {
                events[Self::idx(b.id)].push(Event::BotDeath {
                    victim: d as u32 + 1,
                });
            }
        }
        self.bots.retain(|b| b.alive());
        Turn {
            round: self.round_number,
            turn: self.turn_number,
            bots: snapshot,
            bullets: self.bullets.clone(),
            events,
            round_ended: self.round_ended,
            game_ended: self.game_ended,
            winners,
        }
    }
}

/// Keeps a bot inside a w x h arena, sliding back along its path.
fn constrain(w: f64, h: f64, old: Point, new: Point) -> Point {
    let (mut nx, mut ny) = new;
    let (x, y) = new;
    if x - BOT_RADIUS < 0.0 || x + BOT_RADIUS > w {
        nx = if x - BOT_RADIUS < 0.0 {
            BOT_RADIUS
        } else {
            w - BOT_RADIUS
        };
        let dx = x - old.0;
        if dx != 0.0 {
            ny = old.1 + (y - old.1) * (nx - old.0) / dx;
        }
    }
    if y - BOT_RADIUS < 0.0 || y + BOT_RADIUS > h {
        ny = if y - BOT_RADIUS < 0.0 {
            BOT_RADIUS
        } else {
            h - BOT_RADIUS
        };
        let dy = y - old.1;
        if dy != 0.0 {
            nx = old.0 + (x - old.0) * (ny - old.1) / dy;
        }
    }
    (nx, ny)
}

/// Whether `bot` drives into `victim` (moving toward it).
fn ramming(bot: &Bot, victim: &Bot) -> bool {
    let bearing = normalize_relative(angle_to(bot.x, bot.y, victim.x, victim.y) - bot.direction);
    (bot.speed > 0.0 && bearing > -90.0 && bearing < 90.0)
        || (bot.speed < 0.0 && !(-90.0..=90.0).contains(&bearing))
}
