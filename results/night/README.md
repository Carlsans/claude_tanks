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
| Eng-3 | GPU utilization measured: about 15% average (peaks ~65%) during a GPU-assisted run. The networks are tiny (6–19k weights), so the GPU mostly waits for the CPU simulation | Expected for this model size |
| Eng-4 | GPU "inference server" (groups of matches on their own threads, server batches whichever groups are ready) | **Did not work.** Every round hands work between threads, which on a shared CPU wait on the scheduler. 2.6 s vs 1.9 s per generation even on an idle CPU. Reverted to the pipelined lockstep |
| E1 | GRU-32 imitating the ace (imitation alone: 32/6/62 vs veteran), then ES for 40 min. Promoted to ace-only opponents at gen 71. Final model: **50.1/10.9/39.0 vs ace**, 50.8/11.8/37.4 vs reference, but only 41.1/7.0/51.9 vs veteran. "best" (picked on the 96-match veteran check): 44.5/7.1/48.4 vs veteran, 36.3/10.1/53.6 vs ace | **Half worked.** First network to beat the ace, and it beats the reference head-to-head. But training against the ace alone made it forget the veteran (non-transitive styles), and selecting on the small veteran check picked a lucky early snapshot. Fix: mix veteran and ace at the top level, select on both bots (E4, E5). Saved as `models/champions/gru_es_ace_40min.json` |
| E2 | MLP-L imitating the ace (imitation loss 0.905), then ES for 60 min on 8 threads, same recipe as E1. latest: 42.5/9.1/48.4 vs veteran, **46.8/17.7/35.5 vs ace**, **42.8/29.5/27.7 vs reference** | Same pattern as E1: beats the ace and the reference but loses to the veteran. The memoryless MLP-L does as well as GRU-32 |
| E3 | GRU-64 (redesign) imitating the ace (**best imitation, loss 0.826**), then ES for 60 min on 8 threads. Picked the CPU: GPU 839 vs CPU 742 ns/tick on a shared CPU. best: **52.2/6.3/41.5 vs veteran**, 41.5/13.2/45.3 vs ace, 46.4/16.3/37.3 vs reference. latest: 38.0/15.5/46.5 vs veteran, **42.4/29.4/28.2 vs ace**, **38.8/36.3/24.9 vs reference** | More memory copies the ace better. It still specializes on the ace like E1/E2, and later snapshots draw much more: it learned to dodge and survive rather than win. Every model so far lands at about +5 mean (W−L) over the three opponents. The non-transitive styles are the real limit |
| Eng-5 | Parallel experiments: two queues of 8 threads each instead of one 16-thread queue | Fills the idle cores at the end of each generation, and explores two ideas at once. The automatic choice keeps GPU runs on the CPU when sharing makes the GPU slower |
