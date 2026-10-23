//! Forward-with-cache and backpropagation (through time) for fixed-topology nets.
//! Used by PPO; parameter layout is identical to `FixedNet`.

use super::fixed::{Layout, N_IN, N_OUT, matvec_add, sigmoid};

/// Activations kept for the backward pass over a sequence.
pub enum SeqCache {
    /// Per step: activations of every layer (input first, output last).
    Mlp(Vec<Vec<Vec<f32>>>),
    /// Per step: (h_prev, h).
    Elman(Vec<(Vec<f32>, Vec<f32>)>),
    Gru(Vec<GruStep>),
}

pub struct GruStep {
    h_prev: Vec<f32>,
    z: Vec<f32>,
    r: Vec<f32>,
    n: Vec<f32>,
    h: Vec<f32>,
}

/// Dense stack forward. `linear_out` leaves the last layer without tanh.
pub fn dense_forward(
    layers: &[(usize, usize, usize, usize)],
    p: &[f32],
    x: &[f32],
    linear_out: bool,
) -> Vec<Vec<f32>> {
    let mut acts = vec![x.to_vec()];
    for (li, &(i, o, w, b)) in layers.iter().enumerate() {
        let mut y = p[b..b + o].to_vec();
        matvec_add(&p[w..w + i * o], acts.last().unwrap(), &mut y);
        if !(linear_out && li == layers.len() - 1) {
            y.iter_mut().for_each(|v| *v = v.tanh());
        }
        acts.push(y);
    }
    acts
}

/// Backprop through a dense stack given dL/d(output); accumulates into `g`.
pub fn dense_backward(
    layers: &[(usize, usize, usize, usize)],
    p: &[f32],
    acts: &[Vec<f32>],
    d_out: &[f32],
    linear_out: bool,
    g: &mut [f32],
) {
    let mut d = d_out.to_vec();
    for li in (0..layers.len()).rev() {
        let (i, o, w, b) = layers[li];
        let a = &acts[li + 1];
        if !(linear_out && li == layers.len() - 1) {
            for k in 0..o {
                d[k] *= 1.0 - a[k] * a[k];
            }
        }
        let x = &acts[li];
        for k in 0..o {
            g[b + k] += d[k];
            let row = &mut g[w + k * i..w + (k + 1) * i];
            for c in 0..i {
                row[c] += d[k] * x[c];
            }
        }
        if li > 0 {
            let mut dx = vec![0.0; i];
            for k in 0..o {
                let row = &p[w + k * i..w + (k + 1) * i];
                for c in 0..i {
                    dx[c] += row[c] * d[k];
                }
            }
            d = dx;
        }
    }
}

/// Wᵀ d accumulated into `dx` (W is rows x cols).
fn matvec_t_add(w: &[f32], d: &[f32], dx: &mut [f32]) {
    let cols = dx.len();
    for (r, dr) in d.iter().enumerate() {
        let row = &w[r * cols..(r + 1) * cols];
        for c in 0..cols {
            dx[c] += row[c] * dr;
        }
    }
}

/// g[W] += d xᵀ
fn outer_add(g: &mut [f32], d: &[f32], x: &[f32]) {
    let cols = x.len();
    for (r, dr) in d.iter().enumerate() {
        let row = &mut g[r * cols..(r + 1) * cols];
        for c in 0..cols {
            row[c] += dr * x[c];
        }
    }
}

