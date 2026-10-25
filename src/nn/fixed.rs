//! Fixed-topology networks stored as one flat parameter vector, shared by GA and PPO.

use super::Arch;
use crate::rng::Rng;
use crate::sim::sensors::N_OBS;
use crate::sim::world::Action;

pub const N_IN: usize = N_OBS;
pub const N_OUT: usize = Action::OUTPUTS;

/// Offsets of each weight block inside the flat parameter vector.
#[derive(Clone, Debug)]
pub enum Layout {
    /// Dense layers: (in, out, weight offset, bias offset).
    Mlp(Vec<(usize, usize, usize, usize)>),
    Elman {
        h: usize,
        wx: usize,
        wh: usize,
        b: usize,
        wo: usize,
        bo: usize,
    },
    Gru {
        h: usize,
        /// Inputs: the observations, then the battle memory (`Arch::GruMem`).
        n_in: usize,
        /// Input weights for z, r, n gates (each h x n_in).
        wx: [usize; 3],
        /// Recurrent weights for z, r, n (each h x h).
        wh: [usize; 3],
        b: [usize; 3],
        wo: usize,
        bo: usize,
    },
    /// GRU whose gates also read `k` clocks: leaky units with fixed time
    /// constants (see `clock_decay`), written from [x, h]. The output reads
    /// [h, clocks] and, through a direct linear path, the inputs.
    GruClock {
        h: usize,
        k: usize,
        /// Gate weights over [x, clocks] for z, r, n (each h x (N_IN + k)).
        wx: [usize; 3],
        wh: [usize; 3],
        b: [usize; 3],
        /// Clock writer over [x, h] (k x (N_IN + h)).
        wc: usize,
        bc: usize,
        /// Output over [h, clocks] (N_OUT x (h + k)), directly followed by the
        /// input-to-output path (N_OUT x N_IN).
        wo: usize,
        ws: usize,
        bo: usize,
    },
}

/// Clocks in `Arch::GruClock`.
pub const CLOCKS: usize = 16;

/// Per-decision decay of each clock: time constants spread evenly on a log
/// scale from 3 decisions (0.1 s) to 900 (30 s).
pub fn clock_decay(k: usize) -> Vec<f32> {
    (0..k)
        .map(|j| {
            let tau = 3.0 * 300f32.powf(j as f32 / (k - 1).max(1) as f32);
            (-1.0 / tau).exp()
        })
        .collect()
}

impl Layout {
    pub fn new(arch: Arch) -> (Layout, usize) {
        let mut n = 0;
        let mut take = |k: usize| {
            let o = n;
            n += k;
            o
        };
        let layout = match arch {
            Arch::MlpS | Arch::MlpL => {
                let dims: Vec<usize> = if arch == Arch::MlpS {
                    vec![N_IN, 32, N_OUT]
                } else {
                    vec![N_IN, 64, 64, N_OUT]
                };
                Layout::Mlp(
                    dims.windows(2)
                        .map(|d| (d[0], d[1], take(d[0] * d[1]), take(d[1])))
                        .collect(),
                )
            }
            Arch::Elman => {
                let h = 32;
                Layout::Elman {
                    h,
                    wx: take(h * N_IN),
                    wh: take(h * h),
                    b: take(h),
                    wo: take(N_OUT * h),
                    bo: take(N_OUT),
                }
            }
            Arch::Gru | Arch::GruL | Arch::GruMem => {
                let h = if arch == Arch::GruL { 64 } else { 32 };
                let n_in = N_IN + arch.memory_inputs();
                Layout::Gru {
                    h,
                    n_in,
                    wx: [take(h * n_in), take(h * n_in), take(h * n_in)],
                    wh: [take(h * h), take(h * h), take(h * h)],
                    b: [take(h), take(h), take(h)],
                    wo: take(N_OUT * h),
                    bo: take(N_OUT),
                }
            }
            Arch::GruClock => {
                let (h, k) = (32, CLOCKS);
                let u = N_IN + k;
                Layout::GruClock {
                    h,
                    k,
                    wx: [take(h * u), take(h * u), take(h * u)],
                    wh: [take(h * h), take(h * h), take(h * h)],
                    b: [take(h), take(h), take(h)],
                    wc: take(k * (N_IN + h)),
                    bc: take(k),
                    wo: take(N_OUT * (h + k)),
                    ws: take(N_OUT * N_IN),
                    bo: take(N_OUT),
                }
            }
            Arch::Neat => panic!("NEAT has no fixed layout"),
        };
        (layout, n)
    }

