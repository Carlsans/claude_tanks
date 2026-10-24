# Day run — 24 October 2026

**Goal:** a network that beats the night champion ([`gru_es_night.json`](../../models/champions/gru_es_night.json), E11) head-to-head while staying ahead of the veteran and the ace.

**Scorecard** (`evaluate <model> 1000 <label> results/day`):
- 1000 fresh matches each against the veteran, the ace, the reference (`gru_es_40min.json`), the night champion, and (from D1 on) the three stronger bots added during the day: hunter, brawler, stalker.
- Raw records are in [`scores.jsonl`](scores.jsonl).
- 1000 matches give about ±3 points of noise on each win−loss figure.

## Why

- Last night every run trained against the scripted bots plus its own recent snapshots (the hall of fame).
- The scripted styles turned out not to be transitive: a network that beat one bot often lost to the other.
- The other strong networks of the night play differently again. Training against them as fixed opponents ("league") should make the champion more robust than its own near-copies can.
- ES checkpoints wobble by ±3–4 points from one evaluation to the next. An exponential moving average of the weights (`ema.json`) may sit in the middle of that wobble and be stronger than any single snapshot.

## Plan

| # | Hypothesis | Change | Budget |
|---|---|---|---|
| D1 | Diverse fixed opponents beat self-play against near-copies | From the champion. Opponents: 50% scripted (veteran/ace), 35% league (E10, E6 GRU-64, E7 MLP-L, E1 ace specialist, the frozen champion, the reference), 15% own hall of fame | 2 h, 8 threads |
| D2 | Control | Same start and settings, no league (70% scripted, 30% own hall of fame, as in E11) | 2 h, 8 threads, in parallel |
| D3 | Training against the stronger bots makes the network more robust | From D2 latest. Scripted opponents: the veteran, ace, hunter, brawler and stalker in equal parts (70%), plus own hall of fame | 2 h, 8 threads |
| D4 | Leaving the veteran out of the mix helps | As D3, but only the ace, hunter, brawler and stalker | 2 h, 8 threads, in parallel |
| R3a | A network type built for this game does better: a GRU-32 plus 16 clocks (leaky units with fixed time constants from 0.1 s to 30 s) and a direct input-to-output path. The bots run on timers (the hunter holds its course 4.7 s after a dodge), which a plain GRU has to build from its gates | From D3 ema, copied into GRU + clocks unchanged (`init_model`). D3 recipe | 2 h, 8 threads, CPU |
| R3b | Control | From D3 ema, plain GRU-32, same recipe and seed | 2 h, 8 threads, CPU, in parallel |
| I1–I4 | The clocks pay off where timers must be learned: copying a teacher. Imitation trains through windows of 32 decisions (≈1 s), too short for the hunter's 4.7 s hold | Imitate the hunter from scratch (300 matches, 20 epochs, 3 DAgger rounds): GRU-32 and GRU + clocks, each with a 32- and a 256-decision window (≈8.5 s) | about 10 min each |

Both runs start with the shaping weight already at its final value (0.15), since they continue a trained network.

## Log

