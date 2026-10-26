//! A cycle collector. Objects are reference counted, so a cycle of objects
//! that only refer to each other (a doubly linked list, a tree with parent
//! pointers, a stream pipeline) is never freed, though the JVM would collect
//! it. Every object that can hold references is registered; `collect` finds
//! the ones only reachable from such cycles and clears their fields, which
//! frees them.
//!
//! An object is externally referenced when it has more strong references
//! than other registered objects account for: from a local variable, a
//! static field, a thread's stack, the runtime's caches. Those are the roots;
//! whatever they reach is live, the rest is garbage. Collection runs only
//! where no Java code is running (between messages to a robot).
use crate::*;

thread_local! {
    /// The registered objects. An object leaves the list when it is freed
    /// (`Obj::drop`), so the list holds only live ones.
    static ALL: RefCell<Vec<*const Obj>> = const { RefCell::new(Vec::new()) };
    static SINCE: Cell<usize> = const { Cell::new(0) };
    static EVERY: Cell<usize> = const { Cell::new(MIN_EVERY) };
}

/// Fewest registrations between collections. A collection visits every
/// registered object, so the wait also grows with what survived: the cost
/// per object made stays constant however large the robot's data is.
const MIN_EVERY: usize = 100_000;

pub fn register(o: &Rc<Obj>) {
    ALL.with(|a| {
        let mut a = a.borrow_mut();
        o.slot.set(a.len() as u32);
        a.push(Rc::as_ptr(o));
    });
    SINCE.with(|s| s.set(s.get() + 1));
}

/// Takes the object in `slot` off the list (it is being freed).
pub fn unregister(slot: u32) {
    let _ = ALL.try_with(|a| {
        let mut a = a.borrow_mut();
        let i = slot as usize;
        a.swap_remove(i);
        if let Some(&moved) = a.get(i) {
            // The object that took its place is alive: it is on the list.
            unsafe { (*moved).slot.set(slot) };
        }
    });
}

/// Collects if enough objects were made since the last collection.
pub fn maybe_collect() -> usize {
    if SINCE.with(|s| s.get()) < EVERY.with(|e| e.get()) {
        return 0;
    }
    collect()
}

/// The references an object holds, or `None` if one of its fields is being
/// borrowed (it is then treated as a root).
fn edges(o: &Obj) -> Option<Vec<Rc<Obj>>> {
    let mut out = Vec::new();
    for r in o.refs.iter() {
        if let Some(x) = r.try_borrow().ok()?.as_ref() {
            out.push(x.clone());
        }
    }
    if let Ext::Refs(v) = &o.ext {
        out.extend(v.try_borrow().ok()?.iter().flatten().cloned());
    }
    Some(out)
}

/// Frees unreachable cycles. Returns how many objects it cleared.
pub fn collect() -> usize {
    SINCE.with(|s| s.set(0));
    // Our own reference to each, so none is freed (and unlisted) meanwhile.
    let objs: Vec<Rc<Obj>> = ALL
        .with(|a| a.borrow().clone())
        .into_iter()
        .map(|p| unsafe {
            Rc::increment_strong_count(p);
            Rc::from_raw(p)
        })
        .collect();
    const UNSEEN: u32 = u32::MAX;
    for (i, o) in objs.iter().enumerate() {
        o.gc.set(i as u32);
    }
    let index = |x: &Rc<Obj>| {
        let i = x.gc.get();
        ((i as usize) < objs.len() && Rc::ptr_eq(&objs[i as usize], x)).then_some(i as usize)
    };
    // References each object receives from registered objects.
    let mut internal = vec![0usize; objs.len()];
    let mut out: Vec<Option<Vec<usize>>> = Vec::with_capacity(objs.len());
    for o in &objs {
        match edges(o) {
            Some(e) => {
                let idx: Vec<usize> = e.iter().filter_map(index).collect();
                for &j in &idx {
                    internal[j] += 1;
                }
                out.push(Some(idx));
            }
            None => out.push(None),
        }
    }
    // Roots: references from elsewhere (our own `objs` holds one, and each
    // `edges` list was dropped already).
    let mut live = vec![false; objs.len()];
    let mut stack: Vec<usize> = (0..objs.len())
        .filter(|&i| out[i].is_none() || Rc::strong_count(&objs[i]) - 1 > internal[i])
        .collect();
    for &i in &stack {
        live[i] = true;
    }
    while let Some(i) = stack.pop() {
        if let Some(e) = &out[i] {
            for &j in e {
                if !live[j] {
                    live[j] = true;
                    stack.push(j);
                }
            }
        }
    }
    let mut cleared = 0;
    for (i, o) in objs.iter().enumerate() {
        o.gc.set(UNSEEN);
        if live[i] {
            continue;
        }
        cleared += 1;
        for r in o.refs.iter() {
            if let Ok(mut b) = r.try_borrow_mut() {
                *b = None;
            }
        }
        if let Ext::Refs(v) = &o.ext
            && let Ok(mut b) = v.try_borrow_mut()
        {
            b.clear();
        }
    }
    let alive = live.iter().filter(|l| **l).count();
    EVERY.with(|e| e.set(MIN_EVERY.max(3 * alive)));
    drop(objs);
    cleared
}
