//! Tank Royale rules and server.

use claude_tanks::bots::ScriptedBot;
use claude_tanks::royale::battle::{Battle, Event, Intent};
use claude_tanks::royale::pilot::{Adapter, Driver, Pilot, View};
use claude_tanks::royale::server::{BattleOptions, Entrant, Live, Server, run_battle};
use claude_tanks::royale::*;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[test]
fn rule_formulas_match_robocode() {
    assert_eq!(new_speed(0.0, 8.0), 1.0);
    assert_eq!(new_speed(7.5, 8.0), 8.0);
    assert_eq!(new_speed(8.0, 4.0), 6.0);
    assert_eq!(new_speed(-8.0, -5.0), -6.0);
    // The official server's quirks: a target of 0 stops at once, and crossing
    // zero brakes for part of the turn and accelerates for the rest.
    assert_eq!(new_speed(8.0, 0.0), 0.0);
    assert_eq!(new_speed(-3.0, 0.0), 0.0);
    assert_eq!(new_speed(1.0, -8.0), -0.5);
    assert_eq!(new_speed(8.0, -8.0), 3.0);
    assert_eq!(max_turn_rate(0.0), 10.0);
    assert_eq!(max_turn_rate(-8.0), 4.0);
    assert_eq!(bullet_speed(3.0), 11.0);
    assert!((bullet_speed(0.1) - 19.7).abs() < 1e-12);
    assert_eq!(bullet_damage(1.0), 4.0);
    assert_eq!(bullet_damage(3.0), 16.0);
    assert_eq!(gun_heat(3.0), 1.6);
    assert_eq!(wall_damage(8.0), 3.0);
    assert_eq!(wall_damage(1.0), 0.0);
    assert_eq!(normalize_relative(270.0), -90.0);
    assert_eq!(normalize_absolute(-90.0), 270.0);
}

/// A bot spinning its radar finds a standing enemy within a few turns.
#[test]
fn spinning_radar_scans_the_enemy() {
    let mut b = Battle::new(Setup::default(), 2, 3);
    let spin = Intent {
        radar_turn_rate: MAX_RADAR_TURN_RATE,
        ..Intent::default()
    };
    let mut found = None;
    for _ in 0..12 {
        let t = b.step(&[(1, spin)]);
        if t.events[0]
            .iter()
            .any(|e| matches!(e, Event::ScannedBot { scanned: 2, .. }))
        {
            found = Some(t.turn);
            break;
        }
    }
    let turn = found.expect("never scanned");
    assert!(turn <= 10, "first scan at turn {turn}");
}

/// Two of our scripted bots play a full battle under Royale rules.
#[test]
fn local_pilots_finish_a_battle() {
    let setup = Setup {
        rounds: 3,
        ..Setup::default()
    };
    let mut battle = Battle::new(setup.clone(), 2, 11);
    let mut pilots: Vec<Adapter> = (0..2)
        .map(|k| Adapter::new(Driver::Scripted(ScriptedBot::with_level(k, 4))))
        .collect();
    let mut next: Vec<Option<Intent>> = vec![None, None];
    let mut rounds = 0;
    for _ in 0..200_000 {
        let intents: Vec<(u32, Intent)> = (0..2)
            .filter_map(|k| next[k].take().map(|i| (k as u32 + 1, i)))
            .collect();
        let t = battle.step(&intents);
        if t.turn == 1 {
            pilots.iter_mut().for_each(|p| p.reset());
        }
        for (k, p) in pilots.iter_mut().enumerate() {
            if let Some(me) = t.bot(k as u32 + 1).filter(|b| b.alive()) {
                next[k] = Some(p.act(&View {
                    setup: &setup,
                    turn: t.turn,
                    me,
                    bullets: &[],
                    events: &t.events[k],
                }));
            }
        }
        if t.round_ended {
            rounds += 1;
        }
        if t.game_ended {
            break;
        }
    }
    assert_eq!(rounds, 3);
    assert!(battle.game_ended());
    let places: u32 = battle.results.iter().map(|s| s.first_places).sum();
    assert!(places >= 3, "{:?}", battle.results);
}

