//! Throughput of training battles against real bots:
//! `royale_pool <model.json> <bot dir> ... [battles=N] [rounds=R] [slots=S]`.
//! Keeps S processes of every bot running and plays N battles of R rounds
//! against each through them, the way training does.
use claude_tanks::nn::{Genes, ModelFile};
use claude_tanks::royale::pool::RobotPool;
use claude_tanks::royale::server::BotDir;
use claude_tanks::train::eval::net_pilot;
use std::sync::Mutex;
use std::time::Instant;

fn main() {
    let (mut battles, mut rounds, mut slots) = (10usize, 3u32, 2usize);
    let mut paths = Vec::new();
    for a in std::env::args().skip(1) {
        match a.split_once('=') {
            Some(("battles", v)) => battles = v.parse().unwrap(),
            Some(("rounds", v)) => rounds = v.parse().unwrap(),
            Some(("slots", v)) => slots = v.parse().unwrap(),
            _ => paths.push(a),
        }
    }
    let model = ModelFile::load(paths[0].as_ref()).expect("model");
    let Genes::Fixed(p) = &model.genes else {
        panic!("fixed-topology models only")
    };
    let bots: Vec<BotDir> = paths[1..]
        .iter()
        .map(|d| BotDir::read(d.as_ref()).unwrap_or_else(|| panic!("not a bot directory: {d}")))
        .collect();
    let n = bots.len();
    let t = Instant::now();
    let pool = RobotPool::start(bots, slots).expect("pool");
    let jobs = Mutex::new((0..battles * n).map(|j| (j % n, j as u64)));
    let results = Mutex::new(vec![(0u32, 0u32, 0u32, 0u32); n]);
    std::thread::scope(|s| {
        for _ in 0..pool.slots() {
            s.spawn(|| {
                loop {
                    let Some((bot, seed)) = jobs.lock().unwrap().next() else {
                        break;
                    };
                    let r = pool.battle(bot, net_pilot(model.arch, p), rounds, seed);
                    let mut res = results.lock().unwrap();
                    match r {
                        Some(r) => {
                            res[bot].0 += r.won;
                            res[bot].1 += r.lost;
                            res[bot].2 += r.rounds;
                        }
                        None => res[bot].3 += 1,
                    }
                }
            });
        }
    });
    let secs = t.elapsed().as_secs_f32();
    let mut played = 0;
    for (b, (won, lost, r, failed)) in pool.bots().iter().zip(results.into_inner().unwrap()) {
        println!(
            "{:30} won {won:4} lost {lost:4} of {r:4} rounds, {failed} failed battles",
            b.name
        );
        played += r;
    }
    println!(
        "{played} rounds in {secs:.1}s with {} processes: {:.1} rounds/s",
        pool.slots(),
        played as f32 / secs
    );
}
