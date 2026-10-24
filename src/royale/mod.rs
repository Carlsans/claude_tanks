//! Robocode Tank Royale rules, ported from the official server (Apache-2.0,
//! github.com/robocode-dev/tank-royale v1.4.0) so that Tank Royale bots, and
//! classic Robocode robots through the official API bridge, play here as they
//! do there. Units are those of Robocode: one turn per tick, distances in
//! pixels, angles in degrees with x to the right, y up and 0° pointing east,
//! counter-clockwise.

pub mod battle;
pub mod duel;
pub mod pilot;
pub mod protocol;
pub mod server;
pub mod sparring;

pub const BOT_RADIUS: f64 = 18.0;
pub const BOT_DIAMETER: f64 = 36.0;
pub const RADAR_RADIUS: f64 = 1200.0;
pub const MAX_TURN_RATE: f64 = 10.0;
pub const MAX_GUN_TURN_RATE: f64 = 20.0;
pub const MAX_RADAR_TURN_RATE: f64 = 45.0;
pub const MAX_SPEED: f64 = 8.0;
pub const MIN_FIREPOWER: f64 = 0.1;
pub const MAX_FIREPOWER: f64 = 3.0;
pub const ACCELERATION: f64 = 1.0;
pub const DECELERATION: f64 = -2.0;
pub const RAM_DAMAGE: f64 = 0.6;
pub const BULLET_HIT_ENERGY_GAIN: f64 = 3.0;
pub const INITIAL_ENERGY: f64 = 100.0;
pub const INITIAL_GUN_HEAT: f64 = 3.0;
pub const INACTIVITY_DAMAGE: f64 = 0.1;
pub const MAX_BULLET_SPEED: f64 = 20.0 - 3.0 * MIN_FIREPOWER;

/// Battle settings, with the official "classic" defaults.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Setup {
    pub arena_width: u32,
    pub arena_height: u32,
    pub rounds: u32,
    pub gun_cooling_rate: f64,
    pub max_inactivity_turns: u32,
    /// Time a remote bot has to answer a tick, in microseconds.
    pub turn_timeout_us: u32,
    /// Time a remote bot has to get ready for the battle, in microseconds.
    pub ready_timeout_us: u32,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            arena_width: 800,
            arena_height: 600,
            rounds: 10,
            gun_cooling_rate: 0.1,
            max_inactivity_turns: 450,
            turn_timeout_us: 30_000,
            ready_timeout_us: 10_000_000,
        }
    }
}

pub fn clamp(v: f64, min: f64, max: f64) -> f64 {
    min.max(max.min(v))
}

/// Kotlin's `Double.sign`: 0 for zero (Rust's `signum` gives 1).
fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// Speed after one turn toward `target`: accelerating 1 per turn, braking 2,
/// exactly as the official server computes it (a target of 0 stops at once).
pub fn new_speed(speed: f64, target: f64) -> f64 {
    if speed < 0.0 {
        return -new_speed(-speed, -target);
    }
    if speed > 0.0 {
        if sign(speed) == sign(target) {
            let diff = target - speed;
            if diff >= 0.0 {
                (speed + diff.min(ACCELERATION)).min(MAX_SPEED)
            } else {
                (speed + diff.max(DECELERATION)).max(-MAX_SPEED)
            }
        } else if target == 0.0 {
            0.0
        } else {
            // Crossing zero: brake for part of the turn, accelerate for the rest.
            let braking = speed / -DECELERATION;
            (1.0 - braking) * -ACCELERATION
        }
    } else {
        let diff = target - speed;
        let a = diff.abs().min(ACCELERATION);
        if diff >= 0.0 {
            (speed + a).min(MAX_SPEED)
        } else {
            (speed - a).max(-MAX_SPEED)
        }
    }
}

pub fn max_turn_rate(speed: f64) -> f64 {
    MAX_TURN_RATE - 0.75 * clamp(speed, -MAX_SPEED, MAX_SPEED).abs()
}

pub fn wall_damage(speed: f64) -> f64 {
    (clamp(speed, -MAX_SPEED, MAX_SPEED).abs() / 2.0 - 1.0).max(0.0)
}

pub fn clamp_firepower(p: f64) -> f64 {
    clamp(p, MIN_FIREPOWER, MAX_FIREPOWER)
}

pub fn bullet_speed(power: f64) -> f64 {
    20.0 - 3.0 * clamp_firepower(power)
}

pub fn bullet_damage(power: f64) -> f64 {
    let mut d = 4.0 * clamp_firepower(power);
    if power > 1.0 {
        d += 2.0 * (power - 1.0);
    }
    d
}

pub fn gun_heat(power: f64) -> f64 {
    1.0 + clamp_firepower(power) / 5.0
}

