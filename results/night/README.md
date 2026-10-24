# Night run — 23 → 24 October 2026

**Goal:** a network that beats the **ace**, the new strongest scripted bot, and beats the veteran by a wider margin than today's champion.

**How every candidate is scored** (`cargo run --release --example evaluate -- <model> 1000`):
- 1000 fresh matches each against the veteran, the ace and the reference champion (`models/champions/gru_es_40min.json`).
- No training run ever sees these terrains.
- Raw records are in [`scores.jsonl`](scores.jsonl).

## Plan

Experiments run one after another with all 16 threads. Each next step depends on the previous results, so the order can change.

| # | Hypothesis | Change | Budget |
|---|---|---|---|
| E1 | A stronger teacher gives a stronger start | GRU-32 imitates the ace, then ES. The curriculum goes up to the ace | 40 min |
| E2 | Architecture matters when the teacher has internal state | MLP-L (no memory) imitates the ace, then ES | 40 min |
| E3 | More memory lets the network copy the ace's dodge and jink timers | Redesign: a new GRU-64 architecture | 40 min |
| E4 | ES is limited by noisy fitness | Best architecture with 32 matches per candidate, or with noise σ 0.02 | 30 min each |
| E5 | The opponent mix matters | Mostly the ace, or mostly self-play (hall of fame) | 30 min |
| E6 | PPO works better from a strong start | PPO from ace imitation | 30 min |
| E7 | More compute on the best recipe | The best configuration, run long | 90+ min |
| E8 | Wrap-up | Tournament of all night champions and both bots, the report, and release v0.2.0 | ~07:00 |

Conditional redesign: if the networks can't copy the ace's timed behaviours (dodging, jinking), add the network's own previous action to its inputs.

## Baseline

| Model | vs veteran W/D/L % | vs ace W/D/L % | vs reference W/D/L % |
|---|---|---|---|
| GRU-32 / ES 40 min, veteran teacher (reference) | 53.3 / 12.2 / 34.5 | 36.8 / 19.8 / 43.4 | 34.5 / 31.8 / 33.7 |
| Ace (scripted) | 67 / 8 / 24 | — | — |

## Log

| # | Result | Verdict |
|---|---|---|
| E0 | MLP-L / ES, 40 min, veteran teacher: 56.5/7.5/36.0 vs veteran, 36.8/13.8/49.4 vs ace, 44.7/15.9/39.4 vs reference | MLP-L ≈ GRU-32 at equal time; slightly ahead head-to-head |
| Eng-1 | Vectorizable matvec: network inference 1.7–2.1× faster on CPU (GRU-32 200k → 331k ticks/s/thread) | **Worked.** Every later run gets ~1.5× more generations |
| Eng-2 | GPU evaluation (wgpu) for GA/ES, lockstep matches in two pipelined halves, tail on CPU, automatic device choice | Works and matches the CPU. Faster only for bigger nets: GRU-64 ~1.5×. GRU-32 is faster on the vectorized CPU, because simulation dominates |
