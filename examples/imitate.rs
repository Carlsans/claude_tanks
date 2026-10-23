//! Imitation-only quality check: `imitate <arch> [epochs] [matches] [dagger] [lr]`, then play the veteran.
use claude_tanks::nn::{Arch, Brain, Genes};
use claude_tanks::rng::Rng;
use claude_tanks::train::eval::vs_scripted;
use claude_tanks::train::imitation::{ImitationParams, pretrain};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let arch = Arch::ALL.into_iter().find(|x| x.short() == a[1]).unwrap();
    let mut p = ImitationParams::default();
    if let Some(v) = a.get(2) {
        p.epochs = v.parse().unwrap();
    }
    if let Some(v) = a.get(3) {
        p.matches = v.parse().unwrap();
    }
    if let Some(v) = a.get(4) {
        p.dagger_rounds = v.parse().unwrap();
    }
    if let Some(v) = a.get(5) {
        p.lr = v.parse().unwrap();
    }
    let t = std::time::Instant::now();
    let params = pretrain(arch, &p, &mut Rng::new(1), &|l| println!("{l}"));
    let g = Genes::Fixed(params);
    let (r, _) = vs_scripted(&|| Box::new(Brain::new(arch, &g)), 200, 3);
    println!(
        "{:?} {:.0}s  vs veteran W {:.0}% D {:.0}% L {:.0}%",
        arch,
        t.elapsed().as_secs_f32(),
        r.win * 100.0,
        r.draw * 100.0,
        r.loss * 100.0
    );
}
