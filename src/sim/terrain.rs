//! Random arenas ranging from an open field (density 0) to a light labyrinth (density 1).

use super::{ARENA, CELL, GRID, Vec2, WALL_THICK};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// Distance along `dir` (unit) from `o` to the rect, if hit within `max_t`.
    pub fn ray(&self, o: Vec2, dir: Vec2, max_t: f32) -> Option<f32> {
        let mut t0 = 0.0f32;
        let mut t1 = max_t;
        for (o, d, lo, hi) in [
            (o.x, dir.x, self.min.x, self.max.x),
            (o.y, dir.y, self.min.y, self.max.y),
        ] {
            if d.abs() < 1e-9 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let (mut a, mut b) = ((lo - o) * inv, (hi - o) * inv);
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                t0 = t0.max(a);
                t1 = t1.min(b);
                if t0 > t1 {
                    return None;
                }
            }
        }
        Some(t0)
    }

    /// Push a circle out of the rect. Returns true if they overlapped.
    pub fn push_out(&self, p: &mut Vec2, r: f32) -> bool {
        let cx = p.x.clamp(self.min.x, self.max.x);
        let cy = p.y.clamp(self.min.y, self.max.y);
        let d = Vec2::new(p.x - cx, p.y - cy);
        let dist2 = d.dot(d);
        if dist2 >= r * r {
            return false;
        }
        if dist2 > 1e-12 {
            let dist = dist2.sqrt();
            *p = Vec2::new(cx, cy) + d * (r / dist);
        } else {
            // Center inside the rect: exit through the nearest side.
            let exits = [
                (p.x - self.min.x + r, Vec2::new(self.min.x - r, p.y)),
                (self.max.x - p.x + r, Vec2::new(self.max.x + r, p.y)),
                (p.y - self.min.y + r, Vec2::new(p.x, self.min.y - r)),
                (self.max.y - p.y + r, Vec2::new(p.x, self.max.y + r)),
            ];
            *p = exits
                .iter()
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|e| e.1)
                .unwrap();
        }
        true
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Terrain {
    pub seed: u64,
    pub density: f32,
    pub walls: Vec<Rect>,
    /// Spawn position and hull heading for each side.
    pub spawns: [(Vec2, f32); 2],
}

