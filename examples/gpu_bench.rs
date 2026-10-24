//! One ES-sized generation (96 candidates x 16 matches, 70% scripted opponents,
//! the rest networks) evaluated on the CPU and then on the GPU, per architecture.
use claude_tanks::nn::{Arch, Genes, fixed};
use claude_tanks::rng::Rng;
use claude_tanks::train::eval::{Opponent, Shaping, eval_fixed, terrains};
use std::sync::Arc;

fn main() {
    #[cfg(feature = "gpu")]
    if std::env::args().nth(1).as_deref() == Some("latency") {
        step_latency();
        return;
    }
    #[cfg(feature = "gpu")]
    println!("GPU: {}", claude_tanks::gpu::status());
    let mut rng = Rng::new(5);
    for arch in [Arch::MlpL, Arch::Gru, Arch::GruL] {
        let base = fixed::init_params(arch, &mut rng);
        let members: Vec<Vec<f32>> = (0..96)
            .map(|_| base.iter().map(|v| v + 0.01 * rng.gauss()).collect())
            .collect();
        let opps: Vec<Opponent> = (0..16)
            .map(|m| {
                if m % 10 < 7 {
                    Opponent::Scripted(m as u64, 3 + (m % 2) as u8)
                } else {
                    Opponent::Net(arch, Arc::new(Genes::Fixed(base.clone())))
                }
            })
            .collect();
        let ts = terrains(&mut rng, 16, (0.0, 1.0));
        let shaping = Shaping { weight: 0.2 };
        for gpu in [false, true] {
            let t = std::time::Instant::now();
            let out = eval_fixed(arch, &members, gpu, &opps, &ts, shaping);
            let s = t.elapsed().as_secs_f64();
            println!(
                "{:14} {}: {:5.2}s  {:>9.0} ticks/s  mean fitness {:+.3}",
                arch.name(),
                if gpu { "GPU" } else { "CPU" },
                s,
                out.ticks as f64 / s,
                out.fitness.iter().sum::<f32>() / out.fitness.len() as f32
            );
        }
    }
}

#[cfg(feature = "gpu")]
#[allow(dead_code)]
fn step_latency() {
    use claude_tanks::nn::fixed::{N_IN, N_OUT};
    let Some(g) = claude_tanks::gpu::get() else {
        return;
    };
    let mut rng = Rng::new(1);
    let p = fixed::init_params(Arch::Gru, &mut rng);
    for rows in [1usize, 512, 2048] {
        let row_param = vec![0u32; rows];
        let b = g.batch(Arch::Gru, &[p.as_slice()], &row_param);
        let obs = vec![0.1f32; rows * N_IN];
        let mut out = vec![0.0f32; rows * N_OUT];
        b.step(&obs, &mut out).unwrap();
        let t = std::time::Instant::now();
        for _ in 0..200 {
            b.step(&obs, &mut out).unwrap();
        }
        println!(
            "GPU step, {rows} rows: {:.3} ms",
            t.elapsed().as_secs_f64() * 1000.0 / 200.0
        );
    }
}
