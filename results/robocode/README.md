# Robocode-rules runs — 24 October 2026

**Goal:** a network that wins Robocode Tank Royale duels. Every run starts from the day champion ([`gru_es_day.json`](../../models/champions/gru_es_day.json)), which was trained under our own rules.

**Scorecards:**
- `royale_eval <model> 1000 <label>`: 1000 one-round duels against each of the five built-in sample-bot ports, and 1000 against the day champion. Raw records are in [`scores.jsonl`](scores.jsonl). The training run picks `best.json` on the first 100 of the same port seeds, so `best` is slightly flattered there.
- `royale_tournament 1000 <out> <label>=<model> ...`: round-robin between networks and the five ports, with an Elo fit ([`tournament_rc.md`](tournament_rc.md), full matrix in [`tournament_rc.json`](tournament_rc.json)).
- **The real bots:** 20 rounds against each of the ten one-bot samples of Tank Royale 1.4.0 (Java), through our server ([`real_bots_rc.md`](real_bots_rc.md)). Five of them have no port, so the networks never met them in training.
- **Held-out RoboRumble robots** (from RC3 on): `royale_survey` with 10 rounds against each of 120 robots spread over the strength range of the 393 held-out robots of the [split](rumble_split.json) (records in [`test120/`](test120)). Comparisons are paired: the same robots, placements and rounds for every network.
- **Full RoboRumble battles** (from 25 October): `royale_survey <test120> <model> 35` plays the same 120 robots for 35 rounds, as in the RoboRumble, under a fresh name each time (records in [`test120x35/`](test120x35)). Records keep every round's winner, so they show whether the robots learn our network over a battle.

## Plan

