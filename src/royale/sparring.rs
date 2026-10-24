//! Native ports of official Tank Royale sample bots (SpinBot, Walls, TrackFire,
//! RamFire, Crazy), as fast in-process sparring partners for training under
//! Robocode rules. A small interpreter emulates the Bot API's blocking
//! commands (`forward(100)` runs over several turns, event handlers interrupt
//! it); the real bots still play through the server.

use super::battle::{Bot, Event, Intent};
use super::pilot::{Pilot, View};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Sparring {
    SpinBot,
    Walls,
    TrackFire,
    RamFire,
    Crazy,
}

impl Sparring {
    pub const ALL: [Sparring; 5] = [
        Sparring::SpinBot,
        Sparring::Walls,
        Sparring::TrackFire,
        Sparring::RamFire,
        Sparring::Crazy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Sparring::SpinBot => "SpinBot",
            Sparring::Walls => "Walls",
            Sparring::TrackFire => "TrackFire",
            Sparring::RamFire => "RamFire",
            Sparring::Crazy => "Crazy",
        }
    }

    pub fn pilot(self) -> SparringBot {
        SparringBot::new(self)
    }
}

/// One Bot API command. Blocking ones last until their remaining amount is 0
/// (or one turn for `Fire` and `Rescan`); the others take effect at once.
#[derive(Clone, Copy, Debug)]
enum Cmd {
    /// Blocking moves, in degrees (left = counter-clockwise) and pixels.
    Forward(f64),
    TurnLeft(f64),
    GunLeft(f64),
    Fire(f64),
    Rescan,
    /// Non-blocking.
    SetForward(f64),
    SetTurnLeft(f64),
    SetMaxSpeed(f64),
    /// Blocks until the body's turn is done.
    WaitTurn,
    /// Walls's `peek` flag.
    Peek(bool),
}

/// The Bot API's remaining-amount bookkeeping.
#[derive(Clone, Copy, Debug)]
struct Moves {
    distance: f64,
    turn: f64,
    gun: f64,
    max_speed: f64,
    fire: f64,
    rescan: bool,
}

impl Default for Moves {
    fn default() -> Self {
        Moves {
            distance: 0.0,
            turn: 0.0,
            gun: 0.0,
            max_speed: MAX_SPEED,
            fire: 0.0,
            rescan: false,
        }
    }
}

/// `IntentValidator.getNewTargetSpeed`: the speed to ask for to cover
/// `distance` without overshooting.
pub fn target_speed(speed: f64, distance: f64, max_speed: f64) -> f64 {
    if distance < 0.0 {
        return -target_speed(-speed, -distance, max_speed);
    }
    let target = if distance.is_infinite() {
        max_speed
    } else {
        max_speed.min(max_speed_for(distance))
    };
    let dec = DECELERATION.abs();
    if speed >= 0.0 {
        clamp(target, speed - dec, speed + ACCELERATION)
    } else {
        clamp(
            target,
            speed - ACCELERATION,
            speed + max_deceleration(-speed),
        )
    }
}

fn max_speed_for(distance: f64) -> f64 {
    let dec = DECELERATION.abs();
    let t = (((4.0 * 2.0 / dec) * distance + 1.0).sqrt() - 1.0) / 2.0;
    let t = t.ceil().max(1.0);
    if t.is_infinite() {
        return MAX_SPEED;
    }
    let dist = (t / 2.0) * (t - 1.0) * dec;
    (t - 1.0) * dec + (distance - dist) / t
}

fn max_deceleration(speed: f64) -> f64 {
    let dec = DECELERATION.abs();
    let t = speed / dec;
    t.min(1.0) * dec + (1.0 - t).max(0.0) * ACCELERATION
}

/// Distance covered while braking from `speed` to a stop.
fn stop_distance(speed: f64, max_speed: f64) -> f64 {
    let mut v = speed.abs();
    let mut d = 0.0;
    while v > 0.0 {
        v = target_speed(v, 0.0, max_speed);
        d += v;
    }
    d
}

fn near0(v: f64) -> bool {
    v.abs() < 1e-9
}

