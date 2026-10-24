//! Round robin between scripted bots (by level name or number) and saved models:
//! `bots <matches> <pilot> <pilot> ...`, e.g. `bots 1000 veteran ace hunter
//! models/champions/gru_es_night.json`. A pilot can also be a custom pro-bot
//! style: `style:min=220,max=inf,tilt=0.5,win=0.7,last=inf,hunt=1,jink=12,span=30,hold=40`
//! (unset keys keep the ace's values). Prints win/draw/loss % of each row against each
//! column, sides alternating, maze densities 0 to 1.
use claude_tanks::bots::ScriptedBot;
use claude_tanks::bots::scripted::{LEVELS, Style};
use claude_tanks::nn::ModelFile;
use claude_tanks::sim::world::Controller;
use claude_tanks::train::eval::head_to_head;

enum Pilot {
    Bot(u8),
    Custom(Style),
    Model(ModelFile),
}

fn parse_style(spec: &str) -> Style {
    let mut s = Style::ACE;
    for kv in spec.split(',').filter(|kv| !kv.is_empty()) {
        let (k, v) = kv.split_once('=').expect("key=value");
        let x: f32 = if v == "inf" {
            f32::INFINITY
        } else {
            v.parse().expect(v)
        };
        match k {
            "min" => s.min_range = x,
            "max" => s.max_range = x,
            "tilt" => s.tilt = x,
            "win" => s.window = x,
            "last" => s.last_shell_range = x,
            "hunt" => s.hunt = x != 0.0,
            "jink" => s.jink_min = x as u32,
            "span" => s.jink_span = (x as u32).max(1),
            "hold" => s.dodge_hold = x as u32,
            _ => panic!("unknown style key {k}"),
        }
    }
    s
}

impl Pilot {
    fn make(&self, seed: u64) -> Box<dyn Controller> {
        match self {
            Pilot::Bot(l) => Box::new(ScriptedBot::with_level(seed, *l)),
            Pilot::Custom(s) => Box::new(ScriptedBot::with_style(seed, *s)),
            Pilot::Model(m) => Box::new(m.brain()),
        }
    }
}

fn load(a: &str) -> (String, Pilot) {
    let level = LEVELS
        .iter()
        .position(|l| l.replace(' ', "_") == a)
        .map(|l| l as u8)
        .or_else(|| a.parse().ok());
    let pilot = match (level, a.strip_prefix("style:")) {
        (Some(l), _) => Pilot::Bot(l),
        (None, Some(spec)) => Pilot::Custom(parse_style(spec)),
        (None, None) => Pilot::Model(ModelFile::load(a.as_ref()).expect(a)),
    };
    let name = a
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".json")
        .to_string();
    (name, pilot)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let matches: usize = args.get(1).and_then(|s| s.parse().ok()).expect("matches");
    // `rows... vs cols...` plays every row against every column; without `vs`
    // it is a round robin.
    let split = args.iter().position(|a| a == "vs");
    let row_args = &args[2..split.unwrap_or(args.len())];
    let (names, pilots): (Vec<String>, Vec<Pilot>) = args[2..]
        .iter()
        .filter(|a| *a != "vs")
        .map(|a| load(a))
        .unzip();
    let rows = 0..row_args.len();
    let cols = if split.is_some() {
        row_args.len()..pilots.len()
    } else {
        0..pilots.len()
    };
    let w = names.iter().map(|n| n.len()).max().unwrap_or(8).max(8);
    print!("{:w$}", "");
    for n in &names[cols.clone()] {
        print!("  {:>14}", &n[..n.len().min(14)]);
    }
    println!("  mean W-L");
    for i in rows {
        print!("{:w$}", names[i]);
        let mut sum = 0.0;
        for j in cols.clone() {
            if i == j {
                print!("  {:>14}", "-");
                continue;
            }
            // In a round robin, (i, j) and (j, i) share a seed and mirror each
            // other; against fixed columns every row gets the same matches.
            let (a, b, seed) = if split.is_some() {
                (i, j, 30_000_000 + j as u64 * 10_000)
            } else {
                let (a, b) = (i.min(j), i.max(j));
                (a, b, 30_000_000 + (a * 100 + b) as u64 * 10_000)
            };
            let r = head_to_head(
                &|| pilots[a].make(seed + 1),
                &|| pilots[b].make(seed + 2),
                matches,
                seed,
            );
            let (win, loss) = if i == a {
                (r.win, r.loss)
            } else {
                (r.loss, r.win)
            };
            sum += win - loss;
            print!(
                "  {:>14}",
                format!(
                    "{:.0}/{:.0}/{:.0}",
                    win * 100.0,
                    r.draw * 100.0,
                    loss * 100.0
                )
            );
        }
        let n = cols.clone().filter(|&j| j != i).count().max(1);
        println!("  {:+.1}", sum / n as f32 * 100.0);
    }
}