| # | Hypothesis | Change | Budget |
|---|---|---|---|
| RC1 | What the day champion knows carries over to Robocode, and a short fine-tune adapts it | From the day champion. Opponents: 70% ports, 30% own hall of fame. 32 duels per candidate, shaping 0.3 → 0.15 | 2 h, 8 threads |
| RC2 | 64 duels per candidate (half the fitness noise) beat 32 | As RC1 with `matches=64` | 2 h, 8 threads, in parallel |
| RC3a | The RC networks play rock–paper–scissors (see below). Fixed opponents of other styles (a league) make a network that beats all of them | From RC1 ema. 30% ports, 35% league of 9 frozen networks (day, night and MLP-L night champions, R4b ema, RC1 best and latest, RC2 best, ema and latest), 35% own hall of fame. Shaping 0.15 | 2 h, 8 threads |
| RC3b | Control | As RC3a without the league: 30% ports, 70% own hall of fame | 2 h, 8 threads, in parallel |
| RC4 | Real opponents beat ports and relatives: training against hundreds of RoboRumble robots carries over to robots never met | From RC3a latest. Each candidate per generation: 2 battles of 3 rounds against real robots (787 training robots of the [split](rumble_split.json), 32 running at a time, a new group every 20 generations) plus 8 duels (25% ports, the league with the old champion in place of RC3a latest, own hall of fame) | 4 h, 8 threads + 32 robot processes |
| RC4b | RC4 again, without the swapping | As RC4 with 256 MB robot heaps, 24 robots at a time and robots that skip turns or take over 8 ms a turn left out | 4 h, 8 threads + 24 robot processes |
| M0 | Control for M1–M3 | From RC4b latest. Each candidate per generation: 1 battle of 6 rounds against a real robot (10 running at a time, a new group every 10 generations) plus 8 duels as in RC4 | 300 generations, 4 threads + 10 robot processes |
| M1 | A network that keeps its state from one round to the next can learn the opponent over a battle | As M0 with `keep_state=true` | same, in parallel |
| M2 | Battle statistics as inputs (rounds won, both hit rates, damage balance, guess factors; `gru_mem`) let training find when to close in on a weak or evasive opponent | As M0 with the `gru_mem` architecture (RC4b latest copied in, new inputs starting at zero weight) | same, in parallel |
| M3 | Both together | `gru_mem` with `keep_state=true` | same, in parallel |
| M0b, M2b, M3b | After 300 generations the memory weights were still small (see below): more training lets M2 and M3 use them | M0, M2 and M3 continued from their latest networks, with a new robot order (`seed=1`). M1 dropped | 600 more generations, 3 runs in parallel |
| S1 | Against robots that shoot poorly, training alone finds that closing in pays (we still lose 29% of rounds to them, hitting 19% of our shots from 288 units) | M0 recipe from M0 ema, but only the weakest-shooting third of the training robots (262, by damage per turn against the old champion in the [survey](survey.md)). No reward for distance | 300 generations, in parallel with the above |
| MoE1 | A gate on the battle memory learns when to switch to the specialist, so the tank closes in only on weak opponents | Mixture of experts (`moe`): M0 ema (all robots) and S1 (weak shooters, itself from M0 ema), gate starting even. M0 recipe on all training robots. With M0 and S1 before it, 900 generations in all, as M0b | 300 generations, after S1 |

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
| RC3a best | 97.5 | 99.2 | 99.6 | 99.9 | 97.1 | 98.7 | 64.5 |
| RC3a latest | 92.9 | 99.7 | 99.1 | 99.9 | 93.9 | 97.1 | 60.6 |
| RC3a ema | 93.4 | 98.8 | 98.6 | 100.0 | 92.0 | 96.6 | 61.5 |
| RC3b best | 97.4 | 99.7 | 97.0 | 99.9 | 97.4 | 98.3 | 72.2 |
| RC3b latest | 80.4 | 87.9 | 81.7 | 99.9 | 90.5 | 88.1 | 39.6 |
| RC3b ema | 92.4 | 93.2 | 87.8 | 99.7 | 91.5 | 92.9 | 46.0 |
| RC4 best | 90.3 | 99.0 | 99.5 | 100.0 | 93.1 | 96.4 | 63.0 |
| RC4 latest | 90.8 | 98.3 | 97.4 | 100.0 | 89.0 | 95.1 | 66.7 |
| RC4 ema | 93.0 | 98.5 | 97.9 | 99.9 | 92.9 | 96.4 | 64.9 |
| RC4b best | 97.1 | 98.3 | 98.7 | 100.0 | 97.1 | 98.2 | 78.9 |
| RC4b latest | 87.5 | 97.4 | 97.5 | 100.0 | 93.9 | 95.3 | 65.6 |
| RC4b ema | 92.2 | 98.2 | 99.6 | 99.9 | 93.1 | 96.6 | 69.8 |
| M0 best | 95.1 | 97.4 | 99.4 | 100.0 | 96.4 | 97.7 | 80.0 |
| M0 latest | 89.1 | 96.3 | 98.6 | 99.8 | 95.0 | 95.8 | 74.6 |
| M0 ema | 93.2 | 97.1 | 98.3 | 100.0 | 95.9 | 96.9 | 76.6 |
| M1 best | 96.4 | 97.4 | 99.2 | 100.0 | 95.8 | 97.8 | 77.7 |
| M1 latest | 96.2 | 96.6 | 99.5 | 100.0 | 97.3 | 97.9 | 74.3 |
| M1 ema | 90.6 | 97.6 | 98.5 | 99.8 | 94.1 | 96.1 | 73.3 |
| M2 best | 91.9 | 96.4 | 97.7 | 100.0 | 95.6 | 96.3 | 70.9 |
| M2 latest | 89.7 | 94.8 | 94.0 | 100.0 | 91.7 | 94.0 | 65.5 |
| M2 ema | 91.6 | 96.0 | 97.5 | 100.0 | 94.2 | 95.9 | 68.5 |
| M3 best | 95.5 | 95.7 | 98.7 | 100.0 | 98.0 | 97.6 | 73.6 |
| M3 latest | 93.2 | 97.0 | 99.2 | 100.0 | 98.0 | 97.5 | 70.4 |
| M3 ema | 95.4 | 96.6 | 98.0 | 100.0 | 97.5 | 97.5 | 73.8 |

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

### Against 120 held-out RoboRumble robots (rounds won of 1200)

