# Night run — 23 → 24 October 2026

**Goal:** a network that beats the **ace**, the new strongest scripted bot, and beats the veteran by a wider margin than the previous champion.

**Result:** reached.
- The night champion, [`models/champions/gru_es_night.json`](../../models/champions/gru_es_night.json), beats the veteran (48.5/11.8/39.7), the ace (48.9/21.8/29.3) and the previous champion (40.9/32.0/27.1). All figures are win/draw/loss % over 1000 fresh matches each.
- It tops the final tournament with Elo 1074. The ace is at 1041 and the veteran at 1000.

## Final tournament

Round robin, 1000 matches per pairing ([`tournament.md`](tournament.md), [`tournament.json`](tournament.json)):

| Rank | Pilot | Elo | Mean score |
|---|---|---|---|
| 1 | GRU-32, veteran+ace mix, 32 matches/candidate (E11) — **night champion** | 1074 | 56.3% |
| 2 | GRU-32, veteran+ace mix, 16 matches/candidate (E10) | 1067 | 55.1% |
| 3 | GRU-64, veteran+ace mix (E6) | 1052 | 52.8% |
| 4 | MLP-L, veteran+ace mix (E7) | 1050 | 52.4% |
| 5 | Ace (scripted) | 1041 | 51.0% |
| 6 | MLP-L, veteran teacher, 40 min (E0) | 1033 | 49.7% |
| 7 | GRU-32, ace only (E1) | 1031 | 49.5% |
| 8 | GRU-32, veteran teacher, 40 min (previous champion) | 1016 | 47.0% |
| 9 | Veteran (scripted) | 1000 | 44.5% |
| 10 | GRU-32, 6 min benchmark | 982 | 41.7% |

## What worked and what did not

