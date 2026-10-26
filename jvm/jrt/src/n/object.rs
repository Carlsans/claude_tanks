//! `java.lang.Object`.
use crate::*;

pub fn init(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(same(&this, &o) as i32)
}

pub fn hash_code(this: Ref) -> JResult<i32> {
    Ok(identity_hash(nn(&this)?))
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    let o = nn(&this)?;
    let h = call_hash_code(o)?;
    Ok(jstr(&format!("{}@{:x}", o.class.name.replace('/', "."), h)))
}

pub fn clone(this: Ref) -> JResult<Ref> {
    let o = nn(&this)?;
    if !matches!(o.class.component, Component::None)
        || is_assignable(o.class, &C_java_lang_Cloneable)
    {
        Ok(Some(shallow_clone(o)))
    } else {
        Err(throw(
            &C_java_lang_CloneNotSupportedException,
            &o.class.name.replace('/', "."),
        ))
    }
}

pub fn finalize(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn get_class(this: Ref) -> JResult<Ref> {
    Ok(class_object(nn(&this)?.class))
}

pub fn nop(_this: Ref) -> JResult<()> {
    Ok(())
}

fn monitor(this: &Ref) -> JResult<usize> {
    Ok(Rc::as_ptr(nn(this)?) as usize)
}

pub fn wait(this: Ref) -> JResult<()> {
    wait_j(this, 0)
}

pub fn wait_j(this: Ref, ms: i64) -> JResult<()> {
    if ms < 0 {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            "timeout value is negative",
        ));
    }
    let deadline =
        (ms > 0).then(|| std::time::Instant::now() + std::time::Duration::from_millis(ms as u64));
    crate::thread::block(crate::thread::Block::Wait {
        monitor: monitor(&this)?,
        deadline,
    })
}

pub fn wait_j_nanos(this: Ref, ms: i64, _nanos: i32) -> JResult<()> {
    wait_j(this, ms)
}

pub fn notify(this: Ref) -> JResult<()> {
    crate::thread::notify(monitor(&this)?, false);
    Ok(())
}

pub fn notify_all(this: Ref) -> JResult<()> {
    crate::thread::notify(monitor(&this)?, true);
    Ok(())
}
