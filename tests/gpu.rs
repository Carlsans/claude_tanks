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
    for arch in Arch::FIXED
        .into_iter()
        .filter(|a| claude_tanks::gpu::supports(*a))
    {
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

/// GPU population evaluation against every kind of opponent (scripted, same
/// architecture, other architecture) agrees with the CPU path.
#[test]
fn gpu_population_matches_cpu_with_mixed_opponents() {
    use claude_tanks::nn::Genes;
    use claude_tanks::train::eval::{Opponent, Shaping, eval_fixed, eval_population_gpu, terrains};
    use std::sync::Arc;
    let Some(g) = claude_tanks::gpu::get() else {
        eprintln!("no GPU adapter: skipped");
        return;
    };
    let mut rng = Rng::new(5);
    let arch = Arch::Gru;
    let members: Vec<Vec<f32>> = (0..6).map(|_| init_params(arch, &mut rng)).collect();
    let opps = vec![
        Opponent::Scripted(1, 3),
        Opponent::Net(arch, Arc::new(Genes::Fixed(init_params(arch, &mut rng)))),
        Opponent::Net(
            Arch::MlpL,
            Arc::new(Genes::Fixed(init_params(Arch::MlpL, &mut rng))),
        ),
        Opponent::Scripted(2, 4),
    ];
    let ts = terrains(&mut rng, opps.len(), (0.0, 1.0));
    let shaping = Shaping { weight: 0.3 };
    let gpu = eval_population_gpu(g, arch, &members, &opps, &ts, shaping).unwrap();
    let cpu = eval_fixed(arch, &members, false, &opps, &ts, shaping);
    // Matches are chaotic: a 1e-6 difference can change one of them, so most
    // (not all) members must agree.
    let same = gpu
        .fitness
        .iter()
        .zip(&cpu.fitness)
        .filter(|(g, c)| (*g - *c).abs() < 1e-3)
        .count();
    assert!(
        same >= members.len() - 2,
        "gpu {:?} cpu {:?}",
        gpu.fitness,
        cpu.fitness
    );
}