| Idea | Verdict | Evidence |
|---|---|---|
| A stronger scripted teacher (the ace: exact aim, fast sweep, strafing, dodging) | **Worked** | The ace beats the veteran 67/8/24. Networks that imitate it, then ES, beat the ace (E1, E2, E3) |
| Training against the ace alone | **Did not work** | It specializes. E1 beat the ace but fell to 41/52 against the veteran. The scripted styles are not transitive |
| Mixing veteran and ace at the top of the curriculum, and selecting champions on both | **Worked, the key change** | E4/E5 were the first models to beat all three opponents. Mean W−L went from about +5 to +11 |
| Longer training on the mixed line (E4 → E8 → E10, E9 → E11) | **Worked** | +11 → +12.5 → +14–16 over 3.5 h |
| 32 matches per candidate instead of 16 | **Neutral at 1 h, slightly better at 3 h** | E9 ≈ E8; E11 > E10 in the tournament and head-to-head |
| Bigger network (GRU-64, redesign) | **Copies the ace best, no gain after ES** | Imitation loss 0.826 vs 0.881 (GRU-32). After ES it lands with the others (E6 #3). Draws more |
| Memoryless MLP-L vs recurrent GRU | **About equal** | Same recipe, same strength (E7 vs E5/E6). The recipe matters more than the architecture |
| Selecting the champion on a small (96-match) check | **Misleading** | Early lucky snapshots won ("best" was often worse than "latest" on 1000 fresh matches) |
| Vectorizable CPU matrix code | **Worked** | Network inference 1.7–2.1× faster, so every run trains ~1.5× more generations |
| GPU evaluation (wgpu) | **Works, small gain** | Correct, about 15% GPU use. ~1.5× faster for GRU-64 on an idle CPU, slower for GRU-32 or a shared CPU. The automatic device choice handles it |
| GPU inference server | **Did not work** | Thread hand-offs cost more than the GPU saves. Reverted |
| Two experiments in parallel (8 threads each) | **Worked** | Twice the experiments, uses the idle tail of each generation |
| PPO from the ace start, and adding the network's previous action as an input | **Not tried** | The mixed-ES line kept improving, so the night's time went there |

**How every candidate is scored** (`cargo run --release --example evaluate -- <model> 1000`):
- 1000 fresh matches each against the veteran, the ace and the reference champion (`models/champions/gru_es_40min.json`).
- No training run ever sees these terrains.
- Raw records are in [`scores.jsonl`](scores.jsonl).

## Plan (as written at 23:30; the order then followed the results)

| # | Hypothesis | Change | Budget |
|---|---|---|---|
| E1 | A stronger teacher gives a stronger start | GRU-32 imitates the ace, then ES. The curriculum goes up to the ace | 40 min |
| E2 | Architecture matters when the teacher has internal state | MLP-L (no memory) imitates the ace, then ES | 40 min |
| E3 | More memory lets the network copy the ace's dodge and jink timers | Redesign: a new GRU-64 architecture | 40 min |
| E4 | ES is limited by noisy fitness | 32 matches per candidate | 30 min each |
| E5 | The opponent mix matters | Mostly the ace, or mostly self-play | 30 min |
| E6 | PPO works better from a strong start | PPO from ace imitation | 30 min |
| E7 | More compute on the best recipe | The best configuration, run long | 90+ min |
| E8 | Wrap-up | Tournament, report, release v0.2.0 | ~07:00 |

The log below numbers the experiments in the order they actually ran.

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
| E4 | E1's latest fine-tuned 60 min (8 threads) with **mixed veteran+ace opponents and selection on both**. best: **47.2/7.7/45.1 vs veteran, 49.5/13.2/37.3 vs ace, 50.5/17.6/31.9 vs reference** (mean W−L **+11.0**). latest: 49.4/9.7/40.9, 42.3/20.0/37.7, 43.2/26.9/29.9 (+8.8) | **Worked.** First model that beats all three opponents at once, and the best so far. It recovered the veteran without losing the ace. Its line continues in E8 → E10 |
| E5 | GRU-32 from ace imitation with the mixed recipe from the start, 60 min (8 threads). best: 46.5/8.5/45.0, 44.8/16.2/39.0, 48.1/19.4/32.5 (+7.6). latest: 45.7/10.0/44.3, 46.1/18.9/35.0, 45.3/26.8/27.9 (+10.0) | **Worked.** Also beats all three. The mix keeps the network general from the start. Fine-tuning E1 (E4) is slightly ahead because it had 40 more minutes of training behind it |
| E6 | GRU-64 from ace imitation, mixed recipe, 60 min (8 threads). The automatic device choice switched to the GPU at gen 1234 (824 vs 855 ns/tick) and back to the CPU at gen 1439. best: **51.0/10.0/39.0 vs veteran**, 41.6/22.2/36.2 vs ace, 42.1/27.2/30.7 vs reference (+9.6). latest: 48.6/13.9/37.5, 40.7/29.6/29.7, 33.5/41.7/24.8 (+10.3) | **Worked.** Beats all three, and is the strongest against the veteran. It draws a lot: dodging well, finishing less |
| E7 | E2's MLP-L fine-tuned with the mixed recipe, 60 min (8 threads). best: 46.2/9.3/44.5, **51.0/12.6/36.4 vs ace**, 49.0/17.2/33.8 (+10.5). latest: 44.8/7.1/48.1, 49.6/10.0/40.4, 49.6/11.9/38.5 (+5.7) | **Worked**, close to E4. With the mixed recipe all architectures land at +10–11, within noise of each other. The recipe matters more than the architecture |
| E8 | E4's best trained another 60 min (8 threads), 16 matches per candidate (1918 generations). best: **53.4/7.5/39.1 vs veteran, 49.0/16.9/34.1 vs ace**, 45.5/17.1/37.4 vs reference (**+12.4**). latest: 47.0/10.2/42.8, 47.0/20.4/32.6, 42.0/26.9/31.1 (+9.8) | **Worked**: more training on the mixed line still improves. Best result so far against the two bots |
| E9 | Same as E8 but with 32 matches per candidate (1160 generations, less noise per generation). best: 47.7/10.4/41.9, 47.4/18.6/34.0, **46.4/25.8/27.8 vs reference** (**+12.6**). latest: 46.8/9.1/44.1, 46.8/18.6/34.6, 43.1/27.1/29.8 (+9.4) | **No difference** from E8 at equal time: fewer but cleaner generations land at the same strength. The final runs keep both settings (E10 = 16, E11 = 32) to check over a longer run |
| E10 | E8's best trained another 110 min (8 threads), 16 matches per candidate. latest: **53.5/7.3/39.2 vs veteran, 50.1/15.4/34.5 vs ace, 51.0/16.6/32.4 vs reference** (+16.2, best scorecard of the night). best: 49.6/7.7/42.7, 49.1/14.2/36.7, 54.7/15.3/30.0 (+14.7) | **Worked.** Still improving after 3.5 h on this line. Tournament #2 (Elo 1067). Saved as `models/champions/gru_es_night_m16.json` |
| E11 | E9's best trained another 110 min (8 threads), 32 matches per candidate. latest: 48.5/11.8/39.7 vs veteran, **48.9/21.8/29.3 vs ace**, 40.9/32.0/27.1 vs reference (+14.1). best: 50.2/10.7/39.1, 48.8/17.3/33.9, 44.9/26.3/28.8 (+14.0) | **Night champion.** Tournament #1 (Elo 1074), and it beats E10 head-to-head 41.1/22.0/36.9. The less noisy fitness pays off over a longer run. Saved as `models/champions/gru_es_night.json` |
| Eng-5 | Parallel experiments: two queues of 8 threads each instead of one 16-thread queue | Fills the idle cores at the end of each generation, and explores two ideas at once. The automatic choice keeps GPU runs on the CPU when sharing makes the GPU slower |
