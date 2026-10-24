# Day run — 24 October 2026

**Goal:** a network that beats the night champion ([`gru_es_night.json`](../../models/champions/gru_es_night.json), E11) head-to-head while staying ahead of the veteran and the ace.

**Scorecard** (`evaluate <model> 1000 <label> results/day`):
- 1000 fresh matches each against the veteran, the ace, the reference (`gru_es_40min.json`) and the night champion.
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
| D3+ | Next steps follow the results | e.g. the better recipe with a lower learning rate, or weight averaging | |

Both runs start with the shaping weight already at its final value (0.15), since they continue a trained network.

## Log

| # | Result | Verdict |
|---|---|---|