pub struct SparringBot {
    pub kind: Sparring,
    moves: Moves,
    /// The run() loop: commands from `program[loop_from..]` repeat.
    program: Vec<Cmd>,
    loop_from: usize,
    pc: usize,
    /// Commands of an event handler, run before the program resumes.
    handler: Vec<Cmd>,
    /// The blocking command being waited on, and whether it came from a handler.
    current: Option<Cmd>,
    /// A handler is running (its commands or its blocking command).
    handler_active: bool,
    /// After `rescan()`, a new scan interrupts the running handler.
    interruptible: bool,
    started: bool,
    /// Directions at the last tick, and the API's over-driving flag.
    prev_dir: f64,
    prev_gun: f64,
    overdriving: bool,
    // Bot-specific state.
    peek: bool,
    turn_direction: f64,
    moving_forward: bool,
}

impl SparringBot {
    pub fn new(kind: Sparring) -> SparringBot {
        let mut b = SparringBot {
            kind,
            moves: Moves::default(),
            program: Vec::new(),
            loop_from: 0,
            pc: 0,
            handler: Vec::new(),
            current: None,
            handler_active: false,
            interruptible: false,
            started: false,
            prev_dir: 0.0,
            prev_gun: 0.0,
            overdriving: false,
            peek: false,
            turn_direction: 1.0,
            moving_forward: true,
        };
        b.reset();
        b
    }

    /// Whether a blocking command has finished (checked from the next turn on:
    /// the API's `waitFor` always lets at least one turn pass).
    fn done(&self, c: Cmd, me: &Bot) -> bool {
        match c {
            Cmd::Forward(_) => near0(self.moves.distance) && near0(me.speed),
            Cmd::TurnLeft(_) | Cmd::WaitTurn => near0(self.moves.turn),
            Cmd::GunLeft(_) => near0(self.moves.gun),
            _ => true,
        }
    }

    /// Applies a command; returns true if it blocks.
    fn apply(&mut self, c: Cmd, me: &Bot) -> bool {
        let m = &mut self.moves;
        match c {
            Cmd::Forward(d) => {
                m.distance = d;
                true
            }
            Cmd::TurnLeft(a) => {
                m.turn = a;
                true
            }
            Cmd::GunLeft(a) => {
                m.gun = a;
                true
            }
            Cmd::Fire(p) => {
                // The API refuses to fire a hot gun or without the energy.
                if me.gun_heat <= 0.0 && me.energy >= p && p >= MIN_FIREPOWER {
                    m.fire = p;
                }
                true
            }
            Cmd::Rescan => {
                m.rescan = true;
                self.interruptible = true;
                true
            }
            Cmd::SetForward(d) => {
                m.distance = d;
                false
            }
            Cmd::SetTurnLeft(a) => {
                m.turn = a;
                false
            }
            Cmd::SetMaxSpeed(v) => {
                m.max_speed = v;
                false
            }
            Cmd::WaitTurn => true,
            Cmd::Peek(p) => {
                self.peek = p;
                false
            }
        }
    }

    fn handle(&mut self, v: &View) {
        let me = v.me;
        let busy = self.handler_active && !self.interruptible;
        for e in v.events {
            let cmds: Vec<Cmd> = match (self.kind, e) {
                (_, Event::ScannedBot { by, x, y, .. }) if *by == me.id && !busy => {
                    self.on_scanned(me, *x, *y)
                }
                (
                    _,
                    Event::BotHitBot {
                        bot,
                        x,
                        y,
                        energy,
                        rammed,
                        ..
                    },
                ) if *bot == me.id => self.on_hit_bot(me, *x, *y, *energy, *rammed),
                (Sparring::Crazy, Event::BotHitWall { victim }) if *victim == me.id => {
                    self.reverse();
                    Vec::new()
                }
                (Sparring::TrackFire, Event::WonRound) => vec![Cmd::TurnLeft(-36_000.0)],
                _ => Vec::new(),
            };
            if !cmds.is_empty() {
                self.handler = cmds;
                self.current = None;
                self.handler_active = true;
                self.interruptible = false;
            }
        }
    }

    fn reverse(&mut self) {
        self.moving_forward = !self.moving_forward;
        self.moves.distance = if self.moving_forward {
            40_000.0
        } else {
            -40_000.0
        };
    }

