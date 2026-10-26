//! Builds a mixture of experts from two trained networks:
//! `make_moe <out.json> <expert 1.json> <expert 2.json> [gate bias of expert 2]`.
//! Each expert is a GRU-32 or a GRU + battle memory. The gate starts with no
//! weight on the battle memory, so the mixture plays a fixed blend of the two
//! (even by default; a negative bias leans on expert 1) until training
//! teaches the gate to tell opponents apart. Train it with
//! `train_cli moe es 0 init_model=<out.json> ...`.
use claude_tanks::nn::fixed::mixture;
use claude_tanks::nn::{Arch, Genes, ModelFile};
use claude_tanks::rng::Rng;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage =
        "usage: make_moe <out.json> <expert 1.json> <expert 2.json> [gate bias of expert 2]";
    let out = args.get(1).expect(usage);
    let load = |i: usize| ModelFile::load(args.get(i).expect(usage).as_ref()).expect("model");
    let (a, b) = (load(2), load(3));
    let bias: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let params = |m: &ModelFile| match &m.genes {
        Genes::Fixed(p) => p.clone(),
        Genes::Neat(_) => panic!("{}: not a fixed-topology network", m.name),
    };
    let (pa, pb) = (params(&a), params(&b));
    let p = mixture(
        &[(a.arch, &pa), (b.arch, &pb)],
        [0.0, bias],
        &mut Rng::new(0),
    )
    .expect("experts must be GRU-32 or GRU + battle memory networks");
    let m = ModelFile {
        name: format!("moe({}, {})", a.name, b.name),
        arch: Arch::Moe,
        method: a.method.clone(),
        generation: 0,
        fitness: 0.0,
        vs_scripted: 0.0,
        rules: a.rules,
        keep_state: false,
        genes: Genes::Fixed(p),
    };
    m.save(out.as_ref()).expect("write");
    println!(
        "{out}: {} ({} parameters) from {} and {}",
        Arch::Moe.name(),
        Arch::Moe.param_count(),
        a.name,
        b.name
    );
}
