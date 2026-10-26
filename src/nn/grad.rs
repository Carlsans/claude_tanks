//! Forward-with-cache and backpropagation (through time) for fixed-topology nets.
//! Used by PPO; parameter layout is identical to `FixedNet`.

use super::fixed::{Layout, N_IN, N_OUT, clock_decay, matvec_add, sigmoid};

/// Activations kept for the backward pass over a sequence.
pub enum SeqCache {
    /// Per step: activations of every layer (input first, output last).
    Mlp(Vec<Vec<Vec<f32>>>),
    /// Per step: (h_prev, h).
    Elman(Vec<(Vec<f32>, Vec<f32>)>),
    Gru(Vec<GruStep>),
    GruClock(Vec<GruClockStep>),
}

pub struct GruStep {
    h_prev: Vec<f32>,
    z: Vec<f32>,
    r: Vec<f32>,
    n: Vec<f32>,
    h: Vec<f32>,
}

pub struct GruClockStep {
    gru: GruStep,
    c_prev: Vec<f32>,
    /// Clock writer output (after tanh).
    a: Vec<f32>,
    c: Vec<f32>,
}

/// Offsets of a GRU's gate weights.
#[derive(Clone, Copy)]
struct Gates<'a> {
    h: usize,
    wx: &'a [usize; 3],
    wh: &'a [usize; 3],
    b: &'a [usize; 3],
}

/// One GRU update from gate input `u` (the observation, or more).
fn gru_step(p: &[f32], gt: Gates, u: &[f32], h_prev: Vec<f32>) -> GruStep {
    let Gates { h, wx, wh, b } = gt;
    let nu = u.len();
    let mut z = p[b[0]..b[0] + h].to_vec();
    let mut r = p[b[1]..b[1] + h].to_vec();
    let mut n = p[b[2]..b[2] + h].to_vec();
    matvec_add(&p[wx[0]..wx[0] + h * nu], u, &mut z);
    matvec_add(&p[wh[0]..wh[0] + h * h], &h_prev, &mut z);
    matvec_add(&p[wx[1]..wx[1] + h * nu], u, &mut r);
    matvec_add(&p[wh[1]..wh[1] + h * h], &h_prev, &mut r);
    z.iter_mut().for_each(|v| *v = sigmoid(*v));
    r.iter_mut().for_each(|v| *v = sigmoid(*v));
    let rh: Vec<f32> = r.iter().zip(&h_prev).map(|(a, b)| a * b).collect();
    matvec_add(&p[wx[2]..wx[2] + h * nu], u, &mut n);
    matvec_add(&p[wh[2]..wh[2] + h * h], &rh, &mut n);
    n.iter_mut().for_each(|v| *v = v.tanh());
    let hn = (0..h)
        .map(|j| (1.0 - z[j]) * n[j] + z[j] * h_prev[j])
        .collect();
    GruStep {
        h_prev,
        z,
        r,
        n,
        h: hn,
    }
}