/// Run the policy over a sequence starting from hidden state `h0`.
/// Returns tanh outputs per step and the cache.
pub fn seq_forward(
    layout: &Layout,
    p: &[f32],
    h0: &[f32],
    xs: &[[f32; N_IN]],
) -> (Vec<[f32; N_OUT]>, SeqCache) {
    let mut outs = Vec::with_capacity(xs.len());
    let to_out = |v: &[f32]| {
        let mut o = [0.0; N_OUT];
        o.copy_from_slice(v);
        o
    };
    match layout {
        Layout::Mlp(layers) => {
            let mut cache = Vec::with_capacity(xs.len());
            for x in xs {
                let acts = dense_forward(layers, p, x, false);
                outs.push(to_out(acts.last().unwrap()));
                cache.push(acts);
            }
            (outs, SeqCache::Mlp(cache))
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
            let mut hp = h0.to_vec();
            let mut cache = Vec::with_capacity(xs.len());
            for x in xs {
                let mut a = p[*b..*b + h].to_vec();
                matvec_add(&p[*wx..*wx + h * N_IN], x, &mut a);
                matvec_add(&p[*wh..*wh + h * h], &hp, &mut a);
                a.iter_mut().for_each(|v| *v = v.tanh());
                let mut o = p[*bo..*bo + N_OUT].to_vec();
                matvec_add(&p[*wo..*wo + N_OUT * h], &a, &mut o);
                o.iter_mut().for_each(|v| *v = v.tanh());
                outs.push(to_out(&o));
                cache.push((hp, a.clone()));
                hp = a;
            }
            (outs, SeqCache::Elman(cache))
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
            let mut hp = h0.to_vec();
            let mut cache = Vec::with_capacity(xs.len());
            for x in xs {
                let mut z = p[b[0]..b[0] + h].to_vec();
                let mut r = p[b[1]..b[1] + h].to_vec();
                let mut n = p[b[2]..b[2] + h].to_vec();
                matvec_add(&p[wx[0]..wx[0] + h * N_IN], x, &mut z);
                matvec_add(&p[wh[0]..wh[0] + h * h], &hp, &mut z);
                matvec_add(&p[wx[1]..wx[1] + h * N_IN], x, &mut r);
                matvec_add(&p[wh[1]..wh[1] + h * h], &hp, &mut r);
                z.iter_mut().for_each(|v| *v = sigmoid(*v));
                r.iter_mut().for_each(|v| *v = sigmoid(*v));
                let rh: Vec<f32> = r.iter().zip(&hp).map(|(a, b)| a * b).collect();
                matvec_add(&p[wx[2]..wx[2] + h * N_IN], x, &mut n);
                matvec_add(&p[wh[2]..wh[2] + h * h], &rh, &mut n);
                n.iter_mut().for_each(|v| *v = v.tanh());
                let hn: Vec<f32> = (0..h).map(|j| (1.0 - z[j]) * n[j] + z[j] * hp[j]).collect();
                let mut o = p[*bo..*bo + N_OUT].to_vec();
                matvec_add(&p[*wo..*wo + N_OUT * h], &hn, &mut o);
                o.iter_mut().for_each(|v| *v = v.tanh());
                outs.push(to_out(&o));
                cache.push(GruStep {
                    h_prev: hp,
                    z,
                    r,
                    n,
                    h: hn.clone(),
                });
                hp = hn;
            }
            (outs, SeqCache::Gru(cache))
        }
    }
}