Win−loss points over 1000 fresh matches against each opponent (night champion's own scorecard for comparison).

| Model | veteran | ace | reference | vs champion | hunter | brawler | stalker |
|---|---|---|---|---|---|---|---|
| Night champion (E11) | +8.8 | +19.6 | +13.8 | – | −15 | −14 | −24 |
| D1 league, best | +14.9 | +20.1 | +19.2 | +2.9 | −15.2 | −13.6 | −17.6 |
| D1 league, latest | +12.6 | +13.0 | +22.7 | −1.9 | −14.2 | −12.1 | −20.4 |
| D1 league, ema | +12.7 | +16.3 | **+24.0** | **+3.5** | −10.7 | −7.4 | −17.8 |
| D2 control, best | +22.8 | +18.8 | +17.9 | −7.9 | −12.3 | −8.3 | −17.0 |
| D2 control, latest | **+23.6** | **+21.3** | +21.5 | −0.1 | −9.2 | **−5.9** | −17.8 |
| D2 control, ema | +20.5 | +19.6 | +18.4 | +0.6 | −7.2 | −7.9 | −17.6 |
| D3 all bots, best | +14.3 | +18.5 | +20.4 | +0.5 | −6.9 | −5.0 | −14.2 |
| D3 all bots, latest | +20.6 | +17.3 | +17.4 | +0.3 | −1.4 | −2.2 | −11.7 |
| D3 all bots, ema | +18.1 | +20.2 | +18.5 | +0.3 | **+6.0** | **+3.0** | **−8.5** |
| D4 strong bots only, best | +6.9 | +11.3 | +7.2 | −2.8 | +0.2 | −5.0 | −15.6 |
| D4 strong bots only, latest | +11.0 | +22.0 | +20.1 | +0.9 | −5.7 | −10.4 | −14.4 |
| D4 strong bots only, ema | +16.3 | +20.3 | +17.2 | −1.4 | −0.3 | −6.9 | −15.7 |
| R3a GRU + clocks, best | +8.7 | +19.9 | +18.2 | +0.7 | −4.1 | −4.2 | −10.5 |
| R3a GRU + clocks, latest | +3.8 | +14.1 | +19.1 | −0.2 | −6.4 | −10.1 | −20.1 |
| R3a GRU + clocks, ema | +9.8 | +18.2 | +18.8 | −7.5 | −7.8 | −5.7 | −17.5 |
| R3b GRU-32 control, best | +20.1 | +18.9 | +15.9 | +3.2 | −2.9 | −7.0 | −12.1 |
| R3b GRU-32 control, latest | +13.1 | +15.8 | +20.6 | −1.6 | −4.0 | −3.1 | −10.8 |
| R3b GRU-32 control, ema | +10.7 | +22.8 | **+24.5** | +0.2 | **+6.1** | −2.6 | **−6.4** |

The night champion's hunter/brawler/stalker figures come from the 1000-match round robin in the main README.

| # | Result | Verdict |
|---|---|---|
| D1 | League of six night networks as 35% of the opponents. Slightly better against networks (reference +24, champion +3.5 for the weight average), worse against the veteran (+13 vs +23 for D2) and the ace | **Not worth it.** Head-to-head against the champion is within noise, and it costs ground against the bots |
| D2 | The E11 recipe continued 2 h, shaping started at its final weight. Much better against the veteran (+8.8 → +23.6), ace and reference a little better; head-to-head against the champion even | **Worked a little.** Better scorecard, but not a stronger duellist against its parent |
| EMA | The moving average of the weights scores like the latest snapshot (within ±3 points) | **Neutral.** Kept: it costs nothing |
| Bots | Three stronger scripted bots (hunter, brawler, stalker). Every network loses to them, the night champion by 15–24 points | Next: train against them (D3, D4) |
| D3 | From D2 latest, scripted opponents drawn from all five bots. The weight average gains 13–15 points against each new bot (hunter −7.2 → +6.0, brawler −7.9 → +3.0, stalker −17.6 → −8.5) and keeps its lead over the veteran, the ace and the reference. Mean over the six opponents other than the champion: +9.6 (night champion −1.8, D2 latest +5.6). Head-to-head against the champion: even (+0.3, a third of the matches drawn) | **Worked.** Best network so far against the field. All models published in [`models/day/`](../../models/day) |
| D4 | As D3 without the veteran. Better than D2 against the hunter, but it loses ground against the veteran (+6.9 to +16.3) and the stalker | **Worse than D3.** Keep the veteran in the mix |
| EMA | In D3 the weight average beats the latest snapshot by 4–7 points against each new bot | **Helps here.** Both D3 and D4 favour it |
| R3a | D3 ema copied into GRU + clocks, 2 h more of the D3 recipe (1528 generations; the bigger network is 12% slower). Every snapshot is worse than its starting point: mean over the six non-champion opponents +2.6 (ema) against +9.5 for D3 ema, and −7.5 head-to-head against the night champion | **Failed.** The extra 2858 weights start at zero and ES grows them slowly and noisily; they cost strength instead of adding it |
| R3b | Control: D3 ema continued as a plain GRU-32 (1741 generations). Mean +9.2 (ema), the same as its start | **No gain.** The D3 recipe has plateaued at this level |
| Tournament | 1000 matches per pairing ([`tournament.md`](tournament.md)): stalker 1130, hunter 1125, **D3 ema 1094**, R3b ema 1091, R3a ema 1083, D2 latest 1083, night champion 1079, brawler 1079, ace 1074, veteran 1000 | D3 ema is the strongest network, 15 Elo above the night champion. Saved as [`models/champions/gru_es_day.json`](../../models/champions/gru_es_day.json). The stalker and the hunter remain stronger than every network |
