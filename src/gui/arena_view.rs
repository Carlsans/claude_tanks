//! Draws a `World` with egui shapes (no custom GPU work).

use crate::sim::sensors::{N_RAYS, ray_angle};
use crate::sim::world::World;
use crate::sim::{ARENA, RADAR_HALF_ANGLE, RAY_RANGE, SHELL_RADIUS, TANK_RADIUS, Vec2};
use eframe::egui::{self, Color32, Pos2, Shape, Stroke};

pub const TANK_COLORS: [Color32; 2] = [
    Color32::from_rgb(70, 160, 235),
    Color32::from_rgb(235, 120, 60),
];

#[derive(Clone, Copy)]
pub struct Overlays {
    pub radar: bool,
    pub rays: bool,
    pub last_seen: bool,
}

impl Default for Overlays {
    fn default() -> Self {
        Overlays {
            radar: true,
            rays: false,
            last_seen: true,
        }
    }
}

/// Short-lived visual effect (muzzle flash, impact, explosion).
#[derive(Clone, Copy)]
pub struct Effect {
    pub pos: Vec2,
    pub age: f32,
    pub life: f32,
    pub radius: f32,
    pub color: Color32,
}

struct View {
    origin: Pos2,
    scale: f32,
}

impl View {
    fn p(&self, v: Vec2) -> Pos2 {
        Pos2::new(
            self.origin.x + v.x * self.scale,
            self.origin.y + v.y * self.scale,
        )
    }
    fn s(&self, d: f32) -> f32 {
        d * self.scale
    }
}

pub fn draw(ui: &mut egui::Ui, world: &World, ov: Overlays, effects: &[Effect]) {
    let avail = ui.available_size();
    let side = avail.x.min(avail.y).max(100.0);
    let (resp, painter) = ui.allocate_painter(egui::vec2(side, side), egui::Sense::hover());
    let rect = resp.rect;
    let v = View {
        origin: rect.min,
        scale: side / ARENA,
    };
    painter.rect_filled(rect, 0.0, Color32::from_rgb(30, 36, 32));

    // Grid.
    let grid = Color32::from_rgb(38, 46, 40);
    for k in 1..10 {
        let d = k as f32 * ARENA / 10.0;
        painter.line_segment(
            [v.p(Vec2::new(d, 0.0)), v.p(Vec2::new(d, ARENA))],
            Stroke::new(1.0, grid),
        );
        painter.line_segment(
            [v.p(Vec2::new(0.0, d)), v.p(Vec2::new(ARENA, d))],
            Stroke::new(1.0, grid),
        );
    }

    for w in &world.terrain.walls {
        painter.rect_filled(
            egui::Rect::from_min_max(v.p(w.min), v.p(w.max)),
            1.0,
            Color32::from_rgb(150, 150, 140),
        );
    }

    for (i, t) in world.tanks.iter().enumerate() {
        let color = TANK_COLORS[i];
        if t.alive && ov.rays {
            for k in 0..N_RAYS {
                let dir = Vec2::from_angle(t.heading + ray_angle(k));
                let d = world.terrain.ray(t.pos, dir, RAY_RANGE);
                painter.line_segment(
                    [v.p(t.pos), v.p(t.pos + dir * d)],
                    Stroke::new(1.0, color.gamma_multiply(0.25)),
                );
            }
        }
        if t.alive && ov.radar {
            let a = t.radar_abs();
            let range = world
                .terrain
                .ray(t.pos, Vec2::from_angle(a), crate::sim::ARENA_DIAG);
            let fill = if t.seen.is_some() {
                Color32::from_rgba_unmultiplied(255, 255, 120, 50)
            } else {
                color.gamma_multiply(0.12)
            };
            let l = v.p(t.pos + Vec2::from_angle(a - RADAR_HALF_ANGLE) * range);
            let r = v.p(t.pos + Vec2::from_angle(a + RADAR_HALF_ANGLE) * range);
            painter.add(Shape::convex_polygon(
                vec![v.p(t.pos), l, r],
                fill,
                Stroke::NONE,
            ));
        }
        if ov.last_seen
            && let Some(p) = t.last_seen
            && t.alive
        {
            let c = v.p(p);
            let s = v.s(8.0);
            let st = Stroke::new(1.5, color.gamma_multiply(0.7));
            painter.line_segment([c + egui::vec2(-s, -s), c + egui::vec2(s, s)], st);
            painter.line_segment([c + egui::vec2(-s, s), c + egui::vec2(s, -s)], st);
        }
    }

    for (i, t) in world.tanks.iter().enumerate() {
        draw_tank(&painter, &v, t, TANK_COLORS[i]);
    }

    for s in &world.shells {
        painter.circle_filled(
            v.p(s.pos),
            v.s(SHELL_RADIUS + 1.0).max(2.0),
            Color32::from_rgb(255, 240, 200),
        );
        let tail = s.pos - s.vel * 0.03;
        painter.line_segment(
            [v.p(tail), v.p(s.pos)],
            Stroke::new(
                v.s(2.0).max(1.0),
                Color32::from_rgba_unmultiplied(255, 220, 150, 90),
            ),
        );
    }

    for e in effects {
        let k = (e.age / e.life).clamp(0.0, 1.0);
        let r = v.s(e.radius * (0.4 + k));
        painter.circle_filled(v.p(e.pos), r, e.color.gamma_multiply(1.0 - k));
    }
}

