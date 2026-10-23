# Claude Tanks

Two identical tanks duel in a random arena — from an open field to a light labyrinth — each driven by a black-box neural network. The app trains the networks (genetic algorithm, NEAT, PPO), compares architectures in a benchmark tournament, and replays battles in real or accelerated time.

Everything runs on the CPU, multi-threaded. The GUI is a light 2D egui window (OpenGL), so GPU use stays minimal.

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
| NEAT | evolving topology, recurrent links allowed | grows |

## Training methods

- **GA**: tournament selection, elitism, uniform crossover and Gaussian mutation over the flat weight vector. Works with MLP, Elman and GRU.
- **NEAT**: speciation with an adaptive compatibility threshold, fitness sharing, stagnation culling, and add-node / add-connection mutations.
- **PPO**: a Gaussian policy over the network's tanh outputs, a separate value MLP, GAE, the clipped surrogate objective and Adam. Recurrent nets train with truncated BPTT. All gradients are hand-written and checked against finite differences (`tests/grad.rs`). Works with MLP, Elman and GRU.

All three methods share the same training loop:
- **Self-play:** opponents are drawn from a hall of fame of past champions and from a scripted bot.
- **Curriculum:** the scripted bot starts as a *sitting duck*, then becomes a *wanderer*, a *rookie* and finally a *veteran*. It moves up a level once the champion wins at least 55% of its matches against the current level.
- **Shaping:** small rewards for tracking the enemy on radar, pointing the gun at it, near-misses and survival. The shaping weight anneals over training. PPO uses potential-based shaping, which leaves the optimal policy unchanged.
- **Scoring:** after every generation, the champion plays a fixed set of matches against the veteran bot.

## The GUI

- **Train**
  - Pick an architecture and method, then tune population, matches, maze density, threads, budget, shaping and the method's hyperparameters.
  - Start, pause, stop, or resume a saved run.
  - Live plots show fitness, win/draw/loss against the veteran, and method details: species and network size for NEAT, losses, KL and action std for PPO.
  - No battles are drawn while training.
  - Each run saves to `models/<run>/` (`best.json`, `latest.json`, a snapshot every 25 generations, and a resumable `run.state`).
- **Display**
  - Pick two pilots: any saved model or the scripted bot.
  - Set the terrain seed and density, or ask for a new random terrain each match.
  - Speed goes from 0.25× to 100×, or *max*. You can also pause or advance one step at a time.
  - Overlays show radar beams, wall rays and the last-seen marker. A HUD shows ammo, reload and the live network outputs.
- **Benchmark**
  - Trains every architecture × method with the same simulated-step budget.
  - Then runs a round-robin tournament between the champions and the veteran bot.
  - Results are an Elo ranking and a score matrix, saved to `results/`. Champions go to `models/champions/`.

## Headless tools

```
cargo run --release --example train_cli -- gru ppo 100 population=32 ppo.lr=0.0005
cargo run --release --example benchmark_cli -- 200 100   # M steps per entry, matches per pairing
cargo run --release --example bench                      # simulation speed
cargo test --release
```

## Building

- **Linux:** needs `libxkbcommon`, `wayland` / `xcb` and OpenGL development packages.
- **Windows:** builds with the MSVC toolchain.
- **Raspberry Pi 5:** build natively on Raspberry Pi OS 64-bit, or use the release binary. It runs on the V3D OpenGL driver.

## Benchmark results

See [`results/benchmark.md`](results/benchmark.md).
