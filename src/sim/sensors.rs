//! What a tank perceives. The network only ever sees `observe()`'s vector.

use super::*;
use crate::sim::world::World;

pub const N_RAYS: usize = 8;
pub const N_OBS: usize = 34;

/// Enemy currently inside the radar beam.
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub pos: Vec2,
    pub dist: f32,
    /// Absolute angle from the observer to the enemy.
    pub bearing: f32,
    /// Offset of the enemy within the beam, normalized to [-1, 1].
    pub offset: f32,
    pub heading: f32,
    pub speed: f32,
}

pub fn radar_contact(w: &World, i: usize) -> Option<Contact> {
    let me = &w.tanks[i];
    let en = &w.tanks[1 - i];
    if !me.alive || !en.alive {
        return None;
    }
    let d = en.pos - me.pos;
    let dist = d.len().max(1e-3);
    let bearing = d.angle();
    let tol = RADAR_HALF_ANGLE + (TANK_RADIUS / dist).min(1.0).asin();
    let off = wrap_angle(bearing - me.radar_abs());
    if off.abs() > tol || !w.terrain.line_of_sight(me.pos, en.pos) {
        return None;
    }
    Some(Contact {
        pos: en.pos,
        dist,
        bearing,
        offset: off / tol,
        heading: en.heading,
        speed: en.speed,
    })
}

/// Hull-relative angle of wall-distance ray `k`.
pub fn ray_angle(k: usize) -> f32 {
    k as f32 * std::f32::consts::TAU / N_RAYS as f32
}

pub fn observe(w: &World, i: usize) -> [f32; N_OBS] {
    let me = &w.tanks[i];
    let mut o = [0.0f32; N_OBS];
    o[0] = me.speed / MAX_SPEED;
    o[1] = me.turret.sin();
    o[2] = me.turret.cos();
    o[3] = me.radar.sin();
    o[4] = me.radar.cos();
    o[5] = me.ammo as f32 / MAX_AMMO as f32;
    o[6] = me.cooldown / FIRE_COOLDOWN;
    o[7] = me.regen / AMMO_REGEN;
    for k in 0..N_RAYS {
        let dir = Vec2::from_angle(me.heading + ray_angle(k));
        o[8 + k] = w.terrain.ray(me.pos, dir, RAY_RANGE) / RAY_RANGE;
    }
    if let Some(c) = me.seen {
        o[16] = 1.0;
        o[17] = c.dist / ARENA_DIAG;
        o[18] = c.offset;
        let rel = wrap_angle(c.heading - c.bearing);
        o[19] = rel.sin();
        o[20] = rel.cos();
        o[21] = c.speed / MAX_SPEED;
    }
    o[22] = me.since_seen.min(5.0) / 5.0;
    if let Some(p) = me.last_seen {
        let d = p - me.pos;
        let b = d.angle();
        let to_turret = wrap_angle(b - me.turret_abs());
        let to_hull = wrap_angle(b - me.heading);
        o[23] = to_turret.sin();
        o[24] = to_turret.cos();
        o[25] = to_hull.sin();
        o[26] = to_hull.cos();
        o[27] = d.len() / ARENA_DIAG;
    }
    // Nearest enemy shell inside the radar beam.
    let radar = me.radar_abs();
    let mut best: Option<(f32, f32)> = None;
    for s in w.shells.iter().filter(|s| s.owner != i) {
        let d = s.pos - me.pos;
        let dist = d.len();
        let off = wrap_angle(d.angle() - radar);
        if off.abs() <= RADAR_HALF_ANGLE * 2.0 && best.is_none_or(|b| dist < b.0) {
            best = Some((dist, off / (RADAR_HALF_ANGLE * 2.0)));
        }
    }
    if let Some((dist, off)) = best {
        o[28] = 1.0;
        o[29] = dist / ARENA_DIAG;
        o[30] = off;
    }
    o[31] = w.terrain.ray(me.pos, Vec2::from_angle(radar), ARENA_DIAG) / ARENA_DIAG;
    o[32] = w
        .terrain
        .ray(me.pos, Vec2::from_angle(me.turret_abs()), ARENA_DIAG)
        / ARENA_DIAG;
    o[33] = w.time() / MATCH_TIME;
    o
}

/// Human-readable names of the observation slots (for the display overlay).
pub const OBS_NAMES: [&str; N_OBS] = [
    "speed",
    "turret sin",
    "turret cos",
    "radar sin",
    "radar cos",
    "ammo",
    "cooldown",
    "regen",
    "ray 0",
    "ray 45",
    "ray 90",
    "ray 135",
    "ray 180",
    "ray 225",
    "ray 270",
    "ray 315",
    "seen",
    "seen dist",
    "seen offset",
    "enemy hdg sin",
    "enemy hdg cos",
    "enemy speed",
    "since seen",
    "last brg/turret sin",
    "last brg/turret cos",
    "last brg/hull sin",
    "last brg/hull cos",
    "last dist",
    "shell seen",
    "shell dist",
    "shell offset",
    "radar wall",
    "gun wall",
    "time",
];
