# Claude Tanks

Two identical tanks duel in a random arena — from an open field to a light labyrinth — each driven by a black-box neural network. The app trains the networks (genetic algorithm, NEAT, PPO), compares architectures in a benchmark tournament, and replays battles in real or accelerated time.

Everything runs on the CPU, multi-threaded, with optional GPU help for training. The GUI is a light 2D egui window (OpenGL).

**GPU (optional):**
- GA and ES training can evaluate their networks on the GPU (wgpu compute shaders on Vulkan, DX12 or Metal).
- All matches of a generation advance in lockstep, split in two halves so that the CPU simulates one half while the GPU computes the other half's networks.
- With *Use the GPU when faster* on, the trainer times both devices on the first generations, keeps the faster one, and re-checks every 200 generations.
- The GPU wins for larger networks: GRU-64 runs ~1.5× faster. For small ones the vectorized CPU path is as fast or faster.
- Without a GPU, or with `cargo build --release --no-default-features`, everything runs on the CPU.

Supported platforms: **Linux (x86_64)**, **Windows (x86_64)** and **Raspberry Pi 5 (Linux aarch64)**. Prebuilt binaries are on the [Releases](https://github.com/Carlsans/claude_tanks/releases) page. Each archive also holds the champion models from the latest benchmark.

```
cargo run --release
```

## Rules

| | |
|---|---|
| Arena | 1000 × 1000. A random maze is thinned by the *density* setting: 0 gives an open field with a few pillars, 1 keeps 55% of the walls of a perfect maze, which makes a light labyrinth with loops |
| Rotation | hull 90°/s (1×), turret 180°/s (2×, relative to the hull), radar 270°/s (3×, relative to the turret). All three are controlled |
| Movement | the throttle sets acceleration along the hull, with linear drag. The tank has inertia: it coasts, then slows down. It can move forward or backward |
| Gun | holds at most 3 shells, with 0.5 s between shots. While below 3 shells, the tank gets 1 shell back every 5 s |
| Damage | one hit kills. Simultaneous kills and the 60 s timeout are draws |
| Physics | fixed 60 Hz step and deterministic: the same seed gives the same match. Pilots act at 30 Hz |

### What a pilot sees (34 inputs)
- Its own speed, turret and radar angles, ammo, reload and regen timers.
- 8 wall-distance rays around the hull.
- The radar beam, a 10° cone that walls block:
  - when the enemy is in the beam: enemy distance, offset within the beam, heading and speed;
  - when an enemy shell is in the beam: its distance and offset;
  - the distance to the wall the beam hits.
- Memory of the last place the enemy was seen: its bearing relative to the turret and to the hull, its distance, and the time since.
- The distance from the gun line to the wall, and the match time.

### Outputs (5)
Throttle, hull turn, turret turn, radar turn, fire (fires when the output is above 0).

## Architectures

| Name | Description | Params |
|---|---|---|
| MLP-S | one hidden layer of 32, tanh | 1 285 |
| MLP-L | two hidden layers of 64, tanh | 6 725 |
| Elman RNN | 32 recurrent units | 2 309 |
| GRU | 32 gated recurrent units | 6 597 |
| GRU-L | 64 gated recurrent units | 19 333 |
| GRU + clocks | GRU-32 whose gates also read 16 clocks, and a direct path from inputs to outputs. A clock is a leaky unit with a fixed time constant (0.1 s to 30 s, spread on a log scale) that the network writes into, so it can time holds and jinks the way the bots do. A trained GRU-32 can be copied into it unchanged (`init_model`) | 9 455 |
| NEAT | evolving topology, recurrent links allowed | grows |

## Training methods

- **GA**: tournament selection, elitism, uniform crossover and Gaussian mutation over the flat weight vector. Works with MLP, Elman and GRU.
- **ES**: OpenAI-style evolution strategy. Antithetic Gaussian perturbations of a single weight vector, centred-rank fitness and Adam. Works with MLP, Elman and GRU.
- **NEAT**: speciation with an adaptive compatibility threshold, fitness sharing, stagnation culling, and add-node / add-connection mutations.
- **PPO**: a Gaussian policy over the network's tanh outputs, a separate value MLP, GAE, the clipped surrogate objective and Adam. Recurrent nets train with truncated BPTT. All gradients are hand-written and checked against finite differences (`tests/grad.rs`). Works with MLP, Elman and GRU.

**The scripted bots:**
- The **veteran** is the fixed yardstick.
- The **ace** is stronger. It reads the beam exactly, compensates turret and radar for the hull's turn, swings the turret with the radar to sweep twice as fast, strafes across the line of sight, and reverses whenever an enemy shell appears in its beam. It beats the veteran 67/8/24.
- Three stronger bots share the ace's aim and radar, with a different *style* each:
  - The **hunter** drives to where the enemy was last seen, steering around walls, instead of wandering. It closes in from far away. It changes direction less often: after dodging a shell it holds its course for over 4 s.
  - The **brawler** moves like the hunter but fights up close.
  - The **stalker** dodges only the first shell and then holds its course, and takes only clean shots.
  - Tuned against the night's networks with `cargo run --release --example bots`. The decisive setting was the long hold after a dodge: the networks had learned to expect the ace's quick reversals.
- All of them use exactly the networks' 34 inputs.

Round robin, 1000 matches per pairing (row's win/draw/loss %):

| | veteran | ace | hunter | brawler | stalker | night champion | mean W−L |
|---|---|---|---|---|---|---|---|
| veteran | – | 29/9/62 | 20/3/76 | 32/4/64 | 24/4/72 | 38/12/50 | −36.1 |
| ace | 62/9/29 | – | 41/10/49 | 57/5/38 | 49/7/44 | 30/23/46 | +6.6 |
| **hunter** | 76/3/20 | 49/10/41 | – | 65/2/33 | 54/3/43 | 53/9/38 | **+24.7** |
| brawler | 64/4/32 | 38/5/57 | 33/2/65 | – | 41/3/57 | 54/6/40 | −3.9 |
| stalker | 72/4/24 | 44/7/49 | 43/3/54 | 57/3/41 | – | 57/10/33 | +14.1 |
| night champion | 50/12/38 | 46/23/30 | 38/9/53 | 40/6/54 | 33/10/57 | – | −5.3 |

**Imitation warm start** (on by default for MLP, Elman and GRU):
- Before training, the net clones a scripted teacher (the ace by default) from demonstrations, with DAgger rounds: the net drives and the teacher labels the states it visits.
- GA, ES or PPO then fine-tune from there.
- Pretrained weights are cached in `models/pretrained/`.
- `bptt` sets the memory window: how many decisions back (default 32, about 1 s) the net learns to remember from the teacher. In the GUI it is the *memory window* slider.
- Why it's needed: from scratch, nets learn very slowly. A diagnosis showed that a GA champion kept its radar on the enemy only 3% of the time, against 17% for the scripted bot, so it fired from stale information.
- Cloning alone gets MLP-L and GRU to parity with the bot. ES fine-tuning then pushes a GRU past it.

All methods share the same training loop:
- **Self-play:** opponents are drawn from a hall of fame of past champions and from a scripted bot.
- **League:** `league` lists saved models of any architecture that join the opponents as fixed sparring partners (`league_share` of the non-scripted matches). In the GUI, tick champions under *Opponents & shaping*.
- **Curriculum:** the scripted bot starts as a *sitting duck*, then becomes a *wanderer*, a *rookie*, a *veteran*, an *ace*, a *hunter*, a *brawler* and finally a *stalker*. It moves up a level once the champion wins at least 55% of its matches against the current level. Past the veteran, the scripted opponents mix every level from the veteran up to the current one: training against one bot alone makes networks forget the others. `start_level` starts a run at a given level (for example the top, to fine-tune a trained model against the whole mix), `top_level` caps the curriculum and `mix_from` sets the lowest level of the mix. With `bot_focus` above 0, bots the champion scores worse against are drawn more often: each bot's weight is (1 − recent score)^`bot_focus`.
- **Shaping:** small rewards for tracking the enemy on radar, pointing the gun at it, near-misses and survival. The shaping weight anneals over training. PPO uses potential-based shaping, which leaves the optimal policy unchanged.
- **Scoring:** after every generation, the champion plays a fixed set of matches against the veteran, the ace and every stronger bot the curriculum has reached. `best.json` is the snapshot with the best average win−loss against all of them.
- **Fine-tuning:** `init_model` starts a run from any saved model of the same architecture, or copies a GRU into GRU + clocks.
- **Weight averaging (ES):** the run also keeps an exponential moving average of the weights (`ema_decay`, default 0.99) and saves it as `ema.json`.

## Robocode battles (Tank Royale)

The app also plays by the rules of [Robocode Tank Royale](https://robocode.dev), the successor of [Robocode](https://robocode.sourceforge.io/). Real Robocode bots can fight here, against each other or against our tanks.

**How the rules differ from ours:**

| | Robocode | Ours |
|---|---|---|
| Arena | 800×600, open | Random maze from open field to light labyrinth |
| Health | 100 energy. A bullet does 4–16 damage; firing costs energy and a hit gives back 3× its power | One hit kills |
| Gun | Choose firepower 0.1–3; stronger bullets are slower and heat the gun longer | 3 shells, 0.5 s between shots, 1 back every 5 s |
| Movement | +1 / −2 per turn, top speed 8, turning slows with speed | Engine force against drag (inertia) |
| Radar | Sweeps up to 45° per turn, sees 1200 px with exact data, including the enemy's energy | A 10° beam blocked by walls |

**Importing bots:**
- `src/royale/` ports the official server's turn logic (the [Apache-2.0 source](https://github.com/robocode-dev/tank-royale), v1.4.0) and speaks its WebSocket protocol on port 7654, the official default.
- Any Tank Royale bot can connect: Java, C#, Python, TypeScript. Classic Robocode `.jar` robots run through the official [robocode-api-bridge](https://github.com/robocode-dev/robocode-api-bridge).
- A bot directory (`<Name>/<Name>.json` plus a script, a `.java` or a `.py` file) is started the way the official booter does it. The [official sample bots](https://github.com/robocode-dev/tank-royale/releases) work as they are.
- Our pilots join as local bots. They see only what a Royale bot sees (its own state, its own bullets and its events) and get it as our usual 34 inputs. A few inputs change meaning: own energy, gun heat, the enemy's energy, and "the enemy just fired" (judged by its energy drop, since Royale bots never see enemy bullets).

**Checked against the official server** (same sample bots, rounds won):

| Battle | Official server | Here |
|---|---|---|
| Walls vs SpinBot, 30 rounds | 16 – 14 | 16 – 14 |
| RamFire vs Crazy, 20 rounds | 20 – 0 | 20 – 0 |
| TrackFire vs Corners, 20 rounds | 20 – 0 (score 3628 – 334) | 20 – 0 (score 3607 – 298) |

**Training under Robocode rules:**
- `rules=Robocode` (or *Rules: Robocode* in the Train tab) trains GA or ES networks in one-round Robocode duels. There is no imitation warm start; *Start from* fine-tunes a saved champion instead.
- Opponents are the run's own past champions and built-in ports of five official sample bots: SpinBot, Walls, TrackFire, RamFire and Crazy (`src/royale/sparring.rs`). A small interpreter reproduces the Bot API's blocking commands and movement bookkeeping. The ports play like the originals: Walls vs SpinBot 15–15 (real 16–14), RamFire vs Crazy 30–0 (real 20–0).
- Fitness is +1 for a win and −1 for a loss, plus shaping for bullet damage dealt minus taken. `best.json` is picked on the average win−loss against the five ports.
- `royale_eval <model>` scores a model against each port and against the day champion. To meet the real bots, use the Robocode tab or `royale`.

```sh
# Unpack sample-bots-Java-1.4.0.zip into bots/, then:
cargo run --release --example royale -- rounds=10 bots/Walls bots/SpinBot
cargo run --release --example royale -- rounds=10 model:models/champions/gru_es_day.json bots/SpinBot
cargo run --release --example royale -- bot:hunter wait:MyBot   # a bot you start yourself
```

## The GUI

- **Train**
  - Pick an architecture and method, then tune population, matches, maze density, threads, budget, shaping and the method's hyperparameters.
  - Start, pause, stop, or resume a saved run.
  - Live plots show fitness, win/draw/loss against the veteran, and method details: species and network size for NEAT, losses, KL and action std for PPO.
  - No battles are drawn while training.
  - Each run saves to `models/<run>/` (`best.json`, `latest.json`, `ema.json` for ES, a snapshot every 25 generations, and a resumable `run.state`).
- **Display**
  - Pick two pilots: any saved model or the scripted bot.
  - Set the terrain seed and density, or ask for a new random terrain each match.
  - Speed goes from 0.25× to 100×, or *max*. You can also pause or advance one step at a time.
  - Overlays show radar beams, wall rays and the last-seen marker. A HUD shows ammo, reload and the live network outputs.
- **Benchmark**
  - Trains every architecture × method with the same budget: wall-clock time per entry (the default) and/or simulated steps. Imitation pretraining isn't counted in the budget.
  - Then runs a round-robin tournament between the champions and the veteran bot.
  - Results are an Elo ranking and a score matrix, saved to `results/`. Champions go to `models/champions/`.
- **Robocode**
  - Runs a Tank Royale server (port 7654) that real Robocode bots join.
  - *Bot folders* lists the bot directories under `bots/` (or any folder); *Start* launches one, and it connects by itself.
  - Pick two sides from the connected bots, our scripted bots and our saved models, set the rounds and the speed (turns per second, or *max*), and watch: energy bars, radar sweeps, bullets, and the score table.

## Headless tools

```
cargo run --release --example train_cli -- gru ppo 100 population=32 ppo.lr=0.0005
cargo run --release --example train_cli -- gru es 0 max_seconds=3600 init_model=models/champions/gru_es_night.json 'league=["models/champions/mlp_l_es_night.json"]' league_share=0.7
cargo run --release --example benchmark_cli -- 6 100     # minutes per entry, matches per pairing
cargo run --release --example imitate -- gru 20          # imitation only, then play the veteran
cargo run --release --example diagnose -- models/champions/gru_es.json   # play statistics vs the veteran
cargo run --release --example bench                      # simulation speed per architecture
cargo run --release --example gpu_bench                  # one generation on CPU vs GPU
cargo run --release --example evaluate -- <model> 1000   # scorecard vs every bot from the veteran up, the reference and the champion
cargo run --release --example bots -- 1000 veteran ace hunter <model.json>   # round robin; also "rows vs columns" and custom styles
cargo run --release --example tournament -- 1000 out A=a.json B=b.json   # round robin + Elo, with both bots
cargo test --release
```

## Building

- **Linux:** needs `libxkbcommon`, `wayland` / `xcb` and OpenGL development packages.
- **Windows:** builds with the MSVC toolchain.
- **Raspberry Pi 5:** build natively on Raspberry Pi OS 64-bit, or use the release binary. It runs on the V3D OpenGL driver.

## Results

**Night run** (23–24 Oct 2026, full report in [`results/night/`](results/night/README.md)):
- Eleven experiments, about 8 hours of training on 16 threads.
- The night champion [`models/champions/gru_es_night.json`](models/champions/gru_es_night.json) is a GRU-32. It imitated the ace, then trained 4.5 h with ES against a mix of the veteran, the ace and its own past champions.
- It beats both scripted bots and every earlier network.

Its record over 1000 fresh matches against each opponent (`evaluate <model> 1000`):

| Opponent | Win | Draw | Loss |
|---|---|---|---|
| Veteran (scripted) | **48.5%** | 11.8% | 39.7% |
| Ace (scripted) | **48.9%** | 21.8% | 29.3% |
| Previous champion (GRU, 40 min) | **40.9%** | 32.0% | 27.1% |

Final tournament, 1000 matches per pairing: night champion **1074** Elo, ace 1041, previous champion 1016, veteran 1000 ([`results/night/tournament.md`](results/night/tournament.md)).

What made the night champion:
- a stronger teacher to imitate;
- mixing the veteran and the ace as opponents (training on the ace alone over-specializes);
- selecting champions on both bots;
- simply training longer.

The architecture mattered little: GRU-32, GRU-64 and MLP-L land within a few Elo of each other with the same recipe.

**Day run** (24 Oct 2026, [`results/day/`](results/day/README.md)): continuing from the night champion against all five bots gave the day champion [`models/champions/gru_es_day.json`](models/champions/gru_es_day.json). It is the first network to beat the hunter and the brawler, and it leads the night champion by 15 Elo (1094 vs 1079). The stalker (1130) and the hunter (1125) are still stronger. A GRU with built-in clocks, fine-tuned from it, did worse.

**First benchmark** (6 minutes per architecture × method, veteran teacher): [`results/benchmark.md`](results/benchmark.md).
- ES was the best method, ahead of PPO.
- GA lost what imitation gave it (fixed since: smaller mutations when starting from imitation).
- NEAT from scratch stayed far behind.

Trained weights are in [`models/champions/`](models/champions) and the imitation-pretrained starting points are in [`models/pretrained/`](models/pretrained). Every finished day-run's `best`, `latest` and `ema` networks are published in [`models/day/`](models/day) (a `git pull` brings them).
