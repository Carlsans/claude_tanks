//! One-round duels between two in-process pilots, for training and scoring
//! under Robocode rules.

use super::Setup;
use super::battle::{Battle, Event, Intent};
use super::pilot::{Pilot, View};

/// Duels end as a draw after this many turns (inactivity damage usually
/// settles them well before).
pub const MAX_TURNS: u32 = 4000;

#[derive(Clone, Copy, Debug)]
pub struct DuelResult {
    /// Index of the surviving pilot, `None` for a draw.
    pub winner: Option<usize>,
    pub turns: u32,
    /// Bullet damage each pilot dealt.
    pub dealt: [f64; 2],
}

/// Plays one round on the default 800 x 600 arena; `seed` places the bots.
pub fn duel(seed: u64, a: &mut dyn Pilot, b: &mut dyn Pilot) -> DuelResult {
    let setup = Setup {
        rounds: 1,
        ..Setup::default()
    };
    let mut battle = Battle::new(setup.clone(), 2, seed);
    let mut pilots: [&mut dyn Pilot; 2] = [a, b];
    pilots.iter_mut().for_each(|p| p.reset());
    let mut next: [Option<Intent>; 2] = [None, None];
    let mut dealt = [0.0; 2];
    loop {
        let intents: Vec<(u32, Intent)> = (0..2)
            .filter_map(|k| next[k].take().map(|i| (k as u32 + 1, i)))
            .collect();
        let t = battle.step(&intents);
        // In a duel every hit involves bot 1, as shooter or victim.
        for e in &t.events[0] {
            if let Event::BulletHitBot { bullet, damage, .. } = e {
                dealt[bullet.owner as usize - 1] += damage;
            }
        }
        if t.round_ended || t.turn >= MAX_TURNS {
            let alive: Vec<usize> = (0..2)
                .filter(|&k| t.bot(k as u32 + 1).is_some_and(|b| b.alive()))
                .collect();
            let winner = match alive.as_slice() {
                [k] => Some(*k),
                _ => None,
            };
            return DuelResult {
                winner,
                turns: t.turn,
                dealt,
            };
        }
        for (k, p) in pilots.iter_mut().enumerate() {
            if let Some(me) = t.bot(k as u32 + 1).filter(|b| b.alive()) {
                let bullets: Vec<_> = t
                    .bullets
                    .iter()
                    .filter(|b| b.owner == me.id)
                    .map(|b| b.state())
                    .collect();
                next[k] = Some(p.act(&View {
                    setup: &setup,
                    turn: t.turn,
                    me,
                    bullets: &bullets,
                    events: &t.events[k],
                }));
            }
        }
    }
}
