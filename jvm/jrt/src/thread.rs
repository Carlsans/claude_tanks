//! Java threads as coroutines ("green threads") on one OS thread.
//!
//! A translated program runs all its Java threads on the OS thread that
//! started it, switching only where a thread blocks (`wait`, `sleep`,
//! `join`, `yield`). So objects never cross OS threads, runs are
//! deterministic, and blocking calls like a robot's `execute()` suspend the
//! robot's `run()` loop until the engine sends the next turn.
//!
//! Monitors are not locked: a thread only loses the processor where it
//! blocks, so a critical section that does not block is already exclusive.
//! `wait` does not require owning the monitor.

use crate::*;
use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};
use std::time::{Duration, Instant};

/// Why a thread stopped running.
#[derive(Clone, Copy, Debug)]
pub enum Block {
    Yield,
    /// `Object.wait` on the object with this address, until notified (or
    /// the deadline).
    Wait {
        monitor: usize,
        deadline: Option<Instant>,
    },
    Sleep(Instant),
    /// Until the thread with this id ends.
    Join {
        thread: usize,
        deadline: Option<Instant>,
    },
}

#[derive(Clone, Copy)]
enum Wake {
    Run,
    Interrupted,
}

type Co = Coroutine<Wake, Block, JResult<()>, DefaultStack>;

struct Green {
    /// The `java.lang.Thread` object.
    obj: Ref,
    co: Option<Co>,
    yielder: *const Yielder<Wake, Block>,
    block: Option<Block>,
    notified: bool,
    interrupted: bool,
    done: bool,
}

#[derive(Default)]
struct Sched {
    threads: Vec<Green>,
    current: Option<usize>,
}

thread_local! {
    static SCHED: RefCell<Sched> = RefCell::new(Sched::default());
}

/// Each thread's stack (memory is only touched as it is used).
const STACK: usize = 4 << 20;

/// Starts a thread running `body`. Returns its id.
pub fn spawn(obj: Ref, body: Box<dyn FnOnce() -> JResult<()>>) -> usize {
    let id = SCHED.with(|s| s.borrow().threads.len());
    let co: Co = Coroutine::with_stack(
        DefaultStack::new(STACK).expect("thread stack"),
        move |y: &Yielder<Wake, Block>, _: Wake| {
            let p = y as *const Yielder<Wake, Block>;
            SCHED.with(|s| s.borrow_mut().threads[id].yielder = p);
            body()
        },
    );
    SCHED.with(|s| {
        s.borrow_mut().threads.push(Green {
            obj,
            co: Some(co),
            yielder: std::ptr::null(),
            block: Some(Block::Yield),
            notified: false,
            interrupted: false,
            done: false,
        })
    });
    id
}

/// The running thread's id, if a Java thread is running.
pub fn current() -> Option<usize> {
    SCHED.with(|s| s.borrow().current)
}

/// The running thread's `java.lang.Thread` object.
pub fn current_object() -> Ref {
    SCHED.with(|s| {
        let s = s.borrow();
        s.current.and_then(|i| s.threads[i].obj.clone())
    })
}

/// Stops the running thread until `b` is over. An interrupt ends the wait
/// with `InterruptedException`. Outside any Java thread it returns at once.
pub fn block(b: Block) -> JResult<()> {
    let Some((y, interrupted)) = SCHED.with(|s| {
        let mut s = s.borrow_mut();
        let i = s.current?;
        let t = &mut s.threads[i];
        let pending = t.interrupted && !matches!(b, Block::Yield);
        if pending {
            t.interrupted = false;
        }
        Some((t.yielder, pending))
    }) else {
        return Ok(());
    };
    if interrupted {
        return Err(throw(
            &C_java_lang_InterruptedException,
            "sleep interrupted",
        ));
    }
    // SAFETY: the yielder belongs to the running coroutine, which is this
    // call's own stack.
    match unsafe { (*y).suspend(b) } {
        Wake::Run => Ok(()),
        Wake::Interrupted => Err(throw(&C_java_lang_InterruptedException, "interrupted")),
    }
}

/// Wakes threads waiting on a monitor (all, or the first).
pub fn notify(monitor: usize, all: bool) {
    SCHED.with(|s| {
        for t in s.borrow_mut().threads.iter_mut() {
            if let Some(Block::Wait { monitor: m, .. }) = t.block
                && m == monitor
                && !t.notified
            {
                t.notified = true;
                if !all {
                    break;
                }
            }
        }
    });
}