fn draw_tank(painter: &egui::Painter, v: &View, t: &crate::sim::world::Tank, color: Color32) {
    let body = if t.alive {
        color
    } else {
        Color32::from_gray(70)
    };
    let rot = |local: Vec2, ang: f32| {
        let (s, c) = ang.sin_cos();
        t.pos + Vec2::new(local.x * c - local.y * s, local.x * s + local.y * c)
    };
    let (hl, hw) = (TANK_RADIUS * 1.15, TANK_RADIUS * 0.85);
    let hull: Vec<Pos2> = [(-hl, -hw), (hl, -hw), (hl, hw), (-hl, hw)]
        .iter()
        .map(|&(x, y)| v.p(rot(Vec2::new(x, y), t.heading)))
        .collect();
    painter.add(Shape::convex_polygon(
        hull,
        body.gamma_multiply(0.75),
        Stroke::new(1.5, body),
    ));
    // Treads.
    for side in [-1.0, 1.0] {
        let a = v.p(rot(Vec2::new(-hl, side * hw), t.heading));
        let b = v.p(rot(Vec2::new(hl, side * hw), t.heading));
        painter.line_segment([a, b], Stroke::new(v.s(5.0), Color32::from_gray(40)));
    }
    // Front marker.
    painter.circle_filled(
        v.p(rot(Vec2::new(hl - 4.0, 0.0), t.heading)),
        v.s(2.5),
        Color32::from_gray(220),
    );
    let ta = t.turret_abs();
    painter.line_segment(
        [
            v.p(t.pos),
            v.p(t.pos + Vec2::from_angle(ta) * (TANK_RADIUS + 10.0)),
        ],
        Stroke::new(v.s(5.0), Color32::from_gray(if t.alive { 210 } else { 90 })),
    );
    painter.circle_filled(v.p(t.pos), v.s(9.0), body);
    painter.circle_stroke(
        v.p(t.pos),
        v.s(9.0),
        Stroke::new(1.0, Color32::from_gray(20)),
    );
    // Radar dish.
    let ra = t.radar_abs();
    let dish = t.pos + Vec2::from_angle(ra) * 6.0;
    let perp = Vec2::from_angle(ra + std::f32::consts::FRAC_PI_2) * 4.0;
    painter.line_segment(
        [v.p(dish - perp), v.p(dish + perp)],
        Stroke::new(v.s(2.5).max(1.5), Color32::WHITE),
    );
}
