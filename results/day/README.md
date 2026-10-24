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

Both runs start with the shaping weight already at its final value (0.15), since they continue a trained network.

## Log

Win−loss points over 1000 fresh matches against each opponent (night champion's own scorecard for comparison).

| Model | veteran | ace | reference | vs champion | hunter | brawler | stalker |
|---|---|---|---|---|---|---|---|
| Night champion (E11) | +8.8 | +19.6 | +13.8 | – | −15.2 | −13.7 | −24.0 |
| D1 league, best | +14.9 | +20.1 | +19.2 | +2.9 | −15.2 | −13.6 | −17.6 |
| D1 league, latest | +12.6 | +13.0 | +22.7 | −1.9 | −14.2 | −12.1 | −20.4 |
| D1 league, ema | +12.7 | +16.3 | **+24.0** | **+3.5** | −10.7 | −7.4 | −17.8 |
| D2 control, best | +22.8 | +18.8 | +17.9 | −7.9 | −12.3 | −8.3 | −17.0 |
| D2 control, latest | **+23.6** | **+21.3** | +21.5 | −0.1 | −9.2 | **−5.9** | −17.8 |
| D2 control, ema | +20.5 | +19.6 | +18.4 | +0.6 | **−7.2** | −7.9 | −17.6 |

The night champion's hunter/brawler/stalker figures come from the 1000-match round robin in the main README.

| # | Result | Verdict |
|---|---|---|
| D1 | League of six night networks as 35% of the opponents. Slightly better against networks (reference +24, champion +3.5 for the weight average), worse against the veteran (+13 vs +23 for D2) and the ace | **Not worth it.** Head-to-head against the champion is within noise, and it costs ground against the bots |
| D2 | The E11 recipe continued 2 h, shaping started at its final weight. Much better against the veteran (+8.8 → +23.6), ace and reference a little better; head-to-head against the champion even | **Worked a little.** Better scorecard, but not a stronger duellist against its parent |
| EMA | The moving average of the weights scores like the latest snapshot (within ±3 points) | **Neutral.** Kept: it costs nothing |
| Bots | Three stronger scripted bots (hunter, brawler, stalker). Every network loses to them, the night champion by 15–24 points | Next: train against them (D3, D4) |
