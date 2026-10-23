//! Headless training: `cargo run --release --example train_cli -- <arch> <method> <generations>`
//! arch: mlp_s | mlp_l | elman | gru | neat, method: ga | neat | ppo
use claude_tanks::nn::Arch;
use claude_tanks::train::{Method, Msg, TrainConfig, spawn};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arch = Arch::ALL
        .into_iter()
        .find(|a| a.short() == args.get(1).map_or("mlp_s", |s| s))
        .expect("arch");
    let method = Method::ALL
        .into_iter()
        .find(|m| m.name().to_lowercase() == args.get(2).map_or("ga", |s| s))
        .expect("method");
    let gens: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);
    let cfg = TrainConfig {
        run_name: format!("cli_{}", TrainConfig::default_run_name(arch, method)),
        arch,
        method,
        max_generations: gens,
        ..Default::default()
    };
    let h = spawn(cfg, None);
    for m in h.msg.iter() {
        match m {
            Msg::Stats(s) => println!(
                "gen {:4} L{} steps {:>11} {:6.1}s best {:+.3} mean {:+.3} vs scripted W{:3.0}% D{:3.0}% L{:3.0}% {:?}",
                s.generation,
                s.level,
                s.steps,
                s.seconds,
                s.best_fitness,
                s.mean_fitness,
                s.vs_scripted.win * 100.0,
                s.vs_scripted.draw * 100.0,
                s.vs_scripted.loss * 100.0,
                s.extra
            ),
            Msg::Log(l) => println!("{l}"),
            Msg::Finished => break,
            Msg::History(_) => {}
        }
    }
}
