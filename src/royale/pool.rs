//! Bots kept running for training: every slot is a server of its own with one
//! bot process, so battles run side by side, and bots stay up (keeping what
//! they learned about their opponents) from one battle to the next.

use super::Setup;
use super::pilot::Pilot;
use super::server::{BattleOptions, BotDir, Entrant, Live, Server, run_battle};
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Time a bot gets to connect after it is started (all RoboRumble robots
/// that run connect within 6 s).
const CONNECT: Duration = Duration::from_secs(20);
/// Battles running longer than this are stopped and count as failed.
const BATTLE_LIMIT: Duration = Duration::from_secs(300);
/// A bot that fails this many battles in a row is given up.
const MAX_FAILURES: u32 = 3;
/// A bot is given up when, over a battle, it answers too late on more than
/// this share of the turns (each waits the whole turn timeout)...
const MAX_SKIPPED: f64 = 0.1;
/// ...or the turns take longer than this on average (ms). Robots answering
/// just under the timeout never skip a turn, but slow every battle 10-fold.
/// Turns take 1-1.5 ms with strong robots (DrussGT, Diamond) when few run;
/// in the RoboRumble survey, 16 at once, 2.4 ms (median) and at most 5.7 ms
/// for the robots training uses.
const MAX_MS_PER_TURN: f64 = 8.0;
/// Turns after which a battle is stopped as soon as the bot is too slow,
/// instead of waiting for its end (one slow battle could hold up training
/// for minutes).
const SLOW_CHECK_TURNS: u64 = 500;

/// Whether `turns` turns, `skipped` of them without the bot's intent, in
/// `seconds`, make the bot too slow to train against.
fn too_slow(turns: u64, skipped: u64, seconds: f64) -> bool {
    skipped as f64 > MAX_SKIPPED * turns as f64 || seconds * 1000.0 > MAX_MS_PER_TURN * turns as f64
}
/// Memory one bot process may take: the heap `BotDir::launch` allows a JVM,
/// plus the JVM itself.
pub const BOT_MEMORY_MB: u64 = 400;
/// The same for a robot translated to Rust (`scripts/translate_robots.py`),
/// with room for robots that keep large statistics.
pub const NATIVE_BOT_MEMORY_MB: u64 = 80;

/// Memory available for new processes, in MB (Linux only).
pub fn available_memory_mb() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = info.lines().find(|l| l.starts_with("MemAvailable:"))?;
    Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? / 1024)
}

/// One battle, from our side.
#[derive(Clone, Copy, Debug, Default)]
pub struct BattleResult {
    pub rounds: u32,
    pub won: u32,
    pub lost: u32,
    /// Bullet damage we dealt and took.
    pub dealt: f64,
    pub taken: f64,
}

/// What a bot's battles cost since the pool started.
#[derive(Clone, Copy, Debug, Default)]
pub struct BotStats {
    pub battles: u32,
    pub failed: u32,
    pub rounds: u32,
    /// Time in battles, not counting starts.
    pub seconds: f64,
    pub turns: u64,
    /// Turns that went ahead without the bot's intent.
    pub skipped: u64,
    /// Given up: too many failed battles in a row, or too slow.
    pub given_up: bool,
}

impl BotStats {
    pub fn skipped_share(&self) -> f64 {
        self.skipped as f64 / self.turns.max(1) as f64
    }
}

struct Slot {
    server: Server,
    child: Option<Child>,
    conn: Option<u64>,
}

impl Slot {
    fn kill(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.conn = None;
    }

    /// The bot's connection, starting it again if it is gone.
    fn connect(&mut self, bot: &BotDir) -> Option<u64> {
        if let Some(c) = self.conn
            && self.server.bots().iter().any(|(id, _)| *id == c)
        {
            return Some(c);
        }
        let running = self
            .child
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_none());
        if self.conn.is_some() || !running {
            self.kill();
            self.child = bot.launch(&self.server.url()).ok();
        }
        self.conn = self.server.wait_for(&bot.name, &[], CONNECT);
        self.conn
    }
}

pub struct RobotPool {
    bots: Vec<BotDir>,
    per_bot: usize,
    /// Slots of bot k are `k * per_bot .. (k + 1) * per_bot`.
    slots: Vec<Mutex<Slot>>,
    busy: Mutex<Vec<bool>>,
    freed: Condvar,
    failures: Vec<AtomicU32>,
    stats: Vec<Mutex<BotStats>>,
    /// The name our pilots battle under.
    name: String,
}

