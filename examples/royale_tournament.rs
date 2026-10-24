//! Round-robin under Robocode rules between saved models and the sample-bot
//! ports: `royale_tournament <duels per pairing> <out stem> <label>=<model.json> ...`.
//! Writes `<out stem>.json` (labels, score matrix, Elo) and `<out stem>.md`.
//! Elo is anchored at 1000 for SpinBot (the last contestant).
use claude_tanks::nn::{Genes, ModelFile};
use claude_tanks::royale::pilot::Pilot;
use claude_tanks::royale::sparring::Sparring;
use claude_tanks::train::benchmark::fit_elo;
use claude_tanks::train::eval::net_pilot;
use rayon::prelude::*;
use std::fmt::Write;

enum Entrant {
    Model(ModelFile),
    Port(Sparring),
}

impl Entrant {
    fn make(&self) -> Box<dyn Pilot> {
        match self {
            Entrant::Model(m) => match &m.genes {
                Genes::Fixed(p) => net_pilot(m.arch, p),
                Genes::Neat(_) => panic!("fixed-topology models only"),
            },
            Entrant::Port(k) => Box::new(k.pilot()),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let duels: usize = args.get(1).and_then(|s| s.parse().ok()).expect("duels");
    let out = args.get(2).expect("output stem");
    let mut labels = Vec::new();
    let mut entrants = Vec::new();
    for a in &args[3..] {
        let (label, path) = a.split_once('=').expect("label=model.json");
        labels.push(label.to_string());
        entrants.push(Entrant::Model(ModelFile::load(path.as_ref()).expect(path)));
    }
    // SpinBot last, as the Elo anchor.
    for k in Sparring::ALL.iter().rev() {
        labels.push(format!("{} (port)", k.name()));
        entrants.push(Entrant::Port(*k));
    }

    let n = entrants.len();
    let mut score = vec![vec![0.5f32; n]; n];
    let mut wdl = vec![vec![[0.0f32; 3]; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let seed = 40_000_000 + (i * n + j) as u64 * 10_000;
            let res: Vec<Option<usize>> = (0..duels)
                .into_par_iter()
                .map(|d| {
                    let (mut a, mut b) = (entrants[i].make(), entrants[j].make());
                    let s = seed + d as u64;
                    // Alternate which side each pilot starts on.
                    if d % 2 == 0 {
                        claude_tanks::royale::duel::duel(s, a.as_mut(), b.as_mut()).winner
                    } else {
                        claude_tanks::royale::duel::duel(s, b.as_mut(), a.as_mut())
                            .winner
                            .map(|w| 1 - w)
                    }
                })
                .collect();
            let frac =
                |w: Option<usize>| res.iter().filter(|r| **r == w).count() as f32 / duels as f32;
            let (win, draw, loss) = (frac(Some(0)), frac(None), frac(Some(1)));
            score[i][j] = win + 0.5 * draw;
            score[j][i] = loss + 0.5 * draw;
            wdl[i][j] = [win, draw, loss];
            wdl[j][i] = [loss, draw, win];
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
        "\n{duels} one-round duels per pairing under Robocode rules (800 x 600), sides alternate. \
         Elo anchored at 1000 for SpinBot; scores are clamped to 2-98% for the fit, \
         so ratings far above the ports are compressed."
    )
    .unwrap();
    print!("{md}");
    std::fs::write(format!("{out}.md"), &md).unwrap();
    let json = serde_json::json!({
        "duels": duels,
        "labels": labels,
        "elo": elo,
        "score": score,
        "wdl": wdl,
    });
    std::fs::write(format!("{out}.json"), json.to_string()).unwrap();
}