| Network | Rounds won | vs the champion, per robot (paired) |
|---|---|---|
| Champion (RC1 ema) | 525 (43.8%) | – |
| RC3a best | 526 (43.8%) | |
| **RC3a latest** | **590 (49.2%)** | **+5.4 ± 1.8 points** |
| RC3a ema | 556 (46.3%) | +2.6 ± 2.1 |
| RC3b best | 539 (44.9%) | +1.2 ± 1.5 |
| RC3b ema | 509 (42.4%) | |
| RC3b latest | 502 (41.8%) | |
| RC4 best | 593 (49.4%) | +5.7 ± 2.1 |
| **RC4 latest** | **615 (51.2%)** | **+7.5 ± 2.2** (+2.1 ± 1.6 against RC3a latest) |
| RC4 ema | 587 (48.9%) | +5.2 ± 2.1 |

The RoboRumble field is far stronger than the samples: our champion wins only 45% of rounds over all 1217 robots that run ([survey](survey.md)).

These 10-round surveys all played under the run's name, and about 11 of the 120 robots save what they learn about an opponent in files named after it. So each network after the first of a run met some robots that already knew its relatives (order: RC3a best, latest, ema; RC4 latest, ema, best). The 35-round surveys below play under a fresh name each time.

### Full battles: 35 rounds against the same 120 robots (rounds won of 4200)

*Score share* is our part of the Robocode score of each battle (survival, bullet damage, ram damage and bonuses), averaged over the robots, as the RoboRumble ranks robots.

| Network | Rounds won | Rounds 1–5 | 26–35 | Score share | vs the champion, rounds per robot (paired) |
|---|---|---|---|---|---|
| Champion (RC1 ema) | 1743 (41.5%) | 43.3% | 41.2% | 43.7% | – |
| RC3a latest | 2035 (48.5%) | 51.7% | 48.3% | 51.0% | +7.0 ± 1.8 points |
| RC4 latest | 2118 (50.4%) | 52.3% | 49.0% | 52.4% | +8.9 ± 1.9 |
| RC4 latest, state kept between rounds (untrained) | 2055 (48.9%) | 51.2% | 47.9% | 51.7% | (−1.5 ± 0.7 against RC4 latest) |
| RC4b best | 2037 (48.5%) | 50.7% | 48.5% | 50.9% | +7.0 ± 1.7 |
| **RC4b latest** | **2162 (51.5%)** | 53.7% | 51.3% | 53.4% | **+10.0 ± 2.0** (+1.0 ± 0.9 against RC4 latest) |
| RC4b ema | 2157 (51.4%) | 54.3% | 50.6% | 52.9% | +9.9 ± 1.8 (+0.9 ± 0.9 against RC4 latest) |

The ranking is the same as in the 10-round surveys. **The robots learn our networks a little over a battle:** every network wins fewer of rounds 26–35 than of rounds 1–10, by 1.2 to 2.8 points per robot (each within about one standard error, but all seven agree). Some robots clearly adapt (TimmiT, FloodSonnet, Hamilton, Saguaro, AFlatNatural, Jen, moonlightBat, ApolloKidd), while others do worse as the battle goes on (MorbidPriest, CannonfodderNano, Sunderer, NanoSatan). Our networks forget everything between rounds; M1–M3 test whether remembering helps.

### Memory between rounds (M0–M3, 35-round battles against the same 120 robots)

All four runs start from RC4b latest and differ only in the memory. Each network is compared with the M0 network of the same kind (latest with latest, and so on).