    /// Generic dense stack (used for the PPO value network).
    pub fn mlp(dims: &[usize]) -> (Layout, usize) {
        let mut n = 0;
        let layers = dims
            .windows(2)
            .map(|d| {
                let w = n;
                let b = w + d[0] * d[1];
                n = b + d[1];
                (d[0], d[1], w, b)
            })
            .collect();
        (Layout::Mlp(layers), n)
    }

    pub fn hidden(&self) -> usize {
        match self {
            Layout::Mlp(_) => 0,
            Layout::Elman { h, .. } | Layout::Gru { h, .. } => *h,
            Layout::GruClock { h, k, .. } => h + k,
        }
    }
}

/// Random initial parameters (scaled by 1/sqrt(fan_in), zero biases).
pub fn init_params(arch: Arch, rng: &mut Rng) -> Vec<f32> {
    let (layout, n) = Layout::new(arch);
    let mut p = vec![0.0; n];
    let mut fill = |off: usize, rows: usize, cols: usize| {
        let s = 1.0 / (cols as f32).sqrt();
        for v in &mut p[off..off + rows * cols] {
            *v = rng.gauss() * s;
        }
    };
    match &layout {
        Layout::Mlp(layers) => {
            for &(i, o, w, _) in layers {
                fill(w, o, i);
            }
        }
        Layout::Elman { h, wx, wh, wo, .. } => {
            fill(*wx, *h, N_IN);
            fill(*wh, *h, *h);
            fill(*wo, N_OUT, *h);
        }
        Layout::Gru {
            h,
            n_in,
            wx,
            wh,
            wo,
            ..
        } => {
            for k in 0..3 {
                fill(wx[k], *h, *n_in);
                fill(wh[k], *h, *h);
            }
            fill(*wo, N_OUT, *h);
        }
        Layout::GruClock {
            h,
            k,
            wx,
            wh,
            wc,
            wo,
            ws,
            ..
        } => {
            for g in 0..3 {
                fill(wx[g], *h, N_IN + k);
                fill(wh[g], *h, *h);
            }
            fill(*wc, *k, N_IN + h);
            fill(*wo, N_OUT, h + k);
            fill(*ws, N_OUT, N_IN);
        }
    }
    p
}

/// Copies a trained GRU into the GRU + clocks architecture of the same size.
/// The copy computes the same outputs: its gates and outputs ignore the
/// clocks and the direct path until training connects them. The clock
/// writers get random weights, so the clocks already tick.
///
/// A GRU-32 copied into `Arch::GruMem` also computes the same outputs: the
/// weights on the battle-memory inputs start at zero.
pub fn transplant(from: Arch, p: &[f32], to: Arch, rng: &mut Rng) -> Option<Vec<f32>> {
    if from == Arch::Gru && to == Arch::GruMem && p.len() == from.param_count() {
        let (
            Layout::Gru { h, wx, wh, .. },
            Layout::Gru {
                n_in,
                wx: wx2,
                wh: wh2,
                ..
            },
        ) = (Layout::new(from).0, Layout::new(to).0)
        else {
            return None;
        };
        let mut q = vec![0.0; to.param_count()];
        for g in 0..3 {
            for r in 0..h {
                q[wx2[g] + r * n_in..][..N_IN].copy_from_slice(&p[wx[g] + r * N_IN..][..N_IN]);
            }
        }
        // Everything after the input weights has the same layout.
        q[wh2[0]..].copy_from_slice(&p[wh[0]..]);
        return Some(q);
    }
    if !matches!(from, Arch::Gru | Arch::GruL) || to != Arch::GruClock {
        return None;
    }
    let (
        Layout::Gru {
            h,
            wx,
            wh,
            b,
            wo,
            bo,
            ..
        },
        Layout::GruClock {
            h: h2,
            k,
            wx: wx2,
            wh: wh2,
            b: b2,
            wo: wo2,
            ws,
            bo: bo2,
            ..
        },
    ) = (Layout::new(from).0, Layout::new(to).0)
    else {
        return None;
    };
    if h != h2 || p.len() != from.param_count() {
        return None;
    }
    let mut q = init_params(to, rng);
    let u = N_IN + k;
    for g in 0..3 {
        for r in 0..h {
            let row = wx2[g] + r * u;
            q[row..row + N_IN].copy_from_slice(&p[wx[g] + r * N_IN..wx[g] + (r + 1) * N_IN]);
            q[row + N_IN..row + u].fill(0.0);
        }
        q[wh2[g]..wh2[g] + h * h].copy_from_slice(&p[wh[g]..wh[g] + h * h]);
        q[b2[g]..b2[g] + h].copy_from_slice(&p[b[g]..b[g] + h]);
    }
    for o in 0..N_OUT {
        let row = wo2 + o * (h + k);
        q[row..row + h].copy_from_slice(&p[wo + o * h..wo + (o + 1) * h]);
        q[row + h..row + h + k].fill(0.0);
    }
    q[ws..ws + N_OUT * N_IN].fill(0.0);
    q[bo2..bo2 + N_OUT].copy_from_slice(&p[bo..bo + N_OUT]);
    Some(q)
}