pub fn interrupt(id: usize) {
    SCHED.with(|s| {
        if let Some(t) = s.borrow_mut().threads.get_mut(id) {
            t.interrupted = true;
        }
    });
}

/// Reads and (if `clear`) clears a thread's interrupt flag.
pub fn interrupted(id: usize, clear: bool) -> bool {
    SCHED.with(|s| {
        let mut s = s.borrow_mut();
        let Some(t) = s.threads.get_mut(id) else {
            return false;
        };
        let v = t.interrupted;
        if clear {
            t.interrupted = false;
        }
        v
    })
}

pub fn is_alive(id: usize) -> bool {
    SCHED.with(|s| s.borrow().threads.get(id).is_some_and(|t| !t.done))
}

/// Whether the thread can run now, and how to wake it.
fn ready(t: &Green, now: Instant, done: &[bool]) -> Option<Wake> {
    if t.done || t.co.is_none() {
        return None;
    }
    let interruptible = !matches!(t.block, Some(Block::Yield) | None);
    if interruptible && t.interrupted {
        return Some(Wake::Interrupted);
    }
    match t.block? {
        Block::Yield => Some(Wake::Run),
        Block::Wait { deadline, .. } => {
            (t.notified || deadline.is_some_and(|d| now >= d)).then_some(Wake::Run)
        }
        Block::Sleep(d) => (now >= d).then_some(Wake::Run),
        Block::Join { thread, deadline } => (done.get(thread).copied().unwrap_or(true)
            || deadline.is_some_and(|d| now >= d))
        .then_some(Wake::Run),
    }
}

/// The soonest deadline among blocked threads, if any.
fn next_deadline() -> Option<Instant> {
    SCHED.with(|s| {
        s.borrow()
            .threads
            .iter()
            .filter(|t| !t.done)
            .filter_map(|t| match t.block? {
                Block::Sleep(d) => Some(d),
                Block::Wait { deadline, .. } | Block::Join { deadline, .. } => deadline,
                Block::Yield => None,
            })
            .min()
    })
}

/// Runs threads until none can run. Threads asleep (or waiting with a
/// deadline) up to `patience` from now are waited for.
pub fn run_until_idle(patience: Duration) {
    let until = Instant::now() + patience;
    loop {
        let now = Instant::now();
        let mut ran = false;
        let n = SCHED.with(|s| s.borrow().threads.len());
        for i in 0..n {
            let wake = SCHED.with(|s| {
                let s = s.borrow();
                let done: Vec<bool> = s.threads.iter().map(|t| t.done).collect();
                ready(&s.threads[i], now, &done)
            });
            let Some(wake) = wake else { continue };
            ran = true;
            let mut co = SCHED
                .with(|s| {
                    let mut s = s.borrow_mut();
                    s.current = Some(i);
                    let t = &mut s.threads[i];
                    if matches!(wake, Wake::Interrupted) {
                        t.interrupted = false;
                    }
                    t.notified = false;
                    t.block = None;
                    t.co.take()
                })
                .expect("a ready thread has its coroutine");
            let r = co.resume(wake);
            SCHED.with(|s| {
                let mut s = s.borrow_mut();
                s.current = None;
                let t = &mut s.threads[i];
                match r {
                    CoroutineResult::Yield(b) => {
                        t.block = Some(b);
                        t.co = Some(co);
                    }
                    CoroutineResult::Return(res) => {
                        t.done = true;
                        if let Err(e) = res
                            && printing()
                        {
                            eprintln!("Exception in thread {}: {}", i, describe(&e));
                        }
                    }
                }
            });
        }
        if !ran {
            match next_deadline() {
                Some(d) if d <= until => {
                    std::thread::sleep(d.saturating_duration_since(Instant::now()))
                }
                _ => return,
            }
        }
    }
}

/// Runs `main` as the program's main thread until every thread is idle or
/// done, the way a JVM runs `main`. Returns its uncaught exception.
pub fn run_main_thread(main: Box<dyn FnOnce() -> JResult<()>>) -> Option<Throw> {
    let err: Rc<RefCell<Option<Throw>>> = Rc::new(RefCell::new(None));
    let e2 = err.clone();
    let obj = crate::n::thread::new_object("main");
    spawn(
        obj,
        Box::new(move || {
            if let Err(t) = main() {
                *e2.borrow_mut() = Some(t);
            }
            Ok(())
        }),
    );
    run_until_idle(Duration::from_secs(3600));
    err.borrow_mut().take()
}
