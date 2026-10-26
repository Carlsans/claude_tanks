//! Hosts a translated robot: its classic Robocode code, the Java bridge to
//! the Tank Royale bot API, and the bot API itself, all translated. The
//! bot API talks to its server through an in-process socket: the host
//! hands it the server's messages (`deliver`) and collects what it sends
//! (`take_outbox`), the same JSON a real server and bot exchange.
//!
//! Everything for one robot happens on the OS thread that called `start`.
use crate::reflect::{self, Val};
use crate::*;
use std::time::Duration;

thread_local! {
    static LISTENER: RefCell<Ref> = const { RefCell::new(None) };
    static SOCKET: RefCell<Ref> = const { RefCell::new(None) };
    static OUTBOX: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CLOSED: Cell<bool> = const { Cell::new(false) };
}

/// How long the host lets a robot's threads sleep before it returns.
const PATIENCE: Duration = Duration::from_millis(2);

pub fn attach(listener: Ref, socket: Ref) {
    LISTENER.with(|l| *l.borrow_mut() = listener);
    SOCKET.with(|s| *s.borrow_mut() = socket);
}

pub fn send(text: String) {
    OUTBOX.with(|o| o.borrow_mut().push(text));
}

pub fn close() {
    CLOSED.with(|c| c.set(true));
}

pub fn is_closed() -> bool {
    CLOSED.with(|c| c.get())
}

/// Messages the robot sent since the last call.
pub fn take_outbox() -> Vec<String> {
    let out = OUTBOX.with(|o| std::mem::take(&mut *o.borrow_mut()));
    if std::env::var_os("JRT_TRACE").is_some() {
        for m in &out {
            eprintln!("-> {}", m.chars().take(160).collect::<String>());
        }
    }
    out
}

/// What a robot says about itself (from its `.json` bot file).
pub struct Info {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
}

fn list(items: &[String]) -> JResult<Ref> {
    let c = reflect::class_named("java/util/ArrayList")
        .ok_or_else(|| unsupported("java/util/ArrayList"))?;
    let l = reflect::new_instance(c, "()V", &[])?;
    for s in items {
        reflect::call_virtual(&l, "add(Ljava/lang/Object;)Z", &[Val::A(jstr(s))])?;
    }
    Ok(l)
}

/// Starts a robot: creates the robot class, the bridge's `BotPeer` and the
/// bot API under it, which connects to the in-process socket. `classes` is
/// the translated program (`c::CLASSES`).
pub fn start(
    classes: &'static [&'static Class],
    robot_class: &str,
    info: Info,
) -> Result<(), String> {
    reflect::register(classes);
    // Robots print a lot; `JRT_PRINT` shows it (for debugging).
    set_printing(std::env::var_os("JRT_PRINT").is_some());
    let robot_class = robot_class.replace('.', "/");
    let failed: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let f2 = failed.clone();
    let main = thread::spawn(
        n::thread::new_object("main"),
        Box::new(move || {
            let run = || -> JResult<()> {
                let rc =
                    reflect::class_named(&robot_class).ok_or_else(|| unsupported(&robot_class))?;
                let robot = reflect::new_instance(rc, "()V", &[])?;
                let bi = reflect::class_named("dev/robocode/tankroyale/botapi/BotInfo")
                    .ok_or_else(|| unsupported("BotInfo"))?;
                let bot_info = reflect::new_instance(
                    bi,
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/util/List;Ljava/lang/String;Ljava/lang/String;Ljava/util/List;Ljava/util/Collection;Ljava/lang/String;Ljava/lang/String;Ldev/robocode/tankroyale/botapi/InitialPosition;)V",
                    &[
                        Val::A(jstr(&info.name)),
                        Val::A(jstr(&info.version)),
                        Val::A(list(&info.authors)?),
                        Val::A(None),
                        Val::A(None),
                        Val::A(list(&[])?),
                        Val::A(list(&[])?),
                        Val::A(jstr("JVM (translated to Rust)")),
                        Val::A(jstr("Java")),
                        Val::A(None),
                    ],
                )?;
                let pc = reflect::class_named("dev/robocode/tankroyale/bridge/BotPeer")
                    .ok_or_else(|| unsupported("BotPeer"))?;
                let peer = reflect::new_instance(
                    pc,
                    "(Lrobocode/robotinterfaces/IBasicRobot;Ldev/robocode/tankroyale/botapi/BotInfo;)V",
                    &[Val::A(robot), Val::A(bot_info)],
                )?;
                reflect::call_virtual(&peer, "start()V", &[])?;
                Ok(())
            };
            if let Err(t) = run() {
                *f2.borrow_mut() = Some(describe(&t));
            }
            Ok(())
        }),
    );
    let _ = main;
    thread::run_until_idle(PATIENCE);
    if let Some(e) = failed.borrow_mut().take() {
        return Err(e);
    }
    if LISTENER.with(|l| l.borrow().is_none()) {
        return Err("the robot did not connect".into());
    }
    Ok(())
}

/// The monitor the dispatcher waits on for messages (object addresses are
/// never this small).
const INBOX_MONITOR: usize = 8;

thread_local! {
    static INBOX: RefCell<std::collections::VecDeque<String>> = const { RefCell::new(std::collections::VecDeque::new()) };
    static DISPATCHER: Cell<bool> = const { Cell::new(false) };
}

/// The thread that hands messages to the bot API's listener, one at a
/// time, as the websocket client's thread does.
fn start_dispatcher() {
    if DISPATCHER.with(|d| d.replace(true)) {
        return;
    }
    thread::spawn(
        n::thread::new_object("websocket"),
        Box::new(|| {
            loop {
                let msg = loop {
                    if let Some(m) = INBOX.with(|i| i.borrow_mut().pop_front()) {
                        break m;
                    }
                    thread::block(thread::Block::Wait {
                        monitor: INBOX_MONITOR,
                        deadline: None,
                    })?;
                };
                let listener = LISTENER.with(|l| l.borrow().clone());
                let socket = SOCKET.with(|s| s.borrow().clone());
                let Some(l) = listener.clone() else { continue };
                let on_text: fn(Ref, Ref, Ref, i32) -> JResult<Ref> = unsafe {
                    std::mem::transmute(islot(&l, &C_java_net_http_WebSocket_Listener, 1)?)
                };
                if let Err(e) = on_text(listener, socket, jstr(&msg), 1) {
                    eprintln!("Exception handling a message: {}", describe(&e));
                }
            }
        }),
    );
}

/// Hands the robot a message from the server and runs it until it waits
/// again.
pub fn deliver(text: &str) {
    if std::env::var_os("JRT_TRACE").is_some() {
        eprintln!("<- {}", text.chars().take(160).collect::<String>());
    }
    start_dispatcher();
    INBOX.with(|i| i.borrow_mut().push_back(text.to_string()));
    thread::notify(INBOX_MONITOR, true);
    thread::run_until_idle(PATIENCE);
    if std::env::var_os("JRT_TRACE").is_some() {
        eprintln!("   threads: {}", thread::debug_state());
    }
}
