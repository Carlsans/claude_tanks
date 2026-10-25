//! Tank Royale battle from the command line:
//! `royale [rounds=N] [tps=T] [port=P] [seed=S] <entrant> <entrant> ...`
//! An entrant is a bot directory (started like the official booter does),
//! `model:<file.json>` (one of our networks), `bot:<level>` (one of our
//! scripted bots, by name or number), `spar:<name>` (a built-in port of a
//! sample bot: SpinBot, Walls, TrackFire, RamFire, Crazy) or `wait:<name>` (a
//! bot that connects by itself). Prints each round's winner and the final scores.
use claude_tanks::bots::ScriptedBot;
use claude_tanks::bots::scripted::LEVELS;
use claude_tanks::nn::ModelFile;
use claude_tanks::royale::Setup;
use claude_tanks::royale::pilot::{Adapter, Driver};
use claude_tanks::royale::server::{
    BattleOptions, BotDir, DEFAULT_PORT, Entrant, Live, Server, ranks, run_battle,
};
use claude_tanks::royale::sparring::Sparring;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn main() {
    let mut setup = Setup::default();
    let (mut tps, mut port, mut seed) = (None, DEFAULT_PORT, 1u64);
    let mut specs = Vec::new();
    for a in std::env::args().skip(1) {
        match a.split_once('=') {
            Some(("rounds", v)) => setup.rounds = v.parse().unwrap(),
            Some(("tps", v)) => tps = Some(v.parse().unwrap()),
            Some(("port", v)) => port = v.parse().unwrap(),
            Some(("seed", v)) => seed = v.parse().unwrap(),
            _ => specs.push(a),
        }
    }
    let server = Server::start(port).expect("cannot listen");
    println!("Server on {}", server.url());
    let mut children = Vec::new();
    let (mut entrants, mut names, mut taken) = (Vec::new(), Vec::new(), Vec::new());
    for s in &specs {
        if let Some(path) = s.strip_prefix("model:") {
            let m = ModelFile::load(path.as_ref()).expect(path);
            names.push(m.name.clone());
            entrants.push(Entrant::Local(Box::new(Adapter::for_model(&m))));
        } else if let Some(n) = s.strip_prefix("spar:") {
            let k = Sparring::ALL
                .into_iter()
                .find(|k| k.name().eq_ignore_ascii_case(n))
                .unwrap_or_else(|| panic!("unknown sparring bot {n}"));
            names.push(format!("{} (port)", k.name()));
            entrants.push(Entrant::Local(Box::new(k.pilot())));
        } else if let Some(l) = s.strip_prefix("bot:") {
            let level = LEVELS
                .iter()
                .position(|n| *n == l)
                .map_or_else(|| l.parse().expect(l), |p| p as u8);
            names.push(format!("{} (ours)", LEVELS[level as usize]));
            let bot = ScriptedBot::with_level(seed ^ entrants.len() as u64, level);
            entrants.push(Entrant::Local(Box::new(Adapter::new(Driver::Scripted(
                bot,
            )))));
        } else {
            let name = match s.strip_prefix("wait:") {
                Some(n) => n.to_string(),
                None => {
                    let d = BotDir::read(s.as_ref())
                        .unwrap_or_else(|| panic!("not a bot directory: {s}"));
                    children.push(d.launch(&server.url()).expect("launch"));
                    d.name
                }
            };
            let c = server
                .wait_for(&name, &taken, Duration::from_secs(60))
                .unwrap_or_else(|| panic!("{name} did not connect"));
            taken.push(c);
            names.push(name);
            entrants.push(Entrant::Remote(c));
        }
    }
    let live = Mutex::new(Live::default());
    let opts = BattleOptions { setup, tps, seed };
    let t = std::time::Instant::now();
    let res = run_battle(
        &server,
        entrants,
        names.clone(),
        &opts,
        &live,
        &AtomicBool::new(false),
    );
    for l in &live.lock().unwrap().log {
        println!("{l}");
    }
    match res {
        Ok(scores) => {
            let r = ranks(&scores);
            for (k, s) in scores.iter().enumerate() {
                println!(
                    "#{} {:24} total {:6.0}  rounds won {:3}  bullet damage {:6.0}  survival {:5.0}",
                    r[k],
                    names[k],
                    s.total(),
                    s.first_places,
                    s.bullet_damage,
                    s.survival
                );
            }
            println!(
                "{:.1}s, {} turns without every intent",
                t.elapsed().as_secs_f32(),
                live.lock().unwrap().timeouts
            );
        }
        Err(e) => println!("Battle failed: {e}"),
    }
    for mut c in children {
        let _ = c.kill();
    }
}
