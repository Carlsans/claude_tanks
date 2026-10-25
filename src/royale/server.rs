//! A Tank Royale server: bots connect over WebSocket (default port 7654, as
//! the official server), then battle here against each other or our pilots.

use super::Setup;
use super::battle::{Battle, Intent, Score, Turn};
use super::pilot::{Pilot, View};
use super::protocol::{self, BotHandshake, BotIntent, FromBot};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::BTreeMap;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 7654;

/// Java options for every bot `BotDir::launch` starts. By default a JVM may
/// grow to a quarter of the memory, and robots that keep learning do grow:
/// 32 of them pushed a 32 GB machine deep into swap. Classic Robocode ran all
/// the robots of a battle in one 512 MB JVM.
const BOT_JAVA_OPTIONS: &str = "-Xmx256m -XX:+UseSerialGC";

/// A connected bot.
#[derive(Clone, Debug)]
pub struct Conn {
    pub id: u64,
    /// Set once the bot has introduced itself.
    pub handshake: Option<BotHandshake>,
    tx: Sender<String>,
}

enum Incoming {
    Ready,
    Intent(Box<BotIntent>),
    Closed,
}

pub struct Server {
    pub port: u16,
    conns: Arc<Mutex<BTreeMap<u64, Conn>>>,
    incoming: Receiver<(u64, Incoming)>,
    stop: Arc<AtomicBool>,
}