| Network | Rounds won | Rounds 1–5 | 26–35 | Score share | vs M0, rounds per robot | vs M0, score share | Distance |
|---|---|---|---|---|---|---|---|
| M0 best | 2172 (51.7%) | 51.5% | 50.8% | 53.2% | – | – | 260 |
| M0 latest | 2140 (51.0%) | 52.2% | 50.5% | 53.4% | – | – | 275 |
| **M0 ema** | **2201 (52.4%)** | 52.7% | 52.0% | 53.9% | – | – | 275 |
| M1 best | 1989 (47.4%) | 50.2% | 46.7% | 50.6% | **−4.4 ± 1.0** | **−2.6 ± 0.7** | 264 |
| M1 latest | 1999 (47.6%) | 50.8% | 47.5% | 50.4% | **−3.4 ± 1.1** | **−2.9 ± 0.8** | 258 |
| M1 ema | 2065 (49.2%) | 50.0% | 48.1% | 51.7% | **−3.2 ± 0.9** | **−2.2 ± 0.6** | 288 |
| M2 best | 2156 (51.3%) | 52.2% | 51.1% | 53.0% | −0.4 ± 1.1 | −0.2 ± 0.7 | 297 |
| M2 latest | 2174 (51.8%) | 54.2% | 51.7% | 53.5% | +0.8 ± 1.3 | +0.2 ± 0.8 | 304 |
| M2 ema | 2167 (51.6%) | 53.7% | 52.6% | 53.6% | −0.8 ± 0.9 | −0.3 ± 0.6 | 306 |
| **M3 best** | **2219 (52.8%)** | 57.7% | 51.2% | **54.5%** | +1.1 ± 0.9 | +1.2 ± 0.6 | 278 |
| M3 latest | 2180 (51.9%) | 51.7% | 51.1% | 53.6% | +1.0 ± 1.0 | +0.2 ± 0.6 | 277 |
| M3 ema | 2195 (52.3%) | 53.2% | 52.4% | 54.0% | −0.1 ± 1.0 | +0.1 ± 0.7 | 288 |

**Do the memory networks close in on weak or evasive robots?** No. M2 fights about 30 units farther away than M0 against every kind of robot, and slightly farther still against the weakest shooters (rank correlation +0.31 for M2 latest), which is the opposite of charging. M3 fights at the same distance as M0 whatever the robot.

**Would closing in pay?** Across all the networks above, a network that fights 100 units closer to the same robot gets about 6 points less of the score on average. That depends on the robot, though: against the third of robots that hit us least, closer goes with +1.5 points per 100 units; against the third that hit us most, with −8. The best distance depends on how well the opponent shoots, and that is what the memory inputs measure. This compares networks rather than one network at two distances, so it is a hint, not proof.

**Why training did not find it:** after 300 generations, the weights of the ten memory inputs (which started at zero) are about 0.05 in size, the same as the random drift of every other input weight over those generations. The trained observation weights are about 0.57. No memory input stands out, so the networks barely use them yet. M0b, M2b and M3b give them 600 more generations.

**600 more generations (M0b, M3b, ema networks):** M0b ema wins 52.9% of rounds (100 robots: 20 battles failed to start while the machine ran out of memory), +0.2 ± 1.3 points per robot over M0 ema; M3b ema wins 52.9% (120 robots), +0.5 ± 1.1. Both fight about 15 units farther away, a little more so against evasive robots, so still no charging. M2b and the best and latest networks of these runs are not surveyed yet.

## Verdicts