/// Backprop through time. `d_out[t]` is dL/d(tanh output at t); gradients accumulate into `g`.
pub fn seq_backward(
    layout: &Layout,
    p: &[f32],
    xs: &[[f32; N_IN]],
    outs: &[[f32; N_OUT]],
    cache: &SeqCache,
    d_out: &[[f32; N_OUT]],
    g: &mut [f32],
) {
    match (layout, cache) {
        (Layout::Mlp(layers), SeqCache::Mlp(c)) => {
            for (acts, d) in c.iter().zip(d_out) {
                dense_backward(layers, p, acts, d, false, g);
            }
        }
        (
            Layout::Elman {
                h,
                wx,
                wh,
                b,
                wo,
                bo,
            },
            SeqCache::Elman(c),
        ) => {
            let h = *h;
            let mut dh_next = vec![0.0; h];
            for t in (0..c.len()).rev() {
                let (hp, hn) = &c[t];
                let dpre_o: Vec<f32> = (0..N_OUT)
                    .map(|k| d_out[t][k] * (1.0 - outs[t][k] * outs[t][k]))
                    .collect();
                for k in 0..N_OUT {
                    g[*bo + k] += dpre_o[k];
                }
                outer_add(&mut g[*wo..*wo + N_OUT * h], &dpre_o, hn);
                let mut dh = dh_next.clone();
                matvec_t_add(&p[*wo..*wo + N_OUT * h], &dpre_o, &mut dh);
                let da: Vec<f32> = (0..h).map(|j| dh[j] * (1.0 - hn[j] * hn[j])).collect();
                for j in 0..h {
                    g[*b + j] += da[j];
                }
                outer_add(&mut g[*wx..*wx + h * N_IN], &da, &xs[t]);
                outer_add(&mut g[*wh..*wh + h * h], &da, hp);
                dh_next = vec![0.0; h];
                matvec_t_add(&p[*wh..*wh + h * h], &da, &mut dh_next);
            }
        }
        (
            Layout::Gru {
                h,
                wx,
                wh,
                b,
                wo,
                bo,
            },
            SeqCache::Gru(c),
        ) => {
            let h = *h;
            let mut dh_next = vec![0.0; h];
            for t in (0..c.len()).rev() {
                let s = &c[t];
                let dpre_o: Vec<f32> = (0..N_OUT)
                    .map(|k| d_out[t][k] * (1.0 - outs[t][k] * outs[t][k]))
                    .collect();
                for k in 0..N_OUT {
                    g[*bo + k] += dpre_o[k];
                }
                outer_add(&mut g[*wo..*wo + N_OUT * h], &dpre_o, &s.h);
                let mut dh = dh_next.clone();
                matvec_t_add(&p[*wo..*wo + N_OUT * h], &dpre_o, &mut dh);

                let mut dhp = vec![0.0; h];
                let mut dn_pre = vec![0.0; h];
                let mut dz_pre = vec![0.0; h];
                for j in 0..h {
                    let dn = dh[j] * (1.0 - s.z[j]);
                    let dz = dh[j] * (s.h_prev[j] - s.n[j]);
                    dhp[j] += dh[j] * s.z[j];
                    dn_pre[j] = dn * (1.0 - s.n[j] * s.n[j]);
                    dz_pre[j] = dz * s.z[j] * (1.0 - s.z[j]);
                }
                let rh: Vec<f32> = (0..h).map(|j| s.r[j] * s.h_prev[j]).collect();
                // Candidate gate.
                for j in 0..h {
                    g[b[2] + j] += dn_pre[j];
                }
                outer_add(&mut g[wx[2]..wx[2] + h * N_IN], &dn_pre, &xs[t]);
                outer_add(&mut g[wh[2]..wh[2] + h * h], &dn_pre, &rh);
                let mut drh = vec![0.0; h];
                matvec_t_add(&p[wh[2]..wh[2] + h * h], &dn_pre, &mut drh);
                let mut dr_pre = vec![0.0; h];
                for j in 0..h {
                    dhp[j] += drh[j] * s.r[j];
                    dr_pre[j] = drh[j] * s.h_prev[j] * s.r[j] * (1.0 - s.r[j]);
                }
                // Update and reset gates.
                for (k, d) in [(0usize, &dz_pre), (1, &dr_pre)] {
                    for j in 0..h {
                        g[b[k] + j] += d[j];
                    }
                    outer_add(&mut g[wx[k]..wx[k] + h * N_IN], d, &xs[t]);
                    outer_add(&mut g[wh[k]..wh[k] + h * h], d, &s.h_prev);
                    matvec_t_add(&p[wh[k]..wh[k] + h * h], d, &mut dhp);
                }
                dh_next = dhp;
            }
        }
        _ => unreachable!("cache does not match layout"),
    }
}