    fn on_scanned(&mut self, me: &Bot, x: f64, y: f64) -> Vec<Cmd> {
        match self.kind {
            Sparring::SpinBot => vec![Cmd::Fire(3.0)],
            Sparring::Walls => {
                let mut v = vec![Cmd::Fire(2.0)];
                if self.peek {
                    v.push(Cmd::Rescan);
                }
                v
            }
            Sparring::TrackFire => {
                let b = normalize_relative(angle_to(me.x, me.y, x, y) - me.gun_direction);
                let mut v = vec![Cmd::GunLeft(b)];
                if b.abs() <= 3.0 && me.gun_heat == 0.0 {
                    v.push(Cmd::Fire((3.0 - b.abs()).min(me.energy - 0.1)));
                }
                v.push(Cmd::Rescan);
                v
            }
            Sparring::RamFire => {
                let b = self.face(me, x, y);
                let d = ((x - me.x).powi(2) + (y - me.y).powi(2)).sqrt();
                vec![Cmd::TurnLeft(b), Cmd::Forward(d + 5.0), Cmd::Rescan]
            }
            Sparring::Crazy => vec![Cmd::Fire(1.0)],
        }
    }

    fn on_hit_bot(&mut self, me: &Bot, x: f64, y: f64, energy: f64, rammed: bool) -> Vec<Cmd> {
        let bearing = normalize_relative(angle_to(me.x, me.y, x, y) - me.direction);
        match self.kind {
            Sparring::SpinBot => {
                let mut v = Vec::new();
                if bearing > -10.0 && bearing < 10.0 {
                    v.push(Cmd::Fire(3.0));
                }
                if rammed {
                    v.push(Cmd::TurnLeft(-10.0));
                }
                v
            }
            Sparring::Walls => {
                if bearing > -90.0 && bearing < 90.0 {
                    vec![Cmd::Forward(-100.0)]
                } else {
                    vec![Cmd::Forward(100.0)]
                }
            }
            Sparring::RamFire => {
                let b = self.face(me, x, y);
                let p = if energy > 16.0 {
                    3.0
                } else if energy > 10.0 {
                    2.0
                } else if energy > 4.0 {
                    1.0
                } else if energy > 2.0 {
                    0.5
                } else if energy > 0.4 {
                    0.1
                } else {
                    0.0
                };
                let mut v = vec![Cmd::TurnLeft(b)];
                if p > 0.0 {
                    v.push(Cmd::Fire(p));
                }
                v.push(Cmd::Forward(40.0));
                v
            }
            Sparring::Crazy => {
                if rammed {
                    self.reverse();
                }
                Vec::new()
            }
            Sparring::TrackFire => Vec::new(),
        }
    }

    /// RamFire's `turnToFaceTarget`: the bearing to turn left by.
    fn face(&mut self, me: &Bot, x: f64, y: f64) -> f64 {
        let b = normalize_relative(angle_to(me.x, me.y, x, y) - me.direction);
        self.turn_direction = if b >= 0.0 { 1.0 } else { -1.0 };
        b
    }
}

impl Pilot for SparringBot {
    fn reset(&mut self) {
        self.moves = Moves::default();
        self.handler.clear();
        self.handler_active = false;
        self.interruptible = false;
        self.current = None;
        self.pc = 0;
        self.started = false;
        self.overdriving = false;
        self.peek = false;
        self.turn_direction = 1.0;
        self.moving_forward = true;
    }