/// y = W x + b (W is rows x cols, row-major), accumulated into `y`.
/// Eight independent partial sums let the compiler vectorize the dot products
/// (a single running sum must stay sequential under IEEE rules).
#[inline]
pub fn matvec_add(w: &[f32], x: &[f32], y: &mut [f32]) {
    let cols = x.len();
    let (x8, x_rest) = x.as_chunks::<8>();
    for (r, yr) in y.iter_mut().enumerate() {
        let (w8, w_rest) = w[r * cols..(r + 1) * cols].as_chunks::<8>();
        let mut acc = [0.0f32; 8];
        for (wc, xc) in w8.iter().zip(x8) {
            for l in 0..8 {
                acc[l] += wc[l] * xc[l];
            }
        }
        let mut sum =
            ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]));
        for (wv, xv) in w_rest.iter().zip(x_rest) {
            sum += wv * xv;
        }
        *yr += sum;
    }
}

pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[derive(Clone, Debug)]
pub struct FixedNet {
    pub arch: Arch,
    pub params: Vec<f32>,
    layout: Layout,
    h: Vec<f32>,
    /// Clock decays (GRU + clocks only).
    decay: Vec<f32>,
    buf: Vec<Vec<f32>>,
}

impl FixedNet {
    pub fn new(arch: Arch, params: Vec<f32>) -> FixedNet {
        let (layout, n) = Layout::new(arch);
        assert_eq!(params.len(), n, "parameter count mismatch for {arch:?}");
        let h = vec![0.0; layout.hidden()];
        let decay = match &layout {
            Layout::GruClock { k, .. } => clock_decay(*k),
            _ => Vec::new(),
        };
        FixedNet {
            arch,
            params,
            layout,
            h,
            decay,
            buf: vec![Vec::new(); 6],
        }
    }

    pub fn reset(&mut self) {
        self.h.iter_mut().for_each(|v| *v = 0.0);
    }

    pub fn hidden_state(&self) -> &[f32] {
        &self.h
    }

    pub fn set_hidden_state(&mut self, h: &[f32]) {
        self.h.copy_from_slice(h);
    }

