//! Plays every bot directory under a folder against one of our networks:
//! `royale_survey <bots folder> <model.json> [rounds] [parallel] [out.jsonl]`.
//! Each bot gets its own server, so several battles run at once. Appends one
//! JSON line per bot to `out.jsonl` (default `results/robocode/survey.jsonl`):
//! whether it connected, then from the bot's side the rounds `won` and `lost`
//! and the damage dealt and taken, and its speed; then `round_seq`, the bot's
//! result round by round (W, L or D), and what our network did: its shots and
//! hits, the enemy's shots seen and hits, and how close it fought. Bots already
//! in the file are skipped, so an interrupted survey resumes.
//!
//! Some robots save what they learned about an opponent in files named after
//! it, and read them in later battles. So that every survey meets robots that
//! know nothing of our network, it plays under a name of its own: the model's
//! name and a random suffix (recorded as `as`).
use claude_tanks::nn::ModelFile;
use claude_tanks::royale::Setup;
use claude_tanks::royale::pilot::{Adapter, PilotReport};
use claude_tanks::royale::server::{
    BattleOptions, BotDir, Entrant, Live, Server, run_battle, sandbox_name,
};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A battle that takes longer than this is stopped and recorded as such.
const BATTLE_LIMIT: Duration = Duration::from_secs(240);

fn survey(bot: &BotDir, model: &ModelFile, name: &str, rounds: u32) -> serde_json::Value {
    let dir = bot.dir.file_name().unwrap().to_string_lossy().to_string();
    let mut rec = serde_json::json!({"dir": dir, "name": bot.name, "as": name});
    let fail = |mut rec: serde_json::Value, why: &str| {
        rec["status"] = why.into();
        rec
    };
    let Ok(server) = Server::start(0) else {
        return fail(rec, "no server");
    };
    let Ok(mut child) = bot.launch(&server.url()) else {
        return fail(rec, "did not start");
    };
    let started = Instant::now();
    let Some(conn) = server.wait_for(&bot.name, &[], Duration::from_secs(45)) else {
        let _ = child.kill();
        let _ = child.wait();
        return fail(rec, "did not connect");
    };
    rec["connect_s"] = started.elapsed().as_secs_f32().into();
    let report = Arc::new(Mutex::new(PilotReport::default()));
    let me = Entrant::Local(Box::new(
        Adapter::for_model(model).with_report(report.clone()),
    ));
    let opts = BattleOptions {
        setup: Setup {
            rounds,
            ..Setup::default()
        },
        tps: None,
        seed: 1,
    };
    let (live, stop, done) = (
        Mutex::new(Live::default()),
        AtomicBool::new(false),
        AtomicBool::new(false),
    );
    let t = Instant::now();
    let res = std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) && t.elapsed() < BATTLE_LIMIT {
                std::thread::sleep(Duration::from_millis(200));
            }
            stop.store(true, Ordering::Relaxed);
        });
        let r = run_battle(
            &server,
            vec![me, Entrant::Remote(conn)],
            vec![name.to_string(), bot.name.clone()],
            &opts,
            &live,
            &stop,
        );
        done.store(true, Ordering::Relaxed);
        r
    });
    let _ = child.kill();
    let _ = child.wait();
    let secs = t.elapsed().as_secs_f32();
    let l = live.lock().unwrap();
    let turns: u32 = l
        .log
        .iter()
        .filter_map(|s| {
            s.split(" in ")
                .nth(1)?
                .split(' ')
                .next()?
                .parse::<u32>()
                .ok()
        })
        .sum();
    rec["seconds"] = secs.into();
    rec["timeouts"] = l.timeouts.into();
    match res {
        Ok(scores) => {
            rec["status"] = "ok".into();
            rec["rounds"] = rounds.into();
            rec["won"] = scores[1].first_places.into();
            rec["lost"] = scores[0].first_places.into();
            rec["damage_dealt"] = scores[1].bullet_damage.into();
            rec["damage_taken"] = scores[0].bullet_damage.into();
            rec["score"] = scores[1].total().into();
            rec["our_score"] = scores[0].total().into();
            rec["turns"] = turns.into();
            if turns > 0 {
                rec["ms_per_turn"] = (secs * 1000.0 / turns as f32).into();
            }
            let seq: String = l
                .log
                .iter()
                .filter_map(|s| {
                    let won_by = s.strip_prefix("Round ")?.split(" won by ").nth(1)?;
                    let won_by = won_by.rsplit_once(" in ")?.0;
                    Some(if won_by == bot.name {
                        'W'
                    } else if won_by == name {
                        'L'
                    } else {
                        'D'
                    })
                })
                .collect();
            rec["round_seq"] = seq.into();
            let r = *report.lock().unwrap();
            rec["our_resolved"] = r.resolved.into();
            rec["our_hits"] = r.hits.into();
            rec["enemy_shots"] = r.enemy_shots.into();
            rec["hit_by"] = r.hit_by.into();
            if r.seen_turns > 0 {
                rec["mean_distance"] = (r.distance / r.seen_turns as f64).into();
                rec["close_share"] = (r.close_turns as f64 / r.seen_turns as f64).into();
            }
            rec
        }
        Err(e) => fail(rec, &e),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = args
        .get(1)
        .expect("usage: royale_survey <bots folder> <model.json> [rounds] [parallel] [out.jsonl]");
    let model = ModelFile::load(args.get(2).expect("model").as_ref()).expect("model");
    let rounds: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let parallel: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(8);
    let out = args
        .get(5)
        .map_or("results/robocode/survey.jsonl", |s| s.as_str());
    let done: std::collections::HashSet<String> = std::fs::read_to_string(out)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v["dir"].as_str().map(String::from))
        .collect();
    let todo: Vec<BotDir> = BotDir::scan(root.as_ref())
        .into_iter()
        .filter(|b| !done.contains(&*b.dir.file_name().unwrap().to_string_lossy()))
        .collect();
    eprintln!(
        "{} bots to play ({} already done), {rounds} rounds each, {parallel} at a time, sandbox: {}",
        todo.len(),
        done.len(),
        sandbox_name().unwrap_or("none")
    );
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let me = format!("{}-{:06x}", model.name, nanos & 0xff_ffff);
    let queue = Mutex::new(todo.into_iter());
    let file = Mutex::new(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(out)
            .expect("output file"),
    );
    std::thread::scope(|s| {
        for _ in 0..parallel {
            s.spawn(|| {
                loop {
                    let Some(bot) = queue.lock().unwrap().next() else {
                        break;
                    };
                    let rec = survey(&bot, &model, &me, rounds);
                    eprintln!(
                        "{:40} {:16} {}-{}",
                        rec["dir"].as_str().unwrap_or(""),
                        rec["status"].as_str().unwrap_or(""),
                        rec["won"],
                        rec["lost"]
                    );
                    let _ = writeln!(file.lock().unwrap(), "{rec}");
                }
            });
        }
    });
}
