//! The GPU kernels must compute the same network outputs as the CPU code.
//! Skipped (passes trivially) on machines without a GPU adapter.
#![cfg(feature = "gpu")]

use claude_tanks::nn::Arch;
use claude_tanks::nn::fixed::{FixedNet, N_IN, N_OUT, init_params};
use claude_tanks::rng::Rng;

#[test]
fn gpu_matches_cpu_forward() {
    let Some(gpu) = claude_tanks::gpu::get() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut rng = Rng::new(3);
    for arch in Arch::FIXED {
        let sets: Vec<Vec<f32>> = (0..3).map(|_| init_params(arch, &mut rng)).collect();
        // Five rows, mixing parameter sets.
        let row_param = [0u32, 1, 2, 1, 0];
        let refs: Vec<&[f32]> = sets.iter().map(|s| s.as_slice()).collect();
        let batch = gpu.batch(arch, &refs, &row_param);
        let mut cpu: Vec<FixedNet> = row_param
            .iter()
            .map(|&p| FixedNet::new(arch, sets[p as usize].clone()))
            .collect();
        let mut out = vec![0.0; row_param.len() * N_OUT];
        for step in 0..12 {
            let obs: Vec<f32> = (0..row_param.len() * N_IN)
                .map(|_| rng.range(-1.0, 1.0))
                .collect();
            batch.step(&obs, &mut out).unwrap();
            for (r, net) in cpu.iter_mut().enumerate() {
                let want = net.forward(&obs[r * N_IN..(r + 1) * N_IN]);
                for k in 0..N_OUT {
                    let got = out[r * N_OUT + k];
                    assert!(
                        (got - want[k]).abs() < 1e-4,
                        "{arch:?} step {step} row {r} out {k}: gpu {got} cpu {}",
                        want[k]
                    );
                }
            }
        }
    }
}