- **RC1: confirmed.** Within 8 minutes the day champion went from 65% to 95–99% against the ports. The skill carries over to the real bots, including the five it never met: 88–96% of rounds.
- **`best.json` stops meaning much.** The port scorecard reached 100% after about 15 minutes (last new best: RC1 generation 279, RC2 generation 188), so `best` is a 15-minute snapshot. It does best against the real bots (96%) but is the weakest network head-to-head: RC1 latest beats it 99.9%.
- **Two hours of self-play specialize.** RC1 latest beats every other RC network 90–100%, but it only ties the day champion (48.4%) and loses 8 of 20 rounds to the real Walls. Its own hall of fame taught it to beat its relatives, not other styles.
- **RC2: refuted.** With 64 duels per candidate, all three RC2 files rank below RC1 latest and ema in the round-robin, and do no better against the real bots.
- **Robocode champion:** RC1 ema → [`models/champions/gru_es_robocode.json`](../../models/champions/gru_es_robocode.json). It is the most balanced network: second in the round-robin, at least 53% against every network except RC1 latest, 77.5% against the day champion, 92% against the real bots, and no real bot below 16–4.
- **RC3a: confirmed.** With the league, two more hours raised the held-out RoboRumble score from 43.8% to 49.2% of rounds (+5.4 ± 1.8 points per robot), while keeping 97% against the ports.
- **RC3b (control) drifted.** Without the league, the network slowly got worse: 88% against the ports, 40% against the day champion and 41.8% of rounds against the held-out robots by the end. Playing only its own past versions taught it nothing general.
- **RC4: starved, inconclusive.** The run managed only 108 generations in 4 hours instead of about 1300. Generations took 10 s for the first 50, then slowed to as much as 1400 s: each robot is a JVM that may grow to a quarter of the memory by default, and 32 robots that keep learning pushed the machine deep into swap. RC4 latest still wins 51.2% of the held-out rounds, but +2.1 ± 1.6 points per robot over its start (RC3a latest) is within the noise. Bots now start with a 256 MB heap (8 strong robots over 240 rounds each: 2.1 GB instead of 3.9 GB, same speed), and training starts only as many robots as fit in the free memory.
- **RC4b: modest, within the noise.** Without the swapping, RC4b ran 825 generations in 4 hours. Its latest network wins 51.5% of the rounds of full battles against the held-out robots, the best so far, but only +1.0 ± 0.9 points per robot over RC4 latest. RC4b latest is the starting point of M0–M3.
- **Keeping the state between rounds needs training.** Simply letting RC4 latest carry its state over costs 1.5 ± 0.7 points: it was trained to start each round fresh.
- **M1 (state kept between rounds): refuted, worse.** All three M1 networks lose 3 to 4 points of rounds per robot against M0 (about 3 standard errors each) and 2 to 3 points of score share. 300 generations were not enough to unlearn starting each round fresh, or carrying the end of one round (often a dead enemy) into the next misleads the network.
- **M2 (battle-memory inputs): no difference.** Within ±1 point of M0 on every network. It learned to keep farther away (304 instead of 275 units), which lowers both hit rates, but not to choose its distance by opponent.
- **M3 (both): no clear difference.** M3 best has the most rounds won (52.8%) and score share (54.5%) of all networks, +1.2 ± 0.6 points of score share against M0 best. But among the nine comparisons with M0, one at two standard errors is what chance gives. M3 latest and ema are level with M0. Unlike M1, keeping the state does not hurt here.
- **Charging did not emerge.** No memory network fights closer against weak or evasive robots, and the memory weights are still at noise level. Three times longer training (M0b, M3b) changed neither.
- **No new champion yet.** M0 ema, M3 best and the other top networks win 51–53% of rounds against 41.5% for the Robocode champion, but none clearly beats the others. M0b ema and M3b ema (52.9%) are level with them too: against the Java robots, training has plateaued around 52–53% of rounds.

Every RC network is in [`models/robocode/`](../../models/robocode).

**Robots translated to Rust, for training:** `rs_robots.json` lists them by folder name: `use` (1038 that run properly and need under 0.8 ms per turn over 10 rounds), `too_slow` (37, with their ms per turn), `timeouts` (111, miss most turns), `no_connect` (17) and two that do not build. Training (T1 GRU, T2 GRU with battle memory, T3 mixture of experts, from the M0b/M3b/MoE models) uses the 677 of the 787 training robots that are in `use`; the 120 test robots are not in it.

**MoE follow-up (27 October):** G1 trains only the gate of T3 ema (`es.gate_only=true`, sigma 0.2, lr 0.03, experts frozen), G2 continues T3 ema with the normal settings; 2 h each on the Rust robots. The gate now separates opponents (weight on expert 2 spreads 0.35–0.71 for ema, 0.18–0.92 for latest, from 0.52–0.60 before) but the held-out win rate (test120, 35 rounds) is unchanged: T3 ema +1.1, G1 ema +1.3, G1 latest +1.5, G2 ema +0.9 points over M3b, each ± 1.1. Records in `test120x35/g1_gate_*`, `g2_ctrl_*`; models `models/robocode/g1_gate_*`, `g2_ctrl_*`.
