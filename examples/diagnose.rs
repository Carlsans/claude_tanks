//! Play statistics of a pilot vs the veteran bot: `diagnose [model.json|-] [matches]`
//! (no model or `-` = the scripted bot itself).
use claude_tanks::bots::ScriptedBot;
use claude_tanks::nn::ModelFile;
use claude_tanks::sim::Terrain;
use claude_tanks::sim::world::{Controller, run_match};

fn main() {
    let path = std::env::args().nth(1).filter(|p| p != "-");
    let model = path
        .as_ref()
        .map(|p| ModelFile::load(p.as_ref()).expect("model"));
    let n: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    let (mut w, mut l, mut d) = (0, 0, 0);
    let (mut shots, mut hits, mut radar, mut aim, mut near, mut dist, mut secs) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut e_shots, mut e_hits) = (0.0, 0.0);
    for s in 0..n {
        let mut me: Box<dyn Controller> = match &model {
            Some(m) => Box::new(m.brain()),
            None => Box::new(ScriptedBot::new(s + 1000)),
        };
        let r = run_match(
            Terrain::generate(s + 77, (s % 11) as f32 / 10.0),
            s % 2 == 1,
            me.as_mut(),
            &mut ScriptedBot::new(s),
        );
        match r.winner {
            Some(0) => w += 1,
            Some(_) => l += 1,
            None => d += 1,
        }
        let t = r.ticks as f32 / 60.0;
        let st = r.stats[0];
        shots += st.shots as f32;
        hits += st.hits as f32;
        radar += st.radar_time / t;
        aim += st.aim / t;
        near += st.near;
        dist += st.distance;
        secs += t;
        e_shots += r.stats[1].shots as f32;
        e_hits += r.stats[1].hits as f32;
    }
    let k = n as f32;
    println!("W {w} L {l} D {d}  avg match {:.1}s", secs / k);
    println!(
        "me:    shots {:.2} hits {:.2} radar-lock {:.0}% aim {:.0}% near {:.2} moved {:.0}",
        shots / k,
        hits / k,
        radar / k * 100.0,
        aim / k * 100.0,
        near / k,
        dist / k
    );
    println!("enemy: shots {:.2} hits {:.2}", e_shots / k, e_hits / k);
}
