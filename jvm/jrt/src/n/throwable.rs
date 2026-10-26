//! `java.lang.Throwable` and the exceptions the runtime throws. The message
//! is in `refs[0]`, the cause in `refs[1]`.
use crate::*;

pub fn init(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn init_msg(this: Ref, msg: Ref) -> JResult<()> {
    *nn(&this)?.refs[0].borrow_mut() = msg;
    Ok(())
}

pub fn init_msg_cause(this: Ref, msg: Ref, cause: Ref) -> JResult<()> {
    let o = nn(&this)?;
    *o.refs[0].borrow_mut() = msg;
    *o.refs[1].borrow_mut() = cause;
    Ok(())
}

/// `Throwable(Throwable cause)`: the message is the cause's `toString()`.
pub fn init_cause(this: Ref, cause: Ref) -> JResult<()> {
    let msg = match &cause {
        Some(c) => call_to_string(c)?,
        None => None,
    };
    init_msg_cause(this, msg, cause)
}

pub fn get_message(this: Ref) -> JResult<Ref> {
    Ok(nn(&this)?.refs[0].borrow().clone())
}

pub fn get_cause(this: Ref) -> JResult<Ref> {
    Ok(nn(&this)?.refs[1].borrow().clone())
}

pub fn set_cause(this: Ref, cause: Ref) -> JResult<Ref> {
    *nn(&this)?.refs[1].borrow_mut() = cause;
    Ok(this)
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    Ok(jstr(&describe(&Throw(nn(&this)?.clone()))))
}

pub fn print_stack_trace(this: Ref) -> JResult<()> {
    if printing() {
        eprintln!("{}", describe(&Throw(nn(&this)?.clone())));
    }
    Ok(())
}

pub fn fill_in_stack_trace(this: Ref) -> JResult<Ref> {
    Ok(this)
}

fn init_text(this: Ref, text: String) -> JResult<()> {
    init_msg(this, jstr(&text))
}

pub fn init_ioobe_index(this: Ref, i: i32) -> JResult<()> {
    init_text(this, format!("Index out of range: {i}"))
}

pub fn init_ioobe_index_j(this: Ref, i: i64) -> JResult<()> {
    init_text(this, format!("Index out of range: {i}"))
}

pub fn init_aioobe_index(this: Ref, i: i32) -> JResult<()> {
    init_text(this, format!("Array index out of range: {i}"))
}

pub fn init_sioobe_index(this: Ref, i: i32) -> JResult<()> {
    init_text(this, format!("String index out of range: {i}"))
}

pub fn init_msg_cause_flags(
    this: Ref,
    msg: Ref,
    cause: Ref,
    _suppression: i32,
    _writable: i32,
) -> JResult<()> {
    init_msg_cause(this, msg, cause)
}

/// `AssertionError(Object)`: the object's text is the message.
pub fn init_object(this: Ref, o: Ref) -> JResult<()> {
    let msg = new_string(crate::n::string::value_of_any(&o)?);
    let cause = match &o {
        Some(x) if is_assignable(x.class, &C_java_lang_Throwable) => o.clone(),
        _ => None,
    };
    init_msg_cause(this, msg, cause)
}

pub fn add_suppressed(_this: Ref, _t: Ref) -> JResult<()> {
    Ok(())
}

pub fn no_throwables(_this: Ref) -> JResult<Ref> {
    anewarray(&C_java_lang_Throwable, 0)
}

/// No stack traces: an empty array.
pub fn no_stack(_this: Ref) -> JResult<Ref> {
    anewarray(&C_java_lang_Object, 0)
}
