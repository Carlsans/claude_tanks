//! `java.lang.Thread` on the green-thread scheduler (`crate::thread`).
//! Storage: `refs` = [name, target]; `prims` = [scheduler id + 1 (0 before
//! `start`), daemon, priority].
use crate::thread::{self, Block};
use crate::*;
use std::time::{Duration, Instant};

/// `run()`'s method-table slot: Object's five come first.
const RUN: usize = 5;

thread_local! {
    static NEXT: Cell<u32> = const { Cell::new(0) };
}

fn next_name() -> String {
    NEXT.with(|n| {
        let v = n.get();
        n.set(v + 1);
        format!("Thread-{v}")
    })
}

/// A Thread object that is not started (for the main thread).
pub fn new_object(name: &str) -> Ref {
    let t = alloc(&C_java_lang_Thread);
    *t.as_ref().unwrap().refs[0].borrow_mut() = jstr(name);
    t.as_ref().unwrap().prims[2].set(5);
    t
}

fn id(this: &Ref) -> JResult<Option<usize>> {
    let v = nn(this)?.prims[0].get();
    Ok((v > 0).then(|| v as usize - 1))
}

fn setup(this: &Ref, target: Ref, name: Ref) -> JResult<()> {
    let o = nn(this)?;
    *o.refs[0].borrow_mut() = match name {
        Some(n) => Some(n),
        None => jstr(&next_name()),
    };
    *o.refs[1].borrow_mut() = target;
    o.prims[2].set(5);
    Ok(())
}

pub fn init(this: Ref) -> JResult<()> {
    setup(&this, None, None)
}

pub fn init_target(this: Ref, target: Ref) -> JResult<()> {
    setup(&this, target, None)
}

pub fn init_name(this: Ref, name: Ref) -> JResult<()> {
    setup(&this, None, name)
}

pub fn init_target_name(this: Ref, target: Ref, name: Ref) -> JResult<()> {
    setup(&this, target, name)
}

/// `Thread(ThreadGroup, Runnable[, String])`: groups are ignored.
pub fn init_group_target(this: Ref, _group: Ref, target: Ref) -> JResult<()> {
    setup(&this, target, None)
}

pub fn init_group_target_name(this: Ref, _group: Ref, target: Ref, name: Ref) -> JResult<()> {
    setup(&this, target, name)
}

pub fn start(this: Ref) -> JResult<()> {
    let o = nn(&this)?.clone();
    if o.prims[0].get() != 0 {
        return Err(throw(
            &C_java_lang_IllegalStateException,
            "thread already started",
        ));
    }
    let me = Some(o.clone());
    let body = Box::new(move || {
        let o = me.as_ref().unwrap();
        let f: fn(Ref) -> JResult<()> = unsafe { std::mem::transmute(vslot(o, RUN)?) };
        f(me.clone())
    });
    let id = thread::spawn(Some(o.clone()), body);
    o.prims[0].set(id as u64 + 1);
    Ok(())
}

/// The default `run()`: the target's, if any.
pub fn run(this: Ref) -> JResult<()> {
    let target = nn(&this)?.refs[1].borrow().clone();
    match &target {
        Some(t) => {
            let f: fn(Ref) -> JResult<()> =
                unsafe { std::mem::transmute(islot(t, &C_java_lang_Runnable, 0)?) };
            f(target.clone())
        }
        None => Ok(()),
    }
}

pub fn current_thread() -> JResult<Ref> {
    Ok(thread::current_object())
}

pub fn sleep(ms: i64) -> JResult<()> {
    if ms < 0 {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            "timeout value is negative",
        ));
    }
    thread::block(Block::Sleep(
        Instant::now() + Duration::from_millis(ms as u64),
    ))
}

pub fn sleep_nanos(ms: i64, _nanos: i32) -> JResult<()> {
    sleep(ms)
}

pub fn yield_now() -> JResult<()> {
    thread::block(Block::Yield)
}

pub fn spin_wait() -> JResult<()> {
    Ok(())
}

pub fn interrupted() -> JResult<i32> {
    Ok(thread::current().is_some_and(|i| thread::interrupted(i, true)) as i32)
}

pub fn interrupt(this: Ref) -> JResult<()> {
    if let Some(i) = id(&this)? {
        thread::interrupt(i);
    }
    Ok(())
}

pub fn is_interrupted(this: Ref) -> JResult<i32> {
    Ok(id(&this)?.is_some_and(|i| thread::interrupted(i, false)) as i32)
}

pub fn is_alive(this: Ref) -> JResult<i32> {
    Ok(id(&this)?.is_some_and(thread::is_alive) as i32)
}

pub fn join(this: Ref) -> JResult<()> {
    join_ms(this, 0)
}

pub fn join_ms(this: Ref, ms: i64) -> JResult<()> {
    let Some(i) = id(&this)? else {
        return Ok(());
    };
    if !thread::is_alive(i) {
        return Ok(());
    }
    let deadline = (ms > 0).then(|| Instant::now() + Duration::from_millis(ms as u64));
    thread::block(Block::Join {
        thread: i,
        deadline,
    })
}

pub fn get_name(this: Ref) -> JResult<Ref> {
    Ok(nn(&this)?.refs[0].borrow().clone())
}

pub fn set_name(this: Ref, name: Ref) -> JResult<()> {
    *nn(&this)?.refs[0].borrow_mut() = name;
    Ok(())
}

pub fn set_daemon(this: Ref, on: i32) -> JResult<()> {
    nn(&this)?.prims[1].set(on as u64);
    Ok(())
}

pub fn is_daemon(this: Ref) -> JResult<i32> {
    Ok(nn(&this)?.prims[1].get() as i32)
}

pub fn set_priority(this: Ref, p: i32) -> JResult<()> {
    nn(&this)?.prims[2].set(p as u64);
    Ok(())
}

pub fn get_priority(this: Ref) -> JResult<i32> {
    Ok(nn(&this)?.prims[2].get() as i32)
}

pub fn get_id(this: Ref) -> JResult<i64> {
    Ok(nn(&this)?.prims[0].get() as i64)
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    let name = nn(&this)?.refs[0].borrow().clone();
    let n = name.as_ref().map_or(String::new(), |n| rust_string(n));
    Ok(jstr(&format!("Thread[{n},5,main]")))
}

/// `setUncaughtExceptionHandler` and the like: accepted, unused.
pub fn ignore_ref(_this: Ref, _x: Ref) -> JResult<()> {
    Ok(())
}
