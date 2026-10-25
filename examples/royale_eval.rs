//! Robocode-rules scorecard for one model: `royale_eval <model.json> [duels] [label] [results dir]`.
//! Plays one-round duels against each built-in sample-bot port and against the
//! day champion (which was trained under our rules), prints one line, and
//! appends a JSON record to `<results dir>/scores.jsonl` (default `results/robocode`).
use claude_tanks::nn::{Genes, ModelFile};
use claude_tanks::royale::sparring::Sparring;
use claude_tanks::train::eval::{Record, net_pilot, vs_sparring};
use rayon::prelude::*;
use std::io::Write;

const REFERENCE: &str = "models/champions/gru_es_day.json";

fn params(m: &ModelFile) -> Vec<f32> {
    match &m.genes {
        Genes::Fixed(p) => p.clone(),
        Genes::Neat(_) => panic!("fixed-topology models only"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .expect("usage: royale_eval <model.json> [duels] [label] [results dir]");
    let duels: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500);
    let label = args.get(3).cloned().unwrap_or_else(|| path.clone());
    let out_dir = args.get(4).map_or("results/robocode", |s| s.as_str());
    let me = ModelFile::load(path.as_ref()).expect("model");
    let (arch, p) = (me.arch, params(&me));
    let make = || net_pilot(arch, &p, me.keep_state);
    let bots: Vec<(Sparring, Record)> = Sparring::ALL
        .iter()
        .map(|k| (*k, vs_sparring(&make, duels, *k).0))
        .collect();
    // Head-to-head against the reference network.
    let r = ModelFile::load(REFERENCE.as_ref()).expect("reference");
    let (ra, rp) = (r.arch, params(&r));
    let res: Vec<Option<usize>> = (0..duels)
        .into_par_iter()
        .map(|j| {
            let (mut a, mut b) = (
                net_pilot(arch, &p, me.keep_state),
                net_pilot(ra, &rp, r.keep_state),
            );
            let seed = 700_000_000 + j as u64;
            if j % 2 == 0 {
                claude_tanks::royale::duel::duel(seed, a.as_mut(), b.as_mut()).winner
            } else {
                claude_tanks::royale::duel::duel(seed, b.as_mut(), a.as_mut())
                    .winner
                    .map(|w| 1 - w)
            }
        })
        .collect();
    let n = duels as f32;
    let reference = Record {
        win: res.iter().filter(|w| **w == Some(0)).count() as f32 / n,
        draw: res.iter().filter(|w| w.is_none()).count() as f32 / n,
        loss: res.iter().filter(|w| **w == Some(1)).count() as f32 / n,
    };
    let pct = |r: &Record| {
        format!(
            "{:.1}/{:.1}/{:.1}",
            r.win * 100.0,
            r.draw * 100.0,
            r.loss * 100.0
        )
    };
    let mean = Record::mean(&bots.iter().map(|(_, r)| *r).collect::<Vec<_>>());
    let txt: String = bots
        .iter()
        .map(|(k, r)| format!("  vs {} {}", k.name(), pct(r)))
        .collect();
    println!(
        "{label}:{txt}  mean {}  vs day champion {}  (W/D/L %, {duels} duels each)",
        pct(&mean),
        pct(&reference)
    );
    let mut rec = serde_json::json!({
        "label": label,
        "model": path,
        "rules": me.rules,
        "duels": duels,
        "vs_sample_bots": mean,
        "vs_day_champion": reference,
        "time": claude_tanks::train::timestamp(),
    });
    for (k, r) in &bots {
        rec[format!("vs_{}", k.name())] = serde_json::json!(r);
    }
    let dir = std::path::Path::new(out_dir);
    let _ = std::fs::create_dir_all(dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("scores.jsonl"))
    {
        let _ = writeln!(f, "{rec}");
    }
}
