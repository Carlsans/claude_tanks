use claude_tanks::nn::neat::{Genome, Innovations, NeatNet};
use claude_tanks::nn::{Arch, Brain, Genes, fixed};
use claude_tanks::rng::Rng;
use claude_tanks::sim::sensors::N_OBS;

#[test]
fn fixed_nets_run_and_reset() {
    let mut rng = Rng::new(3);
    for arch in Arch::FIXED {
        let p = fixed::init_params(arch, &mut rng);
        assert_eq!(p.len(), arch.param_count());
        let mut b = Brain::new(arch, &Genes::Fixed(p));
        let x = [0.5f32; N_OBS];
        let first = b.outputs(&x);
        let second = b.outputs(&x);
        assert!(first.iter().all(|v| v.abs() <= 1.0));
        if arch.recurrent() {
            assert_ne!(first, second, "{arch:?} should carry state");
        } else {
            assert_eq!(first, second);
        }
        claude_tanks::sim::world::Controller::reset(&mut b);
        assert_eq!(b.outputs(&x), first);
    }
}

#[test]
fn neat_mutations_keep_valid_networks() {
    let mut rng = Rng::new(5);
    let mut inn = Innovations::new();
    let mut g = Genome::initial(&mut rng, &mut inn);
    for _ in 0..200 {
        g.mutate_add_node(&mut rng, &mut inn);
        g.mutate_add_conn(&mut rng, &mut inn);
        g.mutate_weights(&mut rng, 0.5, 0.1);
    }
    assert!(g.conns.windows(2).all(|w| w[0].innov < w[1].innov));
    let other = Genome::initial(&mut rng, &mut inn);
    let child = g.crossover(&other, &mut rng);
    assert!(g.distance(&other, 1.0, 0.4) > 0.0);
    for genome in [&g, &child] {
        let mut net = NeatNet::new(genome);
        let out = net.forward(&[0.3; N_OBS]);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }
}
