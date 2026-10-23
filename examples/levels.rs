//! How the full scripted bot fares against each curriculum level, by maze density.
use claude_tanks::bots::scripted::{LEVELS, ScriptedBot};
use claude_tanks::sim::Terrain;
use claude_tanks::sim::world::run_match;

fn main() {
    for (lvl, name) in LEVELS.iter().enumerate() {
        for d in [0.0, 0.3, 0.6, 1.0] {
            let mut w = [0; 3];
            for s in 0..200u64 {
                let r = run_match(
                    Terrain::generate(s, d),
                    s % 2 == 1,
                    &mut ScriptedBot::new(s),
                    &mut ScriptedBot::with_level(s + 5, lvl as u8),
                );
                w[r.winner.unwrap_or(2)] += 1;
            }
            println!(
                "vs {name:13} density {d:.1}: W {} L {} D {}",
                w[0], w[1], w[2]
            );
        }
    }
}
