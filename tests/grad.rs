use claude_tanks::nn::Arch;
use claude_tanks::nn::fixed::{Layout, N_IN, N_OUT, init_params};
use claude_tanks::nn::grad::{seq_backward, seq_forward};
use claude_tanks::rng::Rng;

/// Loss = sum_t sum_k c[t][k] * out[t][k]; compare analytic and numeric gradients.
#[test]
fn bptt_matches_finite_differences() {
    let mut rng = Rng::new(11);
    for arch in Arch::FIXED {
        let (layout, n) = Layout::new(arch);
        let p: Vec<f32> = init_params(arch, &mut rng)
            .into_iter()
            .map(|v| v * 2.0)
            .collect();
        let h0: Vec<f32> = (0..layout.hidden()).map(|_| rng.range(-0.5, 0.5)).collect();
        let xs: Vec<[f32; N_IN]> = (0..5)
            .map(|_| std::array::from_fn(|_| rng.range(-1.0, 1.0)))
            .collect();
        let c: Vec<[f32; N_OUT]> = (0..5)
            .map(|_| std::array::from_fn(|_| rng.range(-1.0, 1.0)))
            .collect();
        let loss = |p: &[f32]| -> f64 {
            let (outs, _) = seq_forward(&layout, p, &h0, &xs);
            outs.iter()
                .zip(&c)
                .map(|(o, c)| {
                    o.iter()
                        .zip(c)
                        .map(|(a, b)| (*a as f64) * (*b as f64))
                        .sum::<f64>()
                })
                .sum()
        };
        let (outs, cache) = seq_forward(&layout, &p, &h0, &xs);
        let mut g = vec![0.0; n];
        seq_backward(&layout, &p, &xs, &outs, &cache, &c, &mut g);
        let mut worst = 0.0f64;
        for _ in 0..200 {
            let i = rng.below(n);
            let eps = 1e-2;
            let mut pp = p.clone();
            pp[i] += eps;
            let lp = loss(&pp);
            pp[i] -= 2.0 * eps;
            let lm = loss(&pp);
            let num = (lp - lm) / (2.0 * eps as f64);
            let err = (num - g[i] as f64).abs() / (num.abs() + g[i].abs() as f64 + 1e-2);
            worst = worst.max(err);
        }
        assert!(worst < 0.05, "{arch:?}: worst relative error {worst}");
    }
}
