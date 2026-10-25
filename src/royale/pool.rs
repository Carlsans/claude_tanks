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

/// Time a bot gets to connect after it is started.
const CONNECT: Duration = Duration::from_secs(45);
/// Battles running longer than this are stopped and count as failed.
const BATTLE_LIMIT: Duration = Duration::from_secs(300);
/// A bot that fails this many battles in a row is given up.
const MAX_FAILURES: u32 = 3;

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
            busy: Mutex::new(vec![false; slots.len()]),
            bots,
            per_bot,
            slots,
            freed: Condvar::new(),
        })
    }

    pub fn bots(&self) -> &[BotDir] {
        &self.bots
    }

    /// Processes in the pool (battles that can run at once).
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    /// False once bot `bot` failed too many battles in a row.
    pub fn alive(&self, bot: usize) -> bool {
        self.failures[bot].load(Ordering::Relaxed) < MAX_FAILURES
    }

    /// A battle of `rounds` rounds between `pilot` and bot `bot`, on the next
    /// free process of that bot. `None` if the bot failed; it is restarted
    /// for its next battle.
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
        let res = self.play(k, bot, pilot, rounds, seed);
        self.busy.lock().unwrap()[k] = false;
        self.freed.notify_all();
        match res {
            Some(_) => self.failures[bot].store(0, Ordering::Relaxed),
            None => {
                self.failures[bot].fetch_add(1, Ordering::Relaxed);
            }
        }
        res
    }

    fn play(
        &self,
        k: usize,
        bot: usize,
        pilot: Box<dyn Pilot>,
        rounds: u32,
        seed: u64,
    ) -> Option<BattleResult> {
        let mut slot = self.slots[k].lock().unwrap();
        let b = &self.bots[bot];
        let conn = slot.connect(b)?;
        let opts = BattleOptions {
            setup: Setup {
                rounds,
                ..Setup::default()
            },
            tps: None,
            seed,
        };
        let (live, stop, done) = (
            Mutex::new(Live::default()),
            AtomicBool::new(false),
            AtomicBool::new(false),
        );
        let t = Instant::now();
        let res = std::thread::scope(|s| {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) && t.elapsed() < BATTLE_LIMIT {
                    std::thread::sleep(Duration::from_millis(100));
                }
                stop.store(true, Ordering::Relaxed);
            });
            let r = run_battle(
                &slot.server,
                vec![Entrant::Local(pilot), Entrant::Remote(conn)],
                vec!["Candidate".into(), b.name.clone()],
                &opts,
                &live,
                &stop,
            );
            done.store(true, Ordering::Relaxed);
            r
        });
        match res {
            Ok(s) => Some(BattleResult {
                rounds,
                won: s[0].first_places,
                lost: s[1].first_places,
                dealt: s[0].bullet_damage,
                taken: s[1].bullet_damage,
            }),
            Err(_) => {
                slot.kill();
                None
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
