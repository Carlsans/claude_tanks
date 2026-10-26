//! `java.lang.System`.
use crate::*;

pub fn arraycopy(src: Ref, sp: i32, dst: Ref, dp: i32, n: i32) -> JResult<()> {
    let (s, d) = (nn(&src)?, nn(&dst)?);
    let (sl, dl) = (alen(&src)?, alen(&dst)?);
    if sp < 0
        || dp < 0
        || n < 0
        || sp as i64 + n as i64 > sl as i64
        || dp as i64 + n as i64 > dl as i64
    {
        return Err(throw(
            &C_java_lang_ArrayIndexOutOfBoundsException,
            &format!(
                "arraycopy: last source index {} out of bounds for length {sl}",
                sp as i64 + n as i64
            ),
        ));
    }
    let (sp, dp, n) = (sp as usize, dp as usize, n as usize);
    macro_rules! copy {
        ($a:expr, $b:expr) => {{
            if Rc::ptr_eq(s, d) {
                $a.borrow_mut().copy_within(sp..sp + n, dp);
            } else {
                $b.borrow_mut()[dp..dp + n].clone_from_slice(&$a.borrow()[sp..sp + n]);
            }
        }};
    }
    match (&s.ext, &d.ext) {
        (Ext::Bytes(a), Ext::Bytes(b)) => copy!(a, b),
        (Ext::Chars(a), Ext::Chars(b)) => copy!(a, b),
        (Ext::Shorts(a), Ext::Shorts(b)) => copy!(a, b),
        (Ext::Ints(a), Ext::Ints(b)) => copy!(a, b),
        (Ext::Longs(a), Ext::Longs(b)) => copy!(a, b),
        (Ext::Floats(a), Ext::Floats(b)) => copy!(a, b),
        (Ext::Doubles(a), Ext::Doubles(b)) => copy!(a, b),
        (Ext::Refs(a), Ext::Refs(b)) => {
            let part: Vec<Ref> = a.borrow()[sp..sp + n].to_vec();
            if let Component::Ref(e) = d.class.component
                && let Some(Some(bad)) = part
                    .iter()
                    .find(|x| x.as_ref().is_some_and(|x| !is_assignable(x.class, e)))
            {
                return Err(throw(&C_java_lang_ArrayStoreException, bad.class.name));
            }
            b.borrow_mut()[dp..dp + n].clone_from_slice(&part);
        }
        _ => {
            return Err(throw(
                &C_java_lang_ArrayStoreException,
                "arraycopy: type mismatch",
            ));
        }
    }
    Ok(())
}

pub fn current_time_millis() -> JResult<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64))
}

thread_local! {
    static START: std::time::Instant = std::time::Instant::now();
}

pub fn nano_time() -> JResult<i64> {
    Ok(START.with(|s| s.elapsed().as_nanos() as i64))
}

pub fn identity_hash_code(o: Ref) -> JResult<i32> {
    Ok(o.as_ref().map_or(0, identity_hash))
}

pub fn get_property(_key: Ref) -> JResult<Ref> {
    Ok(None)
}

pub fn nop() -> JResult<()> {
    Ok(())
}

/// Robots may not end the program; Robocode refuses it too.
pub fn exit(_status: i32) -> JResult<()> {
    Err(throw(
        &C_java_lang_UnsupportedOperationException,
        "System.exit",
    ))
}

thread_local! {
    static OUT: Ref = stream(1);
    static ERR: Ref = stream(2);
}

fn stream(fd: u64) -> Ref {
    let s = alloc(&C_java_io_PrintStream);
    s.as_ref().unwrap().prims[0].set(fd);
    s
}

pub fn out() -> JResult<Ref> {
    Ok(OUT.with(|o| o.clone()))
}

pub fn err() -> JResult<Ref> {
    Ok(ERR.with(|o| o.clone()))
}

/// `AccessController.doPrivileged(PrivilegedAction)`: just `run()`.
pub fn do_privileged(action: Ref) -> JResult<Ref> {
    let o = nn(&action)?;
    let f: fn(Ref) -> JResult<Ref> =
        unsafe { std::mem::transmute(islot_by_name(o, "java/security/PrivilegedAction", 0)?) };
    f(action)
}