impl RobotPool {
    /// Starts `per_bot` processes of every bot; they connect in the background.
    pub fn start(bots: Vec<BotDir>, per_bot: usize) -> std::io::Result<RobotPool> {
        let per_bot = per_bot.max(1);
        let mut slots = Vec::new();
        for b in &bots {
            for _ in 0..per_bot {
                let server = Server::start(0)?;
                let child = b.launch(&server.url()).ok();
                slots.push(Mutex::new(Slot {
                    server,
                    child,
                    conn: None,
                }));
            }
        }
        Ok(RobotPool {
            failures: bots.iter().map(|_| AtomicU32::new(0)).collect(),
            name: "Candidate".into(),
            stats: bots.iter().map(|_| Mutex::default()).collect(),
            busy: Mutex::new(vec![false; slots.len()]),
            bots,
            per_bot,
            slots,
            freed: Condvar::new(),
        })
    }

    /// Our pilots battle under `name`. Some robots save what they learn
    /// about an opponent in files named after it, so runs that share robots
    /// at the same time should use different names.
    pub fn named(mut self, name: &str) -> RobotPool {
        self.name = name.into();
        self
    }

    pub fn bots(&self) -> &[BotDir] {
        &self.bots
    }

    /// Processes in the pool (battles that can run at once).
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    /// False once bot `bot` is given up.
    pub fn alive(&self, bot: usize) -> bool {
        self.failures[bot].load(Ordering::Relaxed) < MAX_FAILURES
    }

    pub fn stats(&self, bot: usize) -> BotStats {
        *self.stats[bot].lock().unwrap()
    }

    /// A battle of `rounds` rounds between `pilot` and bot `bot`, on the next
    /// free process of that bot. `None` if the bot failed; it is restarted
    /// for its next battle. Bots that fail too often or answer too slowly
    /// are given up.
    pub fn battle(
        &self,
        bot: usize,
        pilot: Box<dyn Pilot>,
        rounds: u32,
        seed: u64,
    ) -> Option<BattleResult> {
        if !self.alive(bot) {
            return None;
        }
        let range = bot * self.per_bot..(bot + 1) * self.per_bot;
        let k = {
            let mut busy = self.busy.lock().unwrap();
            loop {
                if let Some(k) = range.clone().find(|&k| !busy[k]) {
                    busy[k] = true;
                    break k;
                }
                busy = self.freed.wait(busy).unwrap();
            }
        };
        let live = Mutex::new(Live::default());
        let (res, seconds) = self.play(k, bot, pilot, rounds, seed, &live);
        self.busy.lock().unwrap()[k] = false;
        self.freed.notify_all();
        let l = live.into_inner().unwrap();
        if too_slow(l.turns, l.timeouts, seconds) {
            self.failures[bot].store(MAX_FAILURES, Ordering::Relaxed);
        } else if res.is_some() {
            self.failures[bot].store(0, Ordering::Relaxed);
        } else {
            self.failures[bot].fetch_add(1, Ordering::Relaxed);
        }
        let mut s = self.stats[bot].lock().unwrap();
        s.battles += 1;
        s.failed += u32::from(res.is_none());
        s.rounds += res.map_or(0, |r| r.rounds);
        s.seconds += seconds;
        s.turns += l.turns;
        s.skipped += l.timeouts;
        s.given_up = !self.alive(bot);
        res
    }

    fn play(
        &self,
        k: usize,
        bot: usize,
        pilot: Box<dyn Pilot>,
        rounds: u32,
        seed: u64,
        live: &Mutex<Live>,
    ) -> (Option<BattleResult>, f64) {
        let mut slot = self.slots[k].lock().unwrap();
        let b = &self.bots[bot];
        let Some(conn) = slot.connect(b) else {
            return (None, 0.0);
        };
        let opts = BattleOptions {
            setup: Setup {
                rounds,
                ..Setup::default()
            },
            tps: None,
            seed,
        };
        let (stop, done) = (AtomicBool::new(false), AtomicBool::new(false));
        let t = Instant::now();
        let res = std::thread::scope(|s| {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) && t.elapsed() < BATTLE_LIMIT {
                    std::thread::sleep(Duration::from_millis(100));
                    let l = live.lock().unwrap();
                    if l.turns >= SLOW_CHECK_TURNS
                        && too_slow(l.turns, l.timeouts, t.elapsed().as_secs_f64())
                    {
                        break;
                    }
                }
                stop.store(true, Ordering::Relaxed);
            });
            let r = run_battle(
                &slot.server,
                vec![Entrant::Local(pilot), Entrant::Remote(conn)],
                vec![self.name.clone(), b.name.clone()],
                &opts,
                live,
                &stop,
            );
            done.store(true, Ordering::Relaxed);
            r
        });
        let seconds = t.elapsed().as_secs_f64();
        match res {
            Ok(s) => (
                Some(BattleResult {
                    rounds,
                    won: s[0].first_places,
                    lost: s[1].first_places,
                    dealt: s[0].bullet_damage,
                    taken: s[1].bullet_damage,
                }),
                seconds,
            ),
            Err(_) => {
                slot.kill();
                (None, seconds)
            }
        }
    }
}

impl Drop for RobotPool {
    fn drop(&mut self) {
        for s in &self.slots {
            s.lock().unwrap().kill();
        }
    }
}