impl Server {
    /// Listens on `127.0.0.1:port` (0 picks a free port).
    pub fn start(port: u16) -> std::io::Result<Server> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let conns: Arc<Mutex<BTreeMap<u64, Conn>>> = Arc::default();
        let (in_tx, incoming) = unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let (c, s) = (conns.clone(), stop.clone());
        std::thread::spawn(move || {
            let next = AtomicU64::new(1);
            while !s.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let id = next.fetch_add(1, Ordering::Relaxed);
                        let (c, s, in_tx) = (c.clone(), s.clone(), in_tx.clone());
                        std::thread::spawn(move || serve(stream, id, c, in_tx, s));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        });
        Ok(Server {
            port,
            conns,
            incoming,
            stop,
        })
    }

    pub fn url(&self) -> String {
        format!("ws://localhost:{}", self.port)
    }

    /// Bots that have introduced themselves.
    pub fn bots(&self) -> Vec<(u64, BotHandshake)> {
        self.conns
            .lock()
            .unwrap()
            .values()
            .filter_map(|c| c.handshake.clone().map(|h| (c.id, h)))
            .collect()
    }

    /// Waits until a bot called `name` is connected and not in `taken`.
    pub fn wait_for(&self, name: &str, taken: &[u64], timeout: Duration) -> Option<u64> {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if let Some((id, _)) = self
                .bots()
                .into_iter()
                .find(|(id, h)| h.name == name && !taken.contains(id))
            {
                return Some(id);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }

    fn send(&self, conn: u64, msg: &serde_json::Value) {
        if let Some(c) = self.conns.lock().unwrap().get(&conn) {
            let _ = c.tx.send(msg.to_string());
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// One end of a connection's socket. The reader and the writer each drive
/// their own WebSocket on a clone of the socket; every write takes the shared
/// lock, so the reader's automatic pongs cannot cut into a frame being sent.
struct Half {
    s: TcpStream,
    lock: Arc<Mutex<()>>,
}

impl std::io::Read for Half {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.s.read(buf)
    }
}

impl std::io::Write for Half {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _g = self.lock.lock().unwrap();
        self.s.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.s.flush()
    }
}

/// Serves one bot. A blocking reader and a writer woken by the outgoing
/// channel: polling with socket timeouts would cost up to a kernel tick
/// (several ms) per message.
fn serve(
    stream: TcpStream,
    id: u64,
    conns: Arc<Mutex<BTreeMap<u64, Conn>>>,
    in_tx: Sender<(u64, Incoming)>,
    stop: Arc<AtomicBool>,
) {
    use tungstenite::protocol::Role;
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let Ok(ws) = tungstenite::accept(stream) else {
        return;
    };
    // Nothing is buffered yet: the bot waits for our handshake before talking.
    let (Ok(r), Ok(w)) = (ws.get_ref().try_clone(), ws.get_ref().try_clone()) else {
        return;
    };
    drop(ws);
    let lock = Arc::new(Mutex::new(()));
    let mut reader = tungstenite::WebSocket::from_raw_socket(
        Half {
            s: r,
            lock: lock.clone(),
        },
        Role::Server,
        None,
    );
    let mut writer =
        tungstenite::WebSocket::from_raw_socket(Half { s: w, lock }, Role::Server, None);
    let (tx, rx) = unbounded::<String>();
    let session = format!("claude-tanks-{id}");
    let hello = protocol::server_handshake(&session, &Setup::default());
    if writer
        .send(tungstenite::Message::text(hello.to_string()))
        .is_err()
    {
        return;
    }
    conns.lock().unwrap().insert(
        id,
        Conn {
            id,
            handshake: None,
            tx,
        },
    );
    // The writer ends when the connection is forgotten (its sender dropped).
    std::thread::spawn(move || {
        while let Ok(m) = rx.recv() {
            if writer.send(tungstenite::Message::text(m)).is_err() {
                break;
            }
        }
    });
    while !stop.load(Ordering::Relaxed) {
        match reader.read() {
            Ok(tungstenite::Message::Text(t)) => {
                match serde_json::from_str::<FromBot>(t.as_str()) {
                    Ok(FromBot::BotHandshake(h)) => {
                        if let Some(c) = conns.lock().unwrap().get_mut(&id) {
                            c.handshake = Some(h);
                        }
                    }
                    Ok(FromBot::BotReady) => {
                        let _ = in_tx.send((id, Incoming::Ready));
                    }
                    Ok(FromBot::BotIntent(i)) => {
                        let _ = in_tx.send((id, Incoming::Intent(Box::new(i))));
                    }
                    Ok(FromBot::Other) | Err(_) => {}
                }
            }
            Ok(tungstenite::Message::Close(_)) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    conns.lock().unwrap().remove(&id);
    let _ = in_tx.send((id, Incoming::Closed));
}

/// One side of a battle.
pub enum Entrant {
    /// A connected bot (connection id).
    Remote(u64),
    Local(Box<dyn Pilot>),
}

/// What a display needs to follow a battle.
#[derive(Default)]
pub struct Live {
    pub names: Vec<String>,
    pub setup: Setup,
    pub turn: Option<Turn>,
    pub results: Vec<Score>,
    pub log: Vec<String>,
    pub finished: bool,
    /// Turns played, and those that went ahead without every living bot's intent.
    pub turns: u64,
    pub timeouts: u64,
}

pub struct BattleOptions {
    pub setup: Setup,
    /// Turns per second; `None` runs as fast as the bots answer.
    pub tps: Option<f64>,
    pub seed: u64,
}

/// Runs a battle to the end (or until `stop`). Returns each entrant's total score.
pub fn run_battle(
    server: &Server,
    mut entrants: Vec<Entrant>,
    names: Vec<String>,
    opts: &BattleOptions,
    live: &Mutex<Live>,
    stop: &AtomicBool,
) -> Result<Vec<Score>, String> {
    let setup = &opts.setup;
    let n = entrants.len();
    {
        let mut l = live.lock().unwrap();
        *l = Live {
            names: names.clone(),
            setup: setup.clone(),
            ..Live::default()
        };
    }
    let log = |s: String| live.lock().unwrap().log.push(s);
    let remote: Vec<Option<u64>> = entrants
        .iter()
        .map(|e| match e {
            Entrant::Remote(c) => Some(*c),
            Entrant::Local(_) => None,
        })
        .collect();
    let slot = |conn: u64| remote.iter().position(|r| *r == Some(conn));
    // Forget messages from before this battle.
    while server.incoming.try_recv().is_ok() {}

    // Everybody gets the game setup and must answer "ready".
    for (k, r) in remote.iter().enumerate() {
        if let Some(c) = r {
            server.send(
                *c,
                &protocol::game_started(k as u32 + 1, None, &names, setup),
            );
        }
    }
    let mut ready = vec![false; n];
    for (k, r) in remote.iter().enumerate() {
        ready[k] = r.is_none();
    }
    let deadline = Instant::now() + Duration::from_micros(setup.ready_timeout_us as u64);
    while !ready.iter().all(|r| *r) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || stop.load(Ordering::Relaxed) {
            abort(server, &remote);
            return Err("a bot did not get ready in time".into());
        }
        if let Ok((c, m)) = server.incoming.recv_timeout(left)
            && let Some(k) = slot(c)
        {
            match m {
                Incoming::Ready => ready[k] = true,
                Incoming::Closed => {
                    abort(server, &remote);
                    return Err(format!("{} left before the battle", names[k]));
                }
                Incoming::Intent(_) => {}
            }
        }
    }
    log(format!("Battle started: {}", names.join(" vs ")));

    let mut battle = Battle::new(setup.clone(), n, opts.seed);
    let mut pending: Vec<Option<BotIntent>> = vec![None; n];
    let mut local_next: Vec<Option<Intent>> = vec![None; n];
    let mut gone = vec![false; n];
    let mut next_turn_at = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            abort(server, &remote);
            live.lock().unwrap().finished = true;
            return Err("stopped".into());
        }
        let responded: Vec<bool> = pending.iter().map(|p| p.is_some()).collect();
        let intents: Vec<(u32, Intent)> = (0..n)
            .filter_map(|k| {
                let i = match remote[k] {
                    Some(_) => pending[k].take().map(|p| p.to_intent()),
                    None => local_next[k].take(),
                };
                i.map(|i| (k as u32 + 1, i))
            })
            .collect();
        let turn = battle.step(&intents);

        if turn.turn == 1 {
            for c in remote.iter().flatten() {
                server.send(*c, &protocol::round_started(turn.round));
            }
            for e in entrants.iter_mut() {
                if let Entrant::Local(p) = e {
                    p.reset();
                }
            }
        } else {
            for k in 0..n {
                let alive = battle.bots().iter().any(|b| b.id == k as u32 + 1);
                if let Some(c) = remote[k]
                    && alive
                    && !responded[k]
                    && !gone[k]
                {
                    server.send(c, &protocol::skipped_turn(turn.turn - 1));
                }
            }
        }
        for (k, e) in entrants.iter_mut().enumerate() {
            let id = k as u32 + 1;
            let Some(me) = turn.bot(id) else { continue };
            let events = &turn.events[k];
            if !me.alive() && events.is_empty() {
                continue;
            }
            let enemies = turn.bots.iter().filter(|b| b.alive() && b.id != id).count();
            let bullets: Vec<_> = turn
                .bullets
                .iter()
                .filter(|b| b.owner == id)
                .map(|b| b.state())
                .collect();
            match e {
                Entrant::Remote(c) => server.send(
                    *c,
                    &protocol::tick(turn.round, turn.turn, me, enemies, &bullets, events),
                ),
                Entrant::Local(p) if me.alive() => {
                    local_next[k] = Some(p.act(&View {
                        setup,
                        turn: turn.turn,
                        me,
                        bullets: &bullets,
                        events,
                    }));
                }
                Entrant::Local(_) => {}
            }
        }
        if turn.round_ended {
            let ranks = ranks(&battle.results);
            for (k, c) in remote.iter().enumerate() {
                if let Some(c) = c {
                    server.send(
                        *c,
                        &protocol::round_ended(turn.round, turn.turn, &battle.results[k], ranks[k]),
                    );
                }
            }
            let won: Vec<&str> = turn
                .winners
                .iter()
                .map(|w| names[*w as usize - 1].as_str())
                .collect();
            log(format!(
                "Round {} won by {} in {} turns",
                turn.round,
                won.join(" and "),
                turn.turn
            ));
        }
        {
            let mut l = live.lock().unwrap();
            l.results = battle.results.clone();
            l.turn = Some(turn.clone());
            l.turns += 1;
        }
        if turn.game_ended {
            let ranks = ranks(&battle.results);
            for (k, c) in remote.iter().enumerate() {
                if let Some(c) = c {
                    server.send(
                        *c,
                        &protocol::game_ended(setup.rounds, &battle.results[k], ranks[k]),
                    );
                }
            }
            live.lock().unwrap().finished = true;
            return Ok(battle.results.clone());
        }

        // Wait for the living remote bots' intents (or the turn timeout), then
        // hold the pace.
        let started = Instant::now();
        let timeout = Duration::from_micros(setup.turn_timeout_us as u64);
        loop {
            let waiting = (0..n).any(|k| {
                remote[k].is_some()
                    && !gone[k]
                    && pending[k].is_none()
                    && battle.bots().iter().any(|b| b.id == k as u32 + 1)
            });
            let left = timeout.saturating_sub(started.elapsed());
            if !waiting || left.is_zero() {
                if waiting {
                    live.lock().unwrap().timeouts += 1;
                }
                break;
            }
            if let Ok((c, m)) = server.incoming.recv_timeout(left)
                && let Some(k) = slot(c)
            {
                match m {
                    Incoming::Intent(i) => match &mut pending[k] {
                        Some(p) => p.merge(*i),
                        None => pending[k] = Some(*i),
                    },
                    Incoming::Closed => {
                        gone[k] = true;
                        log(format!("{} disconnected", names[k]));
                    }
                    Incoming::Ready => {}
                }
            }
        }
        for k in 0..n {
            if let Some(i) = &mut pending[k]
                && let Some(out) = i.std_out.take()
                && !out.trim().is_empty()
            {
                log(format!("{}: {}", names[k], out.trim_end()));
            }
        }
        if let Some(tps) = opts.tps {
            next_turn_at += Duration::from_secs_f64(1.0 / tps.max(0.1));
            let now = Instant::now();
            if next_turn_at > now {
                std::thread::sleep(next_turn_at - now);
            } else {
                next_turn_at = now;
            }
        }
    }
}