/// Angle in [0, 360).
pub fn normalize_absolute(a: f64) -> f64 {
    let n = a % 360.0;
    if n >= 0.0 { n } else { n + 360.0 }
}

/// Angle in [-180, 180).
pub fn normalize_relative(a: f64) -> f64 {
    let n = a % 360.0;
    if n >= 0.0 {
        if n < 180.0 { n } else { n - 360.0 }
    } else if n >= -180.0 {
        n
    } else {
        n + 360.0
    }
}

/// Direction from one point to another, in [0, 360).
pub fn angle_to(x: f64, y: f64, tx: f64, ty: f64) -> f64 {
    normalize_absolute((ty - y).atan2(tx - x).to_degrees())
}

fn near_zero(v: f64) -> bool {
    v.abs() < 1e-6
}

type Point = (f64, f64);

fn point_in_circle(p: Point, c: Point, r: f64) -> bool {
    let (dx, dy) = (p.0 - c.0, p.1 - c.1);
    dx * dx + dy * dy <= r * r
}

fn point_on_line(p: Point, a: Point, b: Point) -> bool {
    let (dxl, dyl) = (b.0 - a.0, b.1 - a.1);
    let cross = (p.0 - a.0) * dyl - (p.1 - a.1) * dxl;
    if !near_zero(cross) {
        return false;
    }
    let between = |v: f64, lo: f64, hi: f64| v >= lo && v <= hi;
    if dxl.abs() >= dyl.abs() {
        if dxl > 0.0 {
            between(p.0, a.0, b.0)
        } else {
            between(p.0, b.0, a.0)
        }
    } else if dyl > 0.0 {
        between(p.1, a.1, b.1)
    } else {
        between(p.1, b.1, a.1)
    }
}

/// Whether the segment a-b touches the circle.
pub fn line_hits_circle(a: Point, b: Point, c: Point, r: f64) -> bool {
    if point_in_circle(a, c, r) || point_in_circle(b, c, r) {
        return true;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = ((c.0 - a.0) * dx + (c.1 - a.1) * dy) / len2;
    let closest = (a.0 + t * dx, a.1 + t * dy);
    point_on_line(closest, a, b) && point_in_circle(closest, c, r)
}

/// Segment intersection (Franklin Antonio, Graphics Gems III), as the server does it.
pub fn lines_intersect(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    let (ax, ay) = (a2.0 - a1.0, a2.1 - a1.1);
    let (bx, by) = (b1.0 - b2.0, b1.1 - b2.1);
    let (cx, cy) = (a1.0 - b1.0, a1.1 - b1.1);
    let alpha = by * cx - bx * cy;
    let den = ay * bx - ax * by;
    if (den > 0.0 && (alpha < 0.0 || alpha > den)) || (den < 0.0 && (alpha > 0.0 || alpha < den)) {
        return false;
    }
    let beta = ax * cy - ay * cx;
    if (den > 0.0 && (beta < 0.0 || beta > den)) || (den < 0.0 && (beta > 0.0 || beta < den)) {
        return false;
    }
    if den == 0.0 {
        // Parallel: overlap only if collinear.
        let collinear = a1.0 * (a2.1 - b1.1) + a2.0 * (b1.1 - a1.1) + b1.0 * (a1.1 - a2.1);
        if collinear == 0.0 {
            let within = |v: f64, p: f64, q: f64| (v >= p && v <= q) || (v <= p && v >= q);
            let xs =
                within(a1.0, b1.0, b2.0) || within(a2.0, b1.0, b2.0) || within(b1.0, a1.0, a2.0);
            let ys =
                within(a1.1, b1.1, b2.1) || within(a2.1, b1.1, b2.1) || within(b1.1, a1.1, a2.1);
            return xs && ys;
        }
        return false;
    }
    true
}

fn clockwise(v1: Point, v2: Point) -> bool {
    v1.0 * v2.1 <= v1.1 * v2.0
}

/// Whether a circle touches the radar sector swept from `start` to `end` degrees.
pub fn circle_hits_sector(
    c: Point,
    r: f64,
    center: Point,
    radius: f64,
    start: f64,
    end: f64,
) -> bool {
    let (vx, vy) = (c.0 - center.0, c.1 - center.1);
    let max = radius + r;
    if vx * vx + vy * vy > max * max {
        return false;
    }
    let (s, e) = (start.to_radians(), end.to_radians());
    let p0 = (s.cos() * radius, s.sin() * radius);
    let p1 = (e.cos() * radius, e.sin() * radius);
    if !clockwise(p0, (vx, vy)) && clockwise(p1, (vx, vy)) {
        return true;
    }
    line_hits_circle(center, (center.0 + p0.0, center.1 + p0.1), c, r)
        || line_hits_circle(center, (center.0 + p1.0, center.1 + p1.1), c, r)
}
