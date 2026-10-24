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
        /// Input weights for z, r, n gates (each h x N_IN).
        wx: [usize; 3],
        /// Recurrent weights for z, r, n (each h x h).
        wh: [usize; 3],
        b: [usize; 3],
        wo: usize,
        bo: usize,
    },
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
            Arch::Gru | Arch::GruL => {
                let h = if arch == Arch::Gru { 32 } else { 64 };
                Layout::Gru {
                    h,
                    wx: [take(h * N_IN), take(h * N_IN), take(h * N_IN)],
                    wh: [take(h * h), take(h * h), take(h * h)],
                    b: [take(h), take(h), take(h)],
                    wo: take(N_OUT * h),
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
        Layout::Gru { h, wx, wh, wo, .. } => {
            for k in 0..3 {
                fill(wx[k], *h, N_IN);
                fill(wh[k], *h, *h);
            }
            fill(*wo, N_OUT, *h);
        }
    }
    p
}

/// y = W x + b (W is rows x cols, row-major), accumulated into `y`.
/// Eight independent partial sums let the compiler vectorize the dot products
/// (a single running sum must stay sequential under IEEE rules).
#[inline]
pub fn matvec_add(w: &[f32], x: &[f32], y: &mut [f32]) {
    let cols = x.len();
    let split = cols - cols % 8;
    for (r, yr) in y.iter_mut().enumerate() {
        let row = &w[r * cols..(r + 1) * cols];
        let mut acc = [0.0f32; 8];
        for (rc, xc) in row[..split].chunks_exact(8).zip(x[..split].chunks_exact(8)) {
            for l in 0..8 {
                acc[l] += rc[l] * xc[l];
            }
        }
        let mut sum =
            ((acc[0] + acc[4]) + (acc[1] + acc[5])) + ((acc[2] + acc[6]) + (acc[3] + acc[7]));
        for c in split..cols {
            sum += row[c] * x[c];
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
    buf: Vec<Vec<f32>>,
}

impl FixedNet {
    pub fn new(arch: Arch, params: Vec<f32>) -> FixedNet {
        let (layout, n) = Layout::new(arch);
        assert_eq!(params.len(), n, "parameter count mismatch for {arch:?}");
        let h = vec![0.0; layout.hidden()];
        FixedNet {
            arch,
            params,
            layout,
            h,
            buf: vec![Vec::new(); 4],
        }
    }

    pub fn reset(&mut self) {
        self.h.iter_mut().for_each(|v| *v = 0.0);
    }

    pub fn hidden_state(&self) -> &[f32] {
        &self.h
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
                wx,
                wh,
                b,
                wo,
                bo,
            } => {
                let h = *h;
                let [z, r, n, rh] = &mut self.buf[..] else {
                    unreachable!()
                };
                z.clear();
                z.extend_from_slice(&p[b[0]..b[0] + h]);
                r.clear();
                r.extend_from_slice(&p[b[1]..b[1] + h]);
                n.clear();
                n.extend_from_slice(&p[b[2]..b[2] + h]);
                matvec_add(&p[wx[0]..wx[0] + h * N_IN], x, z);
                matvec_add(&p[wh[0]..wh[0] + h * h], &self.h, z);
                matvec_add(&p[wx[1]..wx[1] + h * N_IN], x, r);
                matvec_add(&p[wh[1]..wh[1] + h * h], &self.h, r);
                z.iter_mut().for_each(|v| *v = sigmoid(*v));
                r.iter_mut().for_each(|v| *v = sigmoid(*v));
                rh.clear();
                rh.extend(r.iter().zip(&self.h).map(|(a, b)| a * b));
                matvec_add(&p[wx[2]..wx[2] + h * N_IN], x, n);
                matvec_add(&p[wh[2]..wh[2] + h * h], rh, n);
                for j in 0..h {
                    let nv = n[j].tanh();
                    self.h[j] = (1.0 - z[j]) * nv + z[j] * self.h[j];
                }
                out.copy_from_slice(&p[*bo..*bo + N_OUT]);
                matvec_add(&p[*wo..*wo + N_OUT * h], &self.h, &mut out);
                out.iter_mut().for_each(|v| *v = v.tanh());
            }
        }
        out
    }
}