fn abort(server: &Server, remote: &[Option<u64>]) {
    for c in remote.iter().flatten() {
        server.send(*c, &protocol::game_aborted());
    }
}

/// Rank by total score (1 = best).
pub fn ranks(scores: &[Score]) -> Vec<usize> {
    scores
        .iter()
        .map(|s| 1 + scores.iter().filter(|o| o.total() > s.total()).count())
        .collect()
}

/// A bot directory: `<dir>/<name>.json` plus a script or a source file.
#[derive(Clone, Debug)]
pub struct BotDir {
    pub dir: PathBuf,
    /// Name the bot gives in its handshake.
    pub name: String,
    pub description: String,
}

impl BotDir {
    pub fn read(dir: &Path) -> Option<BotDir> {
        let base = dir.file_name()?.to_str()?.to_string();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{base}.json"))).ok()?)
                .ok()?;
        // Teams are not single bots.
        if json.get("teamMembers").is_some() {
            return None;
        }
        Some(BotDir {
            dir: dir.to_path_buf(),
            name: json["name"].as_str().unwrap_or(&base).to_string(),
            description: json["description"].as_str().unwrap_or("").to_string(),
        })
    }

    /// All bot directories under `root`, and one level down (e.g. `bots/java/Walls`
    /// and `bots/classic/sample.Walls`), by name.
    pub fn scan(root: &Path) -> Vec<BotDir> {
        let dirs = |d: &Path| -> Vec<PathBuf> {
            std::fs::read_dir(d)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        };
        let mut v = Vec::new();
        for d in dirs(root) {
            match BotDir::read(&d) {
                Some(b) => v.push(b),
                None => v.extend(dirs(&d).iter().filter_map(|s| BotDir::read(s))),
            }
        }
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Starts the bot as the official booter would: its script if there is
    /// one, else `java -cp ../lib/* <base>.java` or `python3 <base>.py`.
    pub fn launch(&self, server_url: &str) -> std::io::Result<Child> {
        let base = self.dir.file_name().unwrap().to_string_lossy().to_string();
        let script = if cfg!(windows) {
            self.dir.join(format!("{base}.cmd"))
        } else {
            self.dir.join(format!("{base}.sh"))
        };
        // Scripts run from the bot directory, so they are named relative to it.
        let cmd = if script.exists() {
            let file = script.file_name().unwrap();
            if cfg!(windows) {
                let mut c = Command::new("cmd");
                c.arg("/C").arg(file);
                c
            } else {
                let mut c = Command::new("sh");
                c.arg(file);
                c
            }
        } else if self.dir.join(format!("{base}.java")).exists() {
            let sep = if cfg!(windows) { ";" } else { ":" };
            let mut c = Command::new("java");
            c.arg("-cp")
                .arg(format!("..{0}lib{0}*{sep}.", std::path::MAIN_SEPARATOR))
                .arg(format!("{base}.java"));
            c
        } else if self.dir.join(format!("{base}.py")).exists() {
            let mut c = Command::new(if cfg!(windows) { "python" } else { "python3" });
            c.arg(format!("{base}.py"));
            c
        } else {
            return Err(std::io::Error::other(format!(
                "no script, .java or .py file in {}",
                self.dir.display()
            )));
        };
        // Options already in the environment come last, so they win.
        let java_options = match std::env::var("JDK_JAVA_OPTIONS") {
            Ok(own) => format!("{BOT_JAVA_OPTIONS} {own}"),
            Err(_) => BOT_JAVA_OPTIONS.to_string(),
        };
        let mut cmd = sandbox(&cmd, &self.dir).unwrap_or(cmd);
        cmd.current_dir(&self.dir)
            .env("SERVER_URL", server_url)
            .env("JDK_JAVA_OPTIONS", java_options)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    }
}

/// The sandbox `BotDir::launch` starts bots in, if any: bubblewrap on Linux
/// when it is installed, unless `CLAUDE_TANKS_NO_SANDBOX` is set.
pub fn sandbox_name() -> Option<&'static str> {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let found = *FOUND.get_or_init(|| {
        cfg!(target_os = "linux")
            && Command::new("bwrap")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
    });
    (found && std::env::var_os("CLAUDE_TANKS_NO_SANDBOX").is_none()).then_some("bubblewrap")
}