    /// One step; returns tanh outputs in [-1, 1].
    pub fn forward(&mut self, x: &[f32]) -> [f32; N_OUT] {
        let p = &self.params;
        let mut out = [0.0f32; N_OUT];
        match &self.layout {
            Layout::Mlp(layers) => {
                let mut cur = x.to_vec();
                for &(i, o, w, b) in layers {
                    let mut y = p[b..b + o].to_vec();
                    matvec_add(&p[w..w + i * o], &cur, &mut y);
                    y.iter_mut().for_each(|v| *v = v.tanh());
                    cur = y;
                }
                out.copy_from_slice(&cur);
            }
            Layout::Elman {
                h,
                wx,
                wh,
                b,
                wo,
                bo,
            } => {
                let h = *h;
                let mut a = p[*b..*b + h].to_vec();
                matvec_add(&p[*wx..*wx + h * N_IN], x, &mut a);
                matvec_add(&p[*wh..*wh + h * h], &self.h, &mut a);
                for (hv, av) in self.h.iter_mut().zip(&a) {
                    *hv = av.tanh();
                }
                out.copy_from_slice(&p[*bo..*bo + N_OUT]);
                matvec_add(&p[*wo..*wo + N_OUT * h], &self.h, &mut out);
                out.iter_mut().for_each(|v| *v = v.tanh());
            }
            Layout::Gru {
                h,
                n_in,
                wx,
                wh,
                b,
                wo,
                bo,
            } => {
                let (h, n_in) = (*h, *n_in);
                let [z, r, n, rh, u, ..] = &mut self.buf[..] else {
                    unreachable!()
                };
                // Inputs it does not get (the battle memory outside Robocode
                // battles) stay at zero.
                let x = if x.len() == n_in {
                    x
                } else {
                    u.clear();
                    u.extend(x.iter().copied().chain(std::iter::repeat(0.0)).take(n_in));
                    &u[..]
                };
                z.clear();
                z.extend_from_slice(&p[b[0]..b[0] + h]);
                r.clear();
                r.extend_from_slice(&p[b[1]..b[1] + h]);
                n.clear();
                n.extend_from_slice(&p[b[2]..b[2] + h]);
                matvec_add(&p[wx[0]..wx[0] + h * n_in], x, z);
                matvec_add(&p[wh[0]..wh[0] + h * h], &self.h, z);
                matvec_add(&p[wx[1]..wx[1] + h * n_in], x, r);
                matvec_add(&p[wh[1]..wh[1] + h * h], &self.h, r);
                z.iter_mut().for_each(|v| *v = sigmoid(*v));
                r.iter_mut().for_each(|v| *v = sigmoid(*v));
                rh.clear();
                rh.extend(r.iter().zip(&self.h).map(|(a, b)| a * b));
                matvec_add(&p[wx[2]..wx[2] + h * n_in], x, n);
                matvec_add(&p[wh[2]..wh[2] + h * h], rh, n);
                for j in 0..h {
                    let nv = n[j].tanh();
                    self.h[j] = (1.0 - z[j]) * nv + z[j] * self.h[j];
                }
                out.copy_from_slice(&p[*bo..*bo + N_OUT]);
                matvec_add(&p[*wo..*wo + N_OUT * h], &self.h, &mut out);
                out.iter_mut().for_each(|v| *v = v.tanh());
            }
            Layout::GruClock {
                h,
                k,
                wx,
                wh,
                b,
                wc,
                bc,
                wo,
                ws,
                bo,
            } => {
                let (h, k) = (*h, *k);
                let nu = N_IN + k;
                let [z, r, n, rh, u, a] = &mut self.buf[..] else {
                    unreachable!()
                };
                // The state is [h, clocks].
                let (hs, cs) = self.h.split_at_mut(h);
                u.clear();
                u.extend_from_slice(x);
                u.extend_from_slice(cs);
                z.clear();
                z.extend_from_slice(&p[b[0]..b[0] + h]);
                r.clear();
                r.extend_from_slice(&p[b[1]..b[1] + h]);
                n.clear();
                n.extend_from_slice(&p[b[2]..b[2] + h]);
                matvec_add(&p[wx[0]..wx[0] + h * nu], u, z);
                matvec_add(&p[wh[0]..wh[0] + h * h], hs, z);
                matvec_add(&p[wx[1]..wx[1] + h * nu], u, r);
                matvec_add(&p[wh[1]..wh[1] + h * h], hs, r);
                z.iter_mut().for_each(|v| *v = sigmoid(*v));
                r.iter_mut().for_each(|v| *v = sigmoid(*v));
                rh.clear();
                rh.extend(r.iter().zip(hs.iter()).map(|(a, b)| a * b));
                matvec_add(&p[wx[2]..wx[2] + h * nu], u, n);
                matvec_add(&p[wh[2]..wh[2] + h * h], rh, n);
                for j in 0..h {
                    let nv = n[j].tanh();
                    hs[j] = (1.0 - z[j]) * nv + z[j] * hs[j];
                }
                // Clocks: each one leaks toward what the writer says.
                u.truncate(N_IN);
                u.extend_from_slice(hs);
                a.clear();
                a.extend_from_slice(&p[*bc..*bc + k]);
                matvec_add(&p[*wc..*wc + k * (N_IN + h)], u, a);
                for j in 0..k {
                    let d = self.decay[j];
                    cs[j] = d * cs[j] + (1.0 - d) * a[j].tanh();
                }
                out.copy_from_slice(&p[*bo..*bo + N_OUT]);
                matvec_add(&p[*wo..*wo + N_OUT * (h + k)], &self.h, &mut out);
                matvec_add(&p[*ws..*ws + N_OUT * N_IN], x, &mut out);
                out.iter_mut().for_each(|v| *v = v.tanh());
            }
        }
        out
    }
}
