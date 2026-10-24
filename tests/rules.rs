use claude_tanks::bots::ScriptedBot;
use claude_tanks::sim::world::{Shell, run_match};
use claude_tanks::sim::*;

fn open_world() -> World {
    let mut t = Terrain::generate(1, 0.0);
    // Keep only the outer walls so nothing interferes.
    t.walls.truncate(4);
    t.spawns = [
        (Vec2::new(200.0, 500.0), 0.0),
        (Vec2::new(800.0, 500.0), std::f32::consts::PI),
    ];
    World::new(t, false)
}

fn idle() -> Action {
    Action::default()
}

#[test]
fn rotation_rates_are_1_2_3() {
    let mut w = open_world();
    let a = Action {
        hull: 1.0,
        turret: 1.0,
        radar: 1.0,
        ..idle()
    };
    for _ in 0..10 {
        w.step([a, idle()]);
    }
    let t = &w.tanks[0];
    let h = t.heading;
    assert!((t.turret / h - 2.0).abs() < 1e-3, "{} {}", t.turret, h);
    assert!((t.radar / h - 3.0).abs() < 1e-3);
    assert!((h - HULL_ROT * 10.0 * DT).abs() < 1e-5);
}

#[test]
fn inertia_accelerates_and_coasts() {
    let mut w = open_world();
    let go = Action {
        throttle: 1.0,
        ..idle()
    };
    w.step([go, idle()]);
    let v1 = w.tanks[0].speed;
    assert!(v1 > 0.0 && v1 < MAX_SPEED * 0.1);
    for _ in 0..60 {
        w.step([go, idle()]);
    }
    let v = w.tanks[0].speed;
    assert!(v > v1 * 10.0 && v <= MAX_SPEED);
    w.step([idle(), idle()]);
    let coast = w.tanks[0].speed;
    assert!(coast > 0.9 * v && coast < v, "coasting should decay slowly");
    let back = Action {
        throttle: -1.0,
        ..idle()
    };
    w.tanks[0].pos = Vec2::new(600.0, 300.0);
    for _ in 0..150 {
        w.step([back, idle()]);
    }
    assert!(w.tanks[0].speed < -0.9 * MAX_SPEED);
}

#[test]
fn fire_cooldown_ammo_and_regen() {
    let mut w = open_world();
    // Aim away from the enemy so shells don't end the match.
    w.tanks[0].turret = std::f32::consts::FRAC_PI_2;
    let fire = Action {
        fire: true,
        ..idle()
    };
    w.step([fire, idle()]);
    assert_eq!(w.tanks[0].ammo, 2);
    // Cooldown: no second shot before 0.5 s.
    for _ in 0..28 {
        w.step([fire, idle()]);
    }
    assert_eq!(w.tanks[0].ammo, 2);
    for _ in 0..2 {
        w.step([fire, idle()]);
    }
    assert_eq!(w.tanks[0].ammo, 1);
    for _ in 0..30 {
        w.step([fire, idle()]);
    }
    assert_eq!(w.tanks[0].ammo, 0);
    assert_eq!(w.tanks[0].stats.shots, 3);
    // Regenerate one shell every 5 s, never above 3.
    let start = w.tick;
    while w.tanks[0].ammo == 0 {
        w.step([idle(), idle()]);
    }
    let regen_time = (w.tick - start) as f32 * DT;
    assert!(regen_time > 3.9 && regen_time < 5.01, "{regen_time}");
    for _ in 0..(30.0 / DT) as u32 {
        w.step([idle(), idle()]);
    }
    assert_eq!(w.tanks[0].ammo, MAX_AMMO);
}

#[test]
fn one_hit_kills() {
    let mut w = open_world();
    w.step([
        Action {
            fire: true,
            ..idle()
        },
        idle(),
    ]);
    while !w.done() {
        w.step([idle(), idle()]);
    }
    let r = w.result.unwrap();
    assert_eq!(r.winner, Some(0));
    assert!(!w.tanks[1].alive);
    assert_eq!(r.stats[0].hits, 1);
}

#[test]
fn simultaneous_hits_draw() {
    let mut w = open_world();
    let f = Action {
        fire: true,
        ..idle()
    };
    w.step([f, f]);
    while !w.done() {
        w.step([idle(), idle()]);
    }
    assert_eq!(w.result.unwrap().winner, None);
}