/// `cmd` wrapped in bubblewrap: the system is read-only, the home folder is
/// hidden except the bot's collection (read-only) and the bot's own directory
/// (writable: classic robots keep data files there). Bots still reach the
/// network, which they need to join the server. Killing the returned process
/// ends everything the bot started.
fn sandbox(cmd: &Command, dir: &Path) -> Option<Command> {
    sandbox_name()?;
    let dir = dir.canonicalize().ok()?;
    let collection = dir.parent()?;
    let mut b = Command::new("bwrap");
    b.args(["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"]);
    b.args(["--tmpfs", "/tmp"]);
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        b.arg("--tmpfs").arg(&home);
        // Python packages, and a JDK or Python installed in the home folder.
        let mut keep = vec![home.join(".local")];
        for exe in ["java", "python3"] {
            if let Some(root) = install_root(exe) {
                keep.push(root);
            }
        }
        for k in keep.iter().filter(|k| k.starts_with(&home) && k.is_dir()) {
            b.arg("--ro-bind").arg(k).arg(k);
        }
    }
    b.arg("--ro-bind").arg(collection).arg(collection);
    b.arg("--bind").arg(&dir).arg(&dir);
    b.args(["--unshare-pid", "--unshare-ipc", "--unshare-uts"]);
    b.args(["--die-with-parent", "--new-session", "--chdir"])
        .arg(&dir);
    b.arg("--").arg(cmd.get_program()).args(cmd.get_args());
    Some(b)
}

/// The installation folder of a program on the PATH (`<root>/bin/<exe>`).
fn install_root(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let found = std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())?;
    Some(found.canonicalize().ok()?.parent()?.parent()?.to_path_buf())
}