    fn act(&mut self, v: &View) -> Intent {
        let me = v.me;
        if !self.started {
            // run(): the parts that depend on the start state.
            self.started = true;
            let (w, h) = (v.setup.arena_width as f64, v.setup.arena_height as f64);
            (self.program, self.loop_from) = match self.kind {
                Sparring::SpinBot => (
                    vec![
                        Cmd::SetTurnLeft(-10_000.0),
                        Cmd::SetMaxSpeed(5.0),
                        Cmd::Forward(10_000.0),
                    ],
                    0,
                ),
                Sparring::Walls => {
                    let m = w.max(h);
                    (
                        vec![
                            Cmd::Peek(false),
                            Cmd::TurnLeft(-(me.direction % 90.0)),
                            Cmd::Forward(m),
                            Cmd::Peek(true),
                            Cmd::GunLeft(90.0),
                            Cmd::TurnLeft(90.0),
                            Cmd::Peek(true),
                            Cmd::Forward(m),
                            Cmd::Peek(false),
                            Cmd::TurnLeft(90.0),
                        ],
                        6,
                    )
                }
                Sparring::TrackFire => (vec![Cmd::GunLeft(-10.0)], 0),
                Sparring::RamFire => (vec![Cmd::TurnLeft(-5.0)], 0),
                Sparring::Crazy => (
                    vec![
                        Cmd::SetForward(40_000.0),
                        Cmd::SetTurnLeft(90.0),
                        Cmd::WaitTurn,
                        Cmd::SetTurnLeft(-180.0),
                        Cmd::WaitTurn,
                        Cmd::SetTurnLeft(180.0),
                        Cmd::WaitTurn,
                    ],
                    0,
                ),
            };
        } else {
            // Turns count by how far the body and gun actually turned.
            let eat = |rem: &mut f64, delta: f64| {
                if rem.abs() <= delta.abs() {
                    *rem = 0.0;
                } else {
                    *rem -= delta;
                    if near0(*rem) {
                        *rem = 0.0;
                    }
                }
            };
            eat(
                &mut self.moves.turn,
                normalize_relative(me.direction - self.prev_dir),
            );
            eat(
                &mut self.moves.gun,
                normalize_relative(me.gun_direction - self.prev_gun),
            );
        }
        self.prev_dir = me.direction;
        self.prev_gun = me.gun_direction;
        // The API's own handlers: hitting a wall or being rammed ends a move.
        for e in v.events {
            match e {
                Event::BotHitWall { victim } if *victim == me.id => self.moves.distance = 0.0,
                Event::BotHitBot {
                    bot, rammed: true, ..
                } if *bot == me.id => self.moves.distance = 0.0,
                _ => {}
            }
        }
        self.moves.fire = 0.0;
        self.moves.rescan = false;
        self.handle(v);

        // Run commands until one blocks.
        for _ in 0..32 {
            if let Some(c) = self.current {
                if !self.done(c, me) {
                    break;
                }
                self.current = None;
            }
            let next = if !self.handler.is_empty() {
                Some(self.handler.remove(0))
            } else {
                self.handler_active = false;
                self.interruptible = false;
                if self.program.is_empty() {
                    None
                } else {
                    if self.pc >= self.program.len() {
                        self.pc = self.loop_from;
                    }
                    self.pc += 1;
                    Some(self.program[self.pc - 1])
                }
            };
            let Some(c) = next else { break };
            // RamFire's run(): `turnRight(5 * turnDirection)`.
            let c = match (self.kind, c) {
                (Sparring::RamFire, Cmd::TurnLeft(_)) if !self.handler_active => {
                    Cmd::TurnLeft(-5.0 * self.turn_direction)
                }
                _ => c,
            };
            if self.apply(c, me) {
                self.current = Some(c);
                break;
            }
        }

        // Nat Pavasant's movement update, as in the API: ask for the speed that
        // covers the distance, and count it as covered.
        let mut distance = self.moves.distance;
        let speed = target_speed(me.speed, distance, self.moves.max_speed);
        if near0(speed) && self.overdriving {
            distance = 0.0;
            self.overdriving = false;
        }
        if distance * speed >= 0.0 {
            self.overdriving = stop_distance(speed, self.moves.max_speed) > distance.abs();
        }
        self.moves.distance = distance - speed;
        let m = &self.moves;
        Intent {
            target_speed: speed,
            turn_rate: clamp(m.turn, -MAX_TURN_RATE, MAX_TURN_RATE),
            gun_turn_rate: clamp(m.gun, -MAX_GUN_TURN_RATE, MAX_GUN_TURN_RATE),
            radar_turn_rate: 0.0,
            firepower: m.fire,
            rescan: m.rescan,
            fire_assist: false,
            ..Intent::default()
        }
    }
}