impl Terrain {
    pub fn generate(seed: u64, density: f32) -> Terrain {
        let density = density.clamp(0.0, 1.0);
        let mut rng = Rng::new(seed ^ 0x7E44_A1F0);
        let n = GRID;
        // right[x][y]: wall between (x,y) and (x+1,y); down[x][y]: between (x,y) and (x,y+1).
        let mut right = vec![vec![true; n]; n];
        let mut down = vec![vec![true; n]; n];

        // Perfect maze via iterative recursive backtracker.
        let mut visited = vec![vec![false; n]; n];
        let mut stack = vec![(rng.below(n), rng.below(n))];
        visited[stack[0].0][stack[0].1] = true;
        while let Some(&(x, y)) = stack.last() {
            let mut nbrs = Vec::with_capacity(4);
            if x > 0 && !visited[x - 1][y] {
                nbrs.push((x - 1, y));
            }
            if x + 1 < n && !visited[x + 1][y] {
                nbrs.push((x + 1, y));
            }
            if y > 0 && !visited[x][y - 1] {
                nbrs.push((x, y - 1));
            }
            if y + 1 < n && !visited[x][y + 1] {
                nbrs.push((x, y + 1));
            }
            if nbrs.is_empty() {
                stack.pop();
                continue;
            }
            let (nx, ny) = nbrs[rng.below(nbrs.len())];
            match (nx as i32 - x as i32, ny as i32 - y as i32) {
                (1, 0) => right[x][y] = false,
                (-1, 0) => right[nx][ny] = false,
                (0, 1) => down[x][y] = false,
                _ => down[nx][ny] = false,
            }
            visited[nx][ny] = true;
            stack.push((nx, ny));
        }

        // Thin the maze: keep each remaining wall with probability `density`.
        for x in 0..n {
            for y in 0..n {
                if right[x][y] && !rng.chance(density) {
                    right[x][y] = false;
                }
                if down[x][y] && !rng.chance(density) {
                    down[x][y] = false;
                }
            }
        }

        let h = WALL_THICK / 2.0;
        let mut walls = vec![
            Rect {
                min: Vec2::new(-h, -h),
                max: Vec2::new(ARENA + h, h),
            },
            Rect {
                min: Vec2::new(-h, ARENA - h),
                max: Vec2::new(ARENA + h, ARENA + h),
            },
            Rect {
                min: Vec2::new(-h, -h),
                max: Vec2::new(h, ARENA + h),
            },
            Rect {
                min: Vec2::new(ARENA - h, -h),
                max: Vec2::new(ARENA + h, ARENA + h),
            },
        ];
        for x in 0..n {
            for y in 0..n {
                if x + 1 < n && right[x][y] {
                    let wx = (x + 1) as f32 * CELL;
                    walls.push(Rect {
                        min: Vec2::new(wx - h, y as f32 * CELL - h),
                        max: Vec2::new(wx + h, (y + 1) as f32 * CELL + h),
                    });
                }
                if y + 1 < n && down[x][y] {
                    let wy = (y + 1) as f32 * CELL;
                    walls.push(Rect {
                        min: Vec2::new(x as f32 * CELL - h, wy - h),
                        max: Vec2::new((x + 1) as f32 * CELL + h, wy + h),
                    });
                }
            }
        }

        // Spawns: BFS distances over the (connected) cell graph, pick a far pair.
        let open = |x: usize, y: usize, nx: usize, ny: usize| -> bool {
            if nx > x {
                !right[x][y]
            } else if nx < x {
                !right[nx][ny]
            } else if ny > y {
                !down[x][y]
            } else {
                !down[nx][ny]
            }
        };
        let a = (rng.below(n), rng.below(n));
        let mut dist = vec![vec![usize::MAX; n]; n];
        dist[a.0][a.1] = 0;
        let mut queue = std::collections::VecDeque::from([a]);
        while let Some((x, y)) = queue.pop_front() {
            let cand = [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ];
            for (nx, ny) in cand {
                if nx < n && ny < n && dist[nx][ny] == usize::MAX && open(x, y, nx, ny) {
                    dist[nx][ny] = dist[x][y] + 1;
                    queue.push_back((nx, ny));
                }
            }
        }
        let center =
            |c: (usize, usize)| Vec2::new((c.0 as f32 + 0.5) * CELL, (c.1 as f32 + 0.5) * CELL);
        let far: Vec<(usize, usize)> = (0..n)
            .flat_map(|x| (0..n).map(move |y| (x, y)))
            .filter(|&c| (center(c) - center(a)).len() >= ARENA * 0.5)
            .collect();
        let max_d = far.iter().map(|c| dist[c.0][c.1]).max().unwrap_or(0);
        let best: Vec<_> = far
            .into_iter()
            .filter(|c| dist[c.0][c.1] * 10 >= max_d * 7)
            .collect();
        let b = best[rng.below(best.len())];

        // A few free-standing pillars on grid corners (never block a cell passage).
        let blocks = ((1.0 - density) * 6.0).round() as usize;
        for _ in 0..blocks {
            let p = Vec2::new(
                (1 + rng.below(n - 1)) as f32 * CELL,
                (1 + rng.below(n - 1)) as f32 * CELL,
            );
            let half = rng.range(12.0, 24.0);
            walls.push(Rect {
                min: Vec2::new(p.x - half, p.y - half),
                max: Vec2::new(p.x + half, p.y + half),
            });
        }

        let spawns = [
            (
                center(a),
                rng.range(-std::f32::consts::PI, std::f32::consts::PI),
            ),
            (
                center(b),
                rng.range(-std::f32::consts::PI, std::f32::consts::PI),
            ),
        ];
        Terrain {
            seed,
            density,
            walls,
            spawns,
        }
    }

    /// Distance to the first wall along a ray, capped at `max_t`.
    pub fn ray(&self, o: Vec2, dir: Vec2, max_t: f32) -> f32 {
        let mut best = max_t;
        for w in &self.walls {
            if let Some(t) = w.ray(o, dir, best) {
                best = best.min(t);
            }
        }
        best
    }

    /// True if the straight segment a->b crosses no wall.
    pub fn line_of_sight(&self, a: Vec2, b: Vec2) -> bool {
        let d = b - a;
        let len = d.len();
        if len < 1e-6 {
            return true;
        }
        let dir = d * (1.0 / len);
        self.walls.iter().all(|w| w.ray(a, dir, len).is_none())
    }
}
