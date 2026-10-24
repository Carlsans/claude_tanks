//! Scorecard for one model: `evaluate <model.json> [matches] [label] [results dir]`.
//! Plays the veteran, the ace, the reference model, the current champion and
//! the stronger bots (hunter, brawler, stalker) on fresh terrains (never used in training evaluation), prints one line, and
//! appends a JSON record to `<results dir>/scores.jsonl` (default `results/night`).
use claude_tanks::bots::ScriptedBot;
use claude_tanks::bots::scripted::{ACE, LEVELS, MAX_LEVEL, VETERAN};
use claude_tanks::nn::ModelFile;
use claude_tanks::sim::Terrain;
use claude_tanks::sim::world::{Controller, run_match};
use claude_tanks::train::eval::Record;
use rayon::prelude::*;
use std::io::Write;

const REFERENCE: &str = "models/champions/gru_es_40min.json";
const CHAMPION: &str = "models/champions/gru_es_night.json";

/// `matches` games of `me` against the controller built by `op(j)`; sides alternate.
fn play(
    me: &ModelFile,
    op: &(dyn Fn(u64) -> Box<dyn Controller> + Sync),
    matches: u64,
    seed: u64,
) -> Record {
    let res: Vec<Option<usize>> = (0..matches)
        .into_par_iter()
        .map(|j| {
            let t = Terrain::generate(seed + j, ((j / 2) % 11) as f32 / 10.0);
            run_match(t, j % 2 == 1, &mut me.brain(), op(seed + j).as_mut()).winner
        })
        .collect();
    let n = matches as f32;
    Record {
        win: res.iter().filter(|w| **w == Some(0)).count() as f32 / n,
        draw: res.iter().filter(|w| w.is_none()).count() as f32 / n,
        loss: res.iter().filter(|w| **w == Some(1)).count() as f32 / n,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .expect("usage: evaluate <model.json> [matches] [label] [results dir]");
    let matches: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let label = args.get(3).cloned().unwrap_or_else(|| path.clone());
    let me = ModelFile::load(path.as_ref()).expect("model");
    let out_dir = args.get(4).map_or("results/night", |s| s.as_str());
    let reference = ModelFile::load(REFERENCE.as_ref()).expect("reference model");
    let champion = ModelFile::load(CHAMPION.as_ref()).expect("champion model");

    let vet = play(
        &me,
        &|s| Box::new(ScriptedBot::with_level(s, VETERAN)),
        matches,
        7_000_000,
    );
    let ace = play(
        &me,
        &|s| Box::new(ScriptedBot::with_level(s, ACE)),
        matches,
        8_000_000,
    );
    let refr = play(&me, &|_| Box::new(reference.brain()), matches, 9_000_000);
    let champ = play(&me, &|_| Box::new(champion.brain()), matches, 10_000_000);
    let tops: Vec<(u8, Record)> = (ACE + 1..=MAX_LEVEL)
        .map(|l| {
            let r = play(
                &me,
                &|s| Box::new(ScriptedBot::with_level(s, l)),
                matches,
                10_000_000 + l as u64 * 1_000_000,
            );
            (l, r)
        })
        .collect();

    let pct = |r: &Record| {
        format!(
            "{:.1}/{:.1}/{:.1}",
            r.win * 100.0,
            r.draw * 100.0,
            r.loss * 100.0
        )
    };
    let top_txt: String = tops
        .iter()
        .map(|(l, r)| format!("  vs {} {}", LEVELS[*l as usize], pct(r)))
        .collect();
    println!(
        "{label}: vs veteran {}  vs ace {}  vs reference {}  vs champion {}{top_txt}  (W/D/L %, {matches} matches each)",
        pct(&vet),
        pct(&ace),
        pct(&refr),
        pct(&champ)
    );
    let mut rec = serde_json::json!({
        "label": label,
        "model": path,
        "arch": me.arch.name(),
        "method": me.method,
        "generation": me.generation,
        "matches": matches,
        "vs_veteran": vet,
        "vs_ace": ace,
        "vs_reference": refr,
        "vs_champion": champ,
        "time": claude_tanks::train::timestamp(),
    });
    for (l, r) in &tops {
        rec[format!("vs_{}", LEVELS[*l as usize])] = serde_json::json!(r);
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