#[test]
fn shells_stop_at_walls() {
    let mut w = open_world();
    w.shells.push(Shell {
        pos: Vec2::new(500.0, 100.0),
        vel: Vec2::new(0.0, -SHELL_SPEED),
        owner: 0,
        closest: f32::MAX,
    });
    for _ in 0..30 {
        w.step([idle(), idle()]);
    }
    assert!(w.shells.is_empty());
}

#[test]
fn tanks_stay_in_arena() {
    let mut w = open_world();
    let go = Action {
        throttle: 1.0,
        ..idle()
    };
    for _ in 0..600 {
        w.step([go, idle()]);
    }
    let p = w.tanks[0].pos;
    assert!(p.x <= ARENA - TANK_RADIUS && p.x >= TANK_RADIUS, "{p:?}");
}

#[test]
fn timeout_is_draw() {
    let mut w = open_world();
    while !w.done() {
        w.step([idle(), idle()]);
    }
    let r = w.result.unwrap();
    assert_eq!(r.winner, None);
    assert_eq!(r.ticks, (MATCH_TIME / DT).round() as u32);
}

#[test]
fn spawns_are_far_and_free_at_all_densities() {
    for seed in 0..50 {
        for d in [0.0, 0.5, 1.0] {
            let t = Terrain::generate(seed, d);
            let (a, b) = (t.spawns[0].0, t.spawns[1].0);
            assert!((a - b).len() >= ARENA * 0.5);
            for w in &t.walls {
                let mut p = a;
                assert!(!w.push_out(&mut p, TANK_RADIUS));
                let mut p = b;
                assert!(!w.push_out(&mut p, TANK_RADIUS));
            }
        }
    }
}

#[test]
fn matches_are_deterministic() {
    for seed in 0..5 {
        let t = Terrain::generate(seed, 0.4);
        let r1 = run_match(
            t.clone(),
            false,
            &mut ScriptedBot::new(1),
            &mut ScriptedBot::new(2),
        );
        let r2 = run_match(t, false, &mut ScriptedBot::new(1), &mut ScriptedBot::new(2));
        assert_eq!(r1.winner, r2.winner);
        assert_eq!(r1.ticks, r2.ticks);
    }
}

#[test]
fn scripted_bots_actually_fight() {
    let mut decided = 0;
    for seed in 0..20 {
        let t = Terrain::generate(seed, 0.3);
        let r = run_match(
            t,
            seed % 2 == 0,
            &mut ScriptedBot::new(seed),
            &mut ScriptedBot::new(seed + 99),
        );
        decided += r.winner.is_some() as u32;
    }
    assert!(decided >= 8, "only {decided}/20 matches were decided");
}

#[test]
fn ace_beats_veteran() {
    use claude_tanks::bots::scripted::ACE;
    let mut w = [0; 3];
    for seed in 0..100 {
        let r = run_match(
            Terrain::generate(seed, (seed % 11) as f32 / 10.0),
            seed % 2 == 1,
            &mut ScriptedBot::with_level(seed, ACE),
            &mut ScriptedBot::new(seed + 99),
        );
        w[r.winner.unwrap_or(2)] += 1;
    }
    assert!(
        w[0] > w[1] + 15,
        "ace {} veteran {} draws {}",
        w[0],
        w[1],
        w[2]
    );
}

#[test]
fn stronger_bots_beat_veteran() {
    use claude_tanks::bots::scripted::{ACE, LEVELS, MAX_LEVEL};
    for level in ACE + 1..=MAX_LEVEL {
        let mut w = [0; 3];
        for seed in 0..100 {
            let r = run_match(
                Terrain::generate(seed, (seed % 11) as f32 / 10.0),
                seed % 2 == 1,
                &mut ScriptedBot::with_level(seed, level),
                &mut ScriptedBot::new(seed + 99),
            );
            w[r.winner.unwrap_or(2)] += 1;
        }
        assert!(
            w[0] > w[1] + 15,
            "{} {} veteran {} draws {}",
            LEVELS[level as usize],
            w[0],
            w[1],
            w[2]
        );
    }
}