/// Backward through one GRU step given dL/dh: accumulates the parameter
/// gradients (and dL/du into `du` when given) and returns dL/dh_prev.
fn gru_backward(
    p: &[f32],
    gt: Gates,
    u: &[f32],
    s: &GruStep,
    dh: &[f32],
    g: &mut [f32],
    mut du: Option<&mut [f32]>,
) -> Vec<f32> {
    let Gates { h, wx, wh, b } = gt;
    let nu = u.len();
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
    outer_add(&mut g[wx[2]..wx[2] + h * nu], &dn_pre, u);
    outer_add(&mut g[wh[2]..wh[2] + h * h], &dn_pre, &rh);
    if let Some(du) = du.as_deref_mut() {
        matvec_t_add(&p[wx[2]..wx[2] + h * nu], &dn_pre, du);
    }
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
        outer_add(&mut g[wx[k]..wx[k] + h * nu], d, u);
        outer_add(&mut g[wh[k]..wh[k] + h * h], d, &s.h_prev);
        matvec_t_add(&p[wh[k]..wh[k] + h * h], d, &mut dhp);
        if let Some(du) = du.as_deref_mut() {
            matvec_t_add(&p[wx[k]..wx[k] + h * nu], d, du);
        }
    }
    dhp
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
            ..
        } => {
            let h = *h;
            let gt = Gates { h, wx, wh, b };
            let mut hp = h0.to_vec();
            let mut cache = Vec::with_capacity(xs.len());
            for x in xs {
                let s = gru_step(p, gt, x, hp);
                let mut o = p[*bo..*bo + N_OUT].to_vec();
                matvec_add(&p[*wo..*wo + N_OUT * h], &s.h, &mut o);
                o.iter_mut().for_each(|v| *v = v.tanh());
                outs.push(to_out(&o));
                hp = s.h.clone();
                cache.push(s);
            }
            (outs, SeqCache::Gru(cache))
        }
        Layout::Moe { .. } => unreachable!("no gradients for the mixture of experts"),
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
            let gt = Gates { h, wx, wh, b };
            let decay = clock_decay(k);
            let (mut hp, mut cp) = (h0[..h].to_vec(), h0[h..].to_vec());
            let mut cache = Vec::with_capacity(xs.len());
            for x in xs {
                let u: Vec<f32> = x.iter().chain(&cp).copied().collect();
                let s = gru_step(p, gt, &u, hp);
                let v: Vec<f32> = x.iter().chain(&s.h).copied().collect();
                let mut a = p[*bc..*bc + k].to_vec();
                matvec_add(&p[*wc..*wc + k * (N_IN + h)], &v, &mut a);
                a.iter_mut().for_each(|v| *v = v.tanh());
                let c: Vec<f32> = (0..k)
                    .map(|j| decay[j] * cp[j] + (1.0 - decay[j]) * a[j])
                    .collect();
                let hc: Vec<f32> = s.h.iter().chain(&c).copied().collect();
                let mut o = p[*bo..*bo + N_OUT].to_vec();
                matvec_add(&p[*wo..*wo + N_OUT * (h + k)], &hc, &mut o);
                matvec_add(&p[*ws..*ws + N_OUT * N_IN], x, &mut o);
                o.iter_mut().for_each(|v| *v = v.tanh());
                outs.push(to_out(&o));
                hp = s.h.clone();
                cache.push(GruClockStep {
                    gru: s,
                    c_prev: cp,
                    a,
                    c: c.clone(),
                });
                cp = c;
            }
            (outs, SeqCache::GruClock(cache))
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
                ..
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

                dh_next = gru_backward(p, Gates { h, wx, wh, b }, &xs[t], s, &dh, g, None);
            }
        }
        (
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
            },
            SeqCache::GruClock(c),
        ) => {
            let (h, k) = (*h, *k);
            let decay = clock_decay(k);
            let mut dh_next = vec![0.0; h];
            let mut dc_next = vec![0.0; k];
            for t in (0..c.len()).rev() {
                let s = &c[t];
                let x = &xs[t];
                let dpre_o: Vec<f32> = (0..N_OUT)
                    .map(|j| d_out[t][j] * (1.0 - outs[t][j] * outs[t][j]))
                    .collect();
                for j in 0..N_OUT {
                    g[*bo + j] += dpre_o[j];
                }
                let hc: Vec<f32> = s.gru.h.iter().chain(&s.c).copied().collect();
                outer_add(&mut g[*wo..*wo + N_OUT * (h + k)], &dpre_o, &hc);
                outer_add(&mut g[*ws..*ws + N_OUT * N_IN], &dpre_o, x);
                let mut dhc: Vec<f32> = dh_next.iter().chain(&dc_next).copied().collect();
                matvec_t_add(&p[*wo..*wo + N_OUT * (h + k)], &dpre_o, &mut dhc);
                let (dh, dc) = dhc.split_at_mut(h);
                // Clocks.
                let da: Vec<f32> = (0..k)
                    .map(|j| dc[j] * (1.0 - decay[j]) * (1.0 - s.a[j] * s.a[j]))
                    .collect();
                let mut dcp: Vec<f32> = (0..k).map(|j| dc[j] * decay[j]).collect();
                for j in 0..k {
                    g[*bc + j] += da[j];
                }
                let v: Vec<f32> = x.iter().chain(&s.gru.h).copied().collect();
                outer_add(&mut g[*wc..*wc + k * (N_IN + h)], &da, &v);
                let mut dv = vec![0.0; N_IN + h];
                matvec_t_add(&p[*wc..*wc + k * (N_IN + h)], &da, &mut dv);
                for j in 0..h {
                    dh[j] += dv[N_IN + j];
                }
                // GRU over [x, previous clocks].
                let u: Vec<f32> = x.iter().chain(&s.c_prev).copied().collect();
                let mut du = vec![0.0; N_IN + k];
                dh_next = gru_backward(p, Gates { h, wx, wh, b }, &u, &s.gru, dh, g, Some(&mut du));
                for j in 0..k {
                    dcp[j] += du[N_IN + j];
                }
                dc_next = dcp;
            }
        }
        _ => unreachable!("cache does not match layout"),
    }
}

/// Hidden state after the last step of a cached sequence (empty for MLPs).
pub fn last_hidden(cache: &SeqCache) -> Vec<f32> {
    match cache {
        SeqCache::Mlp(_) => Vec::new(),
        SeqCache::Elman(c) => c.last().map(|s| s.1.clone()).unwrap_or_default(),
        SeqCache::Gru(c) => c.last().map(|s| s.h.clone()).unwrap_or_default(),
        SeqCache::GruClock(c) => c
            .last()
            .map(|s| s.gru.h.iter().chain(&s.c).copied().collect())
            .unwrap_or_default(),
    }
}
