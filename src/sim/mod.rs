//! Headless, deterministic tank-duel simulation.

pub mod sensors;
pub mod terrain;
pub mod world;

use std::f32::consts::PI;

pub use terrain::Terrain;
pub use world::{Action, MatchResult, World};

/// Which game a pilot plays: ours, or Robocode Tank Royale's (`crate::royale`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Rules {
    #[default]
    Tanks,
    Robocode,
}

impl Rules {
    pub fn name(self) -> &'static str {
        match self {
            Rules::Tanks => "Claude Tanks",
            Rules::Robocode => "Robocode",
        }
    }
}

/// Simulation tick (seconds).
pub const DT: f32 = 1.0 / 60.0;
/// Controllers are queried every `ACTION_REPEAT` ticks (30 Hz).
pub const ACTION_REPEAT: u32 = 2;
pub const ARENA: f32 = 1000.0;
pub const GRID: usize = 10;
pub const CELL: f32 = ARENA / GRID as f32;
pub const WALL_THICK: f32 = 8.0;

pub const TANK_RADIUS: f32 = 18.0;
/// Hull rotation speed (rad/s). Turret is 2x (relative to hull), radar 3x (relative to turret).
pub const BASE_ROT: f32 = PI / 2.0;
pub const HULL_ROT: f32 = BASE_ROT;
pub const TURRET_ROT: f32 = 2.0 * BASE_ROT;
pub const RADAR_ROT: f32 = 3.0 * BASE_ROT;
/// Engine acceleration (u/s^2) and linear drag (1/s); terminal speed = ACCEL / DRAG.
pub const ACCEL: f32 = 240.0;
pub const DRAG: f32 = 1.6;
pub const MAX_SPEED: f32 = ACCEL / DRAG;

pub const SHELL_SPEED: f32 = 450.0;
pub const SHELL_RADIUS: f32 = 3.0;
pub const MAX_AMMO: u32 = 3;
pub const FIRE_COOLDOWN: f32 = 0.5;
pub const AMMO_REGEN: f32 = 5.0;
pub const MATCH_TIME: f32 = 60.0;

/// Half-width of the radar beam (rad): a 10 degree cone.
pub const RADAR_HALF_ANGLE: f32 = 5.0 * PI / 180.0;
pub const RAY_RANGE: f32 = 400.0;
pub const ARENA_DIAG: f32 = ARENA * std::f32::consts::SQRT_2;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }
    pub fn from_angle(a: f32) -> Self {
        Vec2::new(a.cos(), a.sin())
    }
    pub fn dot(self, o: Vec2) -> f32 {
        self.x * o.x + self.y * o.y
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn angle(self) -> f32 {
        self.y.atan2(self.x)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f32) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

/// Wrap an angle to (-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = (a + PI).rem_euclid(2.0 * PI) - PI;
    if a <= -PI {
        a += 2.0 * PI;
    }
    a
}
