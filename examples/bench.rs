//! Headless speed check, single thread: scripted bot vs scripted bot, then each
//! network architecture (random weights) vs the scripted bot.
use claude_tanks::bots::ScriptedBot;
use claude_tanks::nn::{Arch, Brain, Genes, fixed};
use claude_tanks::rng::Rng;
use claude_tanks::sim::Terrain;
use claude_tanks::sim::world::{Controller, run_match};

fn bench(label: &str, make: &dyn Fn(u64) -> Box<dyn Controller>) {
    let n = 200;
    let start = std::time::Instant::now();
    let (mut wins, mut ticks) = ([0; 3], 0u64);
    for seed in 0..n {
        let t = Terrain::generate(seed, (seed % 11) as f32 / 10.0);
        let r = run_match(
            t,
            seed % 2 == 1,
            make(seed).as_mut(),
            &mut ScriptedBot::new(seed + 7),
        );
        wins[r.winner.unwrap_or(2)] += 1;
        ticks += r.ticks as u64;
    }
    let s = start.elapsed().as_secs_f64();
    println!(
        "{label:14} {n} matches in {s:5.2}s ({:>7.0} ticks/s), wins A/B/draw {wins:?}",
        ticks as f64 / s
    );
}

fn main() {
    bench("scripted", &|s| Box::new(ScriptedBot::new(s)));
    for arch in Arch::FIXED {
        let genes = Genes::Fixed(fixed::init_params(arch, &mut Rng::new(1)));
        bench(arch.name(), &|_| Box::new(Brain::new(arch, &genes)));
    }
}
