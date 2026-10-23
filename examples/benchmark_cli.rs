//! Headless architecture benchmark: `cargo run --release --example benchmark_cli -- <M steps per entry> [matches]`
use claude_tanks::train::benchmark::{BenchConfig, BenchMsg, entry_name, spawn_benchmark};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let msteps: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
    let matches: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
    let cfg = BenchConfig {
        budget_steps: msteps * 1_000_000,
        matches,
        ..Default::default()
    };
    let entries = cfg.entries.clone();
    let h = spawn_benchmark(cfg);
    for m in h.msg.iter() {
        match m {
            BenchMsg::Entry(k) => println!("== {}", entry_name(entries[k].0, entries[k].1)),
            BenchMsg::Stats(_, s) if s.generation % 20 == 0 => println!(
                "   gen {:4} {:6.1} Msteps {:5.0}s level {} vs veteran W{:3.0}% L{:3.0}%",
                s.generation,
                s.steps as f64 / 1e6,
                s.seconds,
                s.level,
                s.vs_scripted.win * 100.0,
                s.vs_scripted.loss * 100.0
            ),
            BenchMsg::Log(l) if !l.contains("new best") => println!("   {l}"),
            BenchMsg::Results(r) => println!("\n{}", r.markdown()),
            BenchMsg::Finished => break,
            _ => {}
        }
    }
}
