//! Round-robin between saved models and the two scripted bots:
//! `tournament <matches per pairing> <out stem> <label>=<model.json> ...`.
//! Writes `<out stem>.json` (labels, score matrix, Elo) and `<out stem>.md`.
//! Elo is anchored at 1000 for the veteran.
use claude_tanks::bots::ScriptedBot;
use claude_tanks::bots::scripted::{MAX_LEVEL, VETERAN};
use claude_tanks::nn::ModelFile;
use claude_tanks::sim::world::Controller;
use claude_tanks::train::benchmark::fit_elo;
use claude_tanks::train::eval::head_to_head;
use std::fmt::Write;

enum Pilot {
    Model(ModelFile),
    Bot(u8),
}

impl Pilot {
    fn make(&self, seed: u64) -> Box<dyn Controller> {
        match self {
            Pilot::Model(m) => Box::new(m.brain()),
            Pilot::Bot(l) => Box::new(ScriptedBot::with_level(seed, *l)),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: usize = args.get(1).and_then(|s| s.parse().ok()).expect("matches");
    let out = args.get(2).expect("output stem");
    let mut labels = Vec::new();
    let mut pilots = Vec::new();
    for a in &args[3..] {
        let (label, path) = a.split_once('=').expect("label=model.json");
        labels.push(label.to_string());
        pilots.push(Pilot::Model(ModelFile::load(path.as_ref()).expect(path)));
    }
    labels.push("Ace (scripted)".into());
    pilots.push(Pilot::Bot(MAX_LEVEL));
    labels.push("Veteran (scripted)".into());
    pilots.push(Pilot::Bot(VETERAN));

    let n = pilots.len();
    let mut score = vec![vec![0.5f32; n]; n];
    let mut wdl = vec![vec![[0.0f32; 3]; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let seed = 20_000_000 + (i * n + j) as u64 * 10_000;
            let r = head_to_head(
                &|| pilots[i].make(seed + 1),
                &|| pilots[j].make(seed + 2),
                matches,
                seed,
            );
            score[i][j] = r.win + 0.5 * r.draw;
            score[j][i] = r.loss + 0.5 * r.draw;
            wdl[i][j] = [r.win, r.draw, r.loss];
            wdl[j][i] = [r.loss, r.draw, r.win];
            eprintln!("{} vs {}: {:.3}", labels[i], labels[j], score[i][j]);
        }
    }
    let elo = fit_elo(&score);
    let mut rank: Vec<usize> = (0..n).collect();
    rank.sort_by(|&a, &b| elo[b].total_cmp(&elo[a]));

    let mut md = String::new();
    writeln!(md, "| Rank | Pilot | Elo | Mean score |").unwrap();
    writeln!(md, "|---|---|---|---|").unwrap();
    for (pos, &i) in rank.iter().enumerate() {
        let mean = (0..n).filter(|&j| j != i).map(|j| score[i][j]).sum::<f32>() / (n - 1) as f32;
        writeln!(
            md,
            "| {} | {} | {:.0} | {:.1}% |",
            pos + 1,
            labels[i],
            elo[i],
            mean * 100.0
        )
        .unwrap();
    }
    writeln!(
        md,
        "\n{matches} matches per pairing, sides alternate, densities 0 to 1. Elo anchored at 1000 for the veteran."
    )
    .unwrap();
    print!("{md}");
    std::fs::write(format!("{out}.md"), &md).unwrap();
    let json = serde_json::json!({
        "matches": matches,
        "labels": labels,
        "elo": elo,
        "score": score,
        "wdl": wdl,
    });
    std::fs::write(format!("{out}.json"), json.to_string()).unwrap();
}
