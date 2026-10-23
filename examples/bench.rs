//! Headless speed check: scripted bot vs scripted bot.
use claude_tanks::bots::ScriptedBot;
use claude_tanks::sim::Terrain;
use claude_tanks::sim::world::run_match;

fn main() {
    let n = 200;
    let start = std::time::Instant::now();
    let (mut wins, mut ticks) = ([0; 3], 0u64);
    for seed in 0..n {
        let t = Terrain::generate(seed, (seed % 11) as f32 / 10.0);
        let r = run_match(
            t,
            seed % 2 == 1,
            &mut ScriptedBot::new(seed),
            &mut ScriptedBot::new(seed + 7),
        );
        wins[r.winner.map_or(2, |w| w)] += 1;
        ticks += r.ticks as u64;
    }
    let s = start.elapsed().as_secs_f64();
    println!(
        "{n} matches in {s:.2}s ({:.0} ticks/s), wins A/B/draw {wins:?}",
        ticks as f64 / s
    );
}
