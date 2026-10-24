# Robocode-rules runs — 24 October 2026

**Goal:** a network that wins Robocode Tank Royale duels. Every run starts from the day champion ([`gru_es_day.json`](../../models/champions/gru_es_day.json)), which was trained under our own rules.

**Scorecards:**
- `royale_eval <model> 1000 <label>`: 1000 one-round duels against each of the five built-in sample-bot ports, and 1000 against the day champion. Raw records are in [`scores.jsonl`](scores.jsonl). The training run picks `best.json` on the first 100 of the same port seeds, so `best` is slightly flattered there.
- `royale_tournament 1000 <out> <label>=<model> ...`: round-robin between networks and the five ports, with an Elo fit ([`tournament_rc.md`](tournament_rc.md), full matrix in [`tournament_rc.json`](tournament_rc.json)).
- **The real bots:** 20 rounds against each of the ten one-bot samples of Tank Royale 1.4.0 (Java), through our server ([`real_bots_rc.md`](real_bots_rc.md)). Five of them have no port, so the networks never met them in training.

## Plan

| # | Hypothesis | Change | Budget |
|---|---|---|---|
| RC1 | What the day champion knows carries over to Robocode, and a short fine-tune adapts it | From the day champion. Opponents: 70% ports, 30% own hall of fame. 32 duels per candidate, shaping 0.3 → 0.15 | 2 h, 8 threads |
| RC2 | 64 duels per candidate (half the fitness noise) beat 32 | As RC1 with `matches=64` | 2 h, 8 threads, in parallel |
| RC3a | The RC networks play rock–paper–scissors (see below). Fixed opponents of other styles (a league) make a network that beats all of them | From RC1 ema. 30% ports, 35% league of 9 frozen networks (day, night and MLP-L night champions, R4b ema, RC1 best and latest, RC2 best, ema and latest), 35% own hall of fame. Shaping 0.15 | 2 h, 8 threads |
| RC3b | Control | As RC3a without the league: 30% ports, 70% own hall of fame | 2 h, 8 threads, in parallel |

## Results

### Against the ports (`royale_eval`, % of 1000 duels won)

| Network | SpinBot | Walls | TrackFire | RamFire | Crazy | Mean | vs day champion |
|---|---|---|---|---|---|---|---|
| Day champion (no Robocode training) | 53.2 | 51.0 | 36.6 | 94.1 | 88.4 | 64.7 | (itself) |
| RC1 best | 98.8 | 99.7 | 98.8 | 99.9 | 100.0 | **99.4** | **98.4** |
| RC1 latest | 94.0 | 98.9 | 93.9 | 99.8 | 94.8 | 96.3 | 51.7 |
| RC1 ema | 98.7 | 99.7 | 97.9 | 100.0 | 96.8 | 98.6 | 76.4 |
| RC2 best | 98.3 | 99.3 | 99.1 | 99.2 | 100.0 | 99.2 | 97.9 |
| RC2 latest | 97.7 | 99.5 | 97.9 | 99.0 | 100.0 | 98.8 | 95.2 |
| RC2 ema | 98.6 | 99.4 | 99.3 | 99.8 | 100.0 | **99.4** | 95.9 |

### Round-robin (`royale_tournament`, 1000 duels per pairing)

| Rank | Pilot | Elo | Mean score |
|---|---|---|---|
| 1 | RC1 latest | 1735 | 92.6% |
| 2 | RC1 ema | 1600 | 84.6% |
| 3 | RC2 latest | 1443 | 71.8% |
| 4 | RC2 ema | 1424 | 70.4% |
| 5 | RC2 best | 1391 | 68.1% |
| 6 | RC1 best | 1357 | 64.8% |
| 7 | Day champion (our rules) | 1032 | 37.3% |
| 8 | SpinBot (port) | 1000 | 34.4% |
| 9 | Walls (port) | 938 | 29.0% |
| 10 | TrackFire (port) | 923 | 28.3% |
| 11 | RamFire (port) | 733 | 14.3% |
| 12 | Crazy (port) | 528 | 4.3% |

The networks are far from transitive (score of the row against the column, %):

| | Day champion | RC1 best | RC1 ema | RC1 latest | RC2 best | RC2 ema | RC2 latest |
|---|---|---|---|---|---|---|---|
| Day champion | – | 1.9 | 22.6 | **51.6** | 1.3 | 3.0 | 4.3 |
| RC1 best | 98.1 | – | 4.0 | 0.1 | 50.9 | 30.8 | 33.8 |
| RC1 ema | 77.5 | 96.0 | – | 9.7 | 53.1 | 99.7 | 100.0 |
| RC1 latest | 48.4 | 99.9 | 90.3 | – | 97.9 | 99.9 | 100.0 |
| RC2 best | 98.7 | 49.1 | 46.9 | 2.1 | – | 29.1 | 26.1 |
| RC2 ema | 97.0 | 69.2 | 0.3 | 0.1 | 70.9 | – | 40.2 |
| RC2 latest | 95.7 | 66.2 | 0.0 | 0.0 | 73.9 | 59.8 | – |

The networks are deterministic, so 0% or 100% over 1000 starting positions means one of them has a fixed answer to the other.

### Against the real bots (rounds won of 20)

| Network | Corners | Crazy | Fire | MyFirstBot | RamFire | SpinBot | Target | TrackFire | VelociBot | Walls | Total |
|---|---|---|---|---|---|---|---|---|---|---|---|
| RC1 best | 19–1 | 20–0 | 20–0 | 19–1 | 20–0 | 20–0 | 14–6 | 20–0 | 20–0 | 20–0 | **192/200 (96.0%)** |
| RC1 ema | 18–2 | 17–3 | 20–0 | 16–4 | 20–0 | 20–0 | 19–1 | 19–1 | 18–2 | 17–3 | **184/200 (92.0%)** |
| RC2 latest | 20–0 | 20–0 | 15–5 | 16–4 | 20–0 | 20–0 | 15–5 | 19–1 | 19–1 | 20–0 | **184/200 (92.0%)** |
| RC1 latest | 17–3 | 19–1 | 19–1 | 16–4 | 20–0 | 17–3 | 20–0 | 19–1 | 18–2 | 12–8 | **177/200 (88.5%)** |

Earlier, an 8-minute snapshot of RC1 had won 49 of 50 rounds against TrackFire, SpinBot, Walls, Corners and VelociBot.

## Verdicts

- **RC1: confirmed.** Within 8 minutes the day champion went from 65% to 95–99% against the ports. The skill carries over to the real bots, including the five it never met: 88–96% of rounds.
- **`best.json` stops meaning much.** The port scorecard reached 100% after about 15 minutes (last new best: RC1 generation 279, RC2 generation 188), so `best` is a 15-minute snapshot. It does best against the real bots (96%) but is the weakest network head-to-head: RC1 latest beats it 99.9%.
- **Two hours of self-play specialize.** RC1 latest beats every other RC network 90–100%, but it only ties the day champion (48.4%) and loses 8 of 20 rounds to the real Walls. Its own hall of fame taught it to beat its relatives, not other styles.
- **RC2: refuted.** With 64 duels per candidate, all three RC2 files rank below RC1 latest and ema in the round-robin, and do no better against the real bots.
- **Robocode champion:** RC1 ema → [`models/champions/gru_es_robocode.json`](../../models/champions/gru_es_robocode.json). It is the most balanced network: second in the round-robin, at least 53% against every network except RC1 latest, 77.5% against the day champion, 92% against the real bots, and no real bot below 16–4.
- RC3a and RC3b are running.

All six RC networks are in [`models/robocode/`](../../models/robocode).