/// A minimal bot speaking the WebSocket protocol, like the official Bot APIs.
fn fake_bot(url: String, name: &'static str) {
    use tungstenite::Message;
    let (mut ws, _) = tungstenite::connect(url).expect("connect");
    let read = |ws: &mut tungstenite::WebSocket<_>| -> Option<serde_json::Value> {
        loop {
            match ws.read() {
                Ok(Message::Text(t)) => return serde_json::from_str(t.as_str()).ok(),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    };
    let hello = read(&mut ws).expect("server handshake");
    assert_eq!(hello["type"], "ServerHandshake");
    let hs = serde_json::json!({
        "type": "BotHandshake", "sessionId": hello["sessionId"], "name": name,
        "version": "1.0", "authors": ["test"],
    });
    ws.send(Message::text(hs.to_string())).unwrap();
    while let Some(m) = read(&mut ws) {
        match m["type"].as_str() {
            Some("GameStartedEventForBot") => {
                ws.send(Message::text(r#"{"type":"BotReady"}"#)).unwrap();
            }
            Some("TickEventForBot") => {
                let intent = serde_json::json!({
                    "type": "BotIntent", "targetSpeed": 8, "turnRate": 5,
                    "radarTurnRate": 45, "firepower": 2,
                });
                ws.send(Message::text(intent.to_string())).unwrap();
            }
            Some("GameEndedEventForBot") => return,
            _ => {}
        }
    }
}

#[test]
fn remote_bots_play_through_the_server() {
    let server = Server::start(0).expect("listen");
    let url = server.url();
    let bots: Vec<_> = ["Alpha", "Beta"]
        .into_iter()
        .map(|n| {
            let u = url.clone();
            std::thread::spawn(move || fake_bot(u, n))
        })
        .collect();
    let a = server
        .wait_for("Alpha", &[], Duration::from_secs(10))
        .unwrap();
    let b = server
        .wait_for("Beta", &[a], Duration::from_secs(10))
        .unwrap();
    let setup = Setup {
        rounds: 2,
        ..Setup::default()
    };
    let live = Mutex::new(Live::default());
    let opts = BattleOptions {
        setup,
        tps: None,
        seed: 5,
    };
    let scores = run_battle(
        &server,
        vec![Entrant::Remote(a), Entrant::Remote(b)],
        vec!["Alpha".into(), "Beta".into()],
        &opts,
        &live,
        &AtomicBool::new(false),
    )
    .expect("battle");
    assert_eq!(scores.len(), 2);
    let log = live.lock().unwrap().log.join("\n");
    assert!(log.contains("Round 2 won"), "{log}");
    for t in bots {
        t.join().unwrap();
    }
}

/// The built-in sample-bot ports play like the real ones (checked against the
/// Java bots: Walls vs SpinBot is about even, RamFire beats Crazy every round).
#[test]
fn sparring_ports_play_like_the_samples() {
    use claude_tanks::royale::sparring::Sparring;
    let wins = |a: Sparring, b: Sparring, rounds: u32| -> [u32; 2] {
        let setup = Setup {
            rounds,
            ..Setup::default()
        };
        let mut battle = Battle::new(setup.clone(), 2, 21);
        let mut pilots = [a.pilot(), b.pilot()];
        let mut next: [Option<Intent>; 2] = [None, None];
        while !battle.game_ended() {
            let intents: Vec<(u32, Intent)> = (0..2)
                .filter_map(|k| next[k].take().map(|i| (k as u32 + 1, i)))
                .collect();
            let t = battle.step(&intents);
            if t.turn == 1 {
                pilots.iter_mut().for_each(|p| p.reset());
            }
            for (k, p) in pilots.iter_mut().enumerate() {
                if let Some(me) = t.bot(k as u32 + 1).filter(|b| b.alive()) {
                    next[k] = Some(p.act(&View {
                        setup: &setup,
                        turn: t.turn,
                        me,
                        bullets: &[],
                        events: &t.events[k],
                    }));
                }
            }
        }
        [
            battle.results[0].first_places,
            battle.results[1].first_places,
        ]
    };
    let [ram, crazy] = wins(Sparring::RamFire, Sparring::Crazy, 10);
    assert!(ram >= 8 && crazy <= 2, "RamFire {ram} vs Crazy {crazy}");
    let [walls, spin] = wins(Sparring::Walls, Sparring::SpinBot, 20);
    assert!(walls >= 4 && spin >= 4, "Walls {walls} vs SpinBot {spin}");
}

/// Training evaluation under Robocode rules: networks duel the sample-bot ports.
#[test]
fn robocode_population_evaluation() {
    use claude_tanks::nn::Arch;
    use claude_tanks::nn::fixed::init_params;
    use claude_tanks::rng::Rng;
    use claude_tanks::royale::sparring::Sparring;
    use claude_tanks::train::eval::{Opponent, Shaping, eval_population_royale, net_pilot};
    let mut rng = Rng::new(4);
    let members: Vec<Vec<f32>> = (0..3).map(|_| init_params(Arch::MlpS, &mut rng)).collect();
    let opps = vec![
        Opponent::Sparring(Sparring::SpinBot),
        Opponent::Sparring(Sparring::Walls),
    ];
    let out = eval_population_royale(
        &|i| net_pilot(Arch::MlpS, &members[i], false),
        members.len(),
        &opps,
        9,
        Shaping { weight: 0.5 },
    );
    assert_eq!(out.fitness.len(), 3);
    assert!(out.fitness.iter().all(|f| f.is_finite() && f.abs() <= 1.5));
    assert!(out.ticks > 0);
}

#[test]
fn battle_memory_counts_rounds_and_hits() {
    use claude_tanks::royale::pilot::PilotReport;
    use std::sync::Arc;
    let setup = Setup {
        rounds: 4,
        ..Setup::default()
    };
    let mut battle = Battle::new(setup.clone(), 2, 5);
    let reports: Vec<_> = (0..2)
        .map(|_| Arc::new(Mutex::new(PilotReport::default())))
        .collect();
    let mut pilots: Vec<Adapter> = (0..2)
        .map(|k| {
            Adapter::new(Driver::Scripted(ScriptedBot::with_level(k, 4)))
                .with_report(reports[k as usize].clone())
        })
        .collect();
    let mut next: Vec<Option<Intent>> = vec![None, None];
    let mut won = [0u32; 2];
    loop {
        let intents: Vec<(u32, Intent)> = (0..2)
            .filter_map(|k| next[k].take().map(|i| (k as u32 + 1, i)))
            .collect();
        let t = battle.step(&intents);
        if t.turn == 1 {
            pilots.iter_mut().for_each(|p| p.reset());
        }
        for (k, p) in pilots.iter_mut().enumerate() {
            if let Some(me) = t.bot(k as u32 + 1).filter(|b| b.alive()) {
                next[k] = Some(p.act(&View {
                    setup: &setup,
                    turn: t.turn,
                    me,
                    bullets: &[],
                    events: &t.events[k],
                }));
            }
        }
        // Rounds count when the next one starts, so the last is left out.
        if t.round_ended && t.round < setup.rounds {
            for w in &t.winners {
                won[*w as usize - 1] += 1;
            }
        }
        if t.game_ended {
            break;
        }
    }
    for (k, r) in reports.iter().enumerate() {
        let r = *r.lock().unwrap();
        assert_eq!(r.rounds, 3, "{r:?}");
        assert_eq!(r.won, won[k], "{r:?}");
        assert!(r.hits > 0 && r.hits <= r.resolved, "{r:?}");
        assert!(r.seen_turns > 0 && r.distance > 0.0, "{r:?}");
    }
    // Each side's hits are the other's hits taken.
    let (a, b) = (*reports[0].lock().unwrap(), *reports[1].lock().unwrap());
    assert!(a.hit_by <= b.hits && b.hit_by <= a.hits, "{a:?} {b:?}");
}

#[test]
fn gru_mem_starts_as_the_gru_it_copies() {
    use claude_tanks::nn::Arch;
    use claude_tanks::nn::fixed::{FixedNet, init_params, transplant};
    use claude_tanks::rng::Rng;
    use claude_tanks::royale::memory::N_MEM;
    use claude_tanks::sim::sensors::N_OBS;
    let mut rng = Rng::new(3);
    let p = init_params(Arch::Gru, &mut rng);
    let q = transplant(Arch::Gru, &p, Arch::GruMem, &mut rng).expect("transplant");
    assert_eq!(q.len(), Arch::GruMem.param_count());
    let (mut a, mut b) = (FixedNet::new(Arch::Gru, p), FixedNet::new(Arch::GruMem, q));
    for _ in 0..20 {
        let x: Vec<f32> = (0..N_OBS + N_MEM).map(|_| rng.gauss()).collect();
        // Equal up to the order of the sums.
        let (ya, yb) = (a.forward(&x[..N_OBS]), b.forward(&x));
        assert!(
            ya.iter().zip(&yb).all(|(u, v)| (u - v).abs() < 1e-5),
            "{ya:?} {yb:?}"
        );
    }
    // Outside Robocode battles the memory inputs are missing: zeros.
    b.reset();
    let x: Vec<f32> = (0..N_OBS).map(|_| rng.gauss()).collect();
    b.forward(&x);
}

#[test]
fn mixture_of_one_gru_plays_like_it() {
    use claude_tanks::nn::Arch;
    use claude_tanks::nn::fixed::{FixedNet, init_params, transplant};
    use claude_tanks::rng::Rng;
    use claude_tanks::royale::memory::N_MEM;
    use claude_tanks::sim::sensors::N_OBS;
    let mut rng = Rng::new(4);
    let p = init_params(Arch::Gru, &mut rng);
    let q = transplant(Arch::Gru, &p, Arch::Moe, &mut rng).expect("transplant");
    assert_eq!(q.len(), Arch::Moe.param_count());
    let (mut a, mut b) = (FixedNet::new(Arch::Gru, p), FixedNet::new(Arch::Moe, q));
    for _ in 0..20 {
        let x: Vec<f32> = (0..N_OBS + N_MEM).map(|_| rng.gauss()).collect();
        let (ya, yb) = (a.forward(&x[..N_OBS]), b.forward(&x));
        assert!(
            ya.iter().zip(&yb).all(|(u, v)| (u - v).abs() < 1e-5),
            "{ya:?} {yb:?}"
        );
    }
    assert_eq!(b.gate(), Some([0.5, 0.5]));
    assert_eq!(a.gate(), None);
}

#[test]
fn mixture_gate_follows_the_battle_memory() {
    use claude_tanks::nn::Arch;
    use claude_tanks::nn::fixed::{EXPERTS, FixedNet, Layout, init_params, mixture};
    use claude_tanks::rng::Rng;
    use claude_tanks::royale::memory::N_MEM;
    use claude_tanks::sim::sensors::N_OBS;
    let mut rng = Rng::new(5);
    let (pa, pb) = (
        init_params(Arch::Gru, &mut rng),
        init_params(Arch::GruMem, &mut rng),
    );
    let mut q = mixture(
        &[(Arch::Gru, &pa), (Arch::GruMem, &pb)],
        [0.0; EXPERTS],
        &mut rng,
    )
    .expect("mixture");
    // The gate turns to expert 2 when the first memory input is high.
    let Layout::Moe { gw, .. } = Layout::new(Arch::Moe).0 else {
        unreachable!()
    };
    q[gw + N_MEM] = 30.0;
    let mut moe = FixedNet::new(Arch::Moe, q);
    let (mut a, mut b) = (
        FixedNet::new(Arch::Gru, pa),
        FixedNet::new(Arch::GruMem, pb),
    );
    for step in 0..10 {
        let mut x: Vec<f32> = (0..N_OBS + N_MEM).map(|_| rng.gauss()).collect();
        x[N_OBS] = if step < 5 { 0.0 } else { 1.0 };
        let (ya, yb, y) = (a.forward(&x[..N_OBS]), b.forward(&x), moe.forward(&x));
        let want: Vec<f32> = if step < 5 {
            ya.iter().zip(&yb).map(|(u, v)| (u + v) / 2.0).collect()
        } else {
            yb.to_vec()
        };
        assert!(
            y.iter().zip(&want).all(|(u, v)| (u - v).abs() < 1e-5),
            "step {step}: {y:?} {want:?}"
        );
    }
}

#[test]
fn keep_state_carries_the_network_over_rounds() {
    use claude_tanks::nn::fixed::init_params;
    use claude_tanks::nn::{Arch, Brain, Genes};
    use claude_tanks::rng::Rng;
    let setup = Setup::default();
    let mut battle = Battle::new(setup.clone(), 2, 1);
    let t = battle.step(&[]);
    let me = t.bot(1).unwrap();
    let params = init_params(Arch::Gru, &mut Rng::new(8));
    for keep in [false, true] {
        let mut a = Adapter::new(Driver::Net(Brain::new(
            Arch::Gru,
            &Genes::Fixed(params.clone()),
        )));
        a.keep_state = keep;
        for _ in 0..5 {
            a.act(&View {
                setup: &setup,
                turn: 1,
                me,
                bullets: &[],
                events: &[],
            });
        }
        a.reset();
        let Driver::Net(Brain::Fixed(n)) = &a.driver else {
            unreachable!()
        };
        let kept = n.hidden_state().iter().any(|v| *v != 0.0);
        assert_eq!(kept, keep);
    }
}
