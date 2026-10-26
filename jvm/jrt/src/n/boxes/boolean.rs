//! `java.lang.Boolean`.
use super::*;

thread_local! {
    static TRUE: Ref = boxed(&C_java_lang_Boolean, 1);
    static FALSE: Ref = boxed(&C_java_lang_Boolean, 0);
}

fn get(r: &Ref) -> JResult<i32> {
    Ok((bits(r)? != 0) as i32)
}

pub fn init(this: Ref, v: i32) -> JResult<()> {
    nn(&this)?.prims[0].set((v != 0) as u64);
    Ok(())
}

pub fn value_of(v: i32) -> JResult<Ref> {
    if v != 0 { get_true() } else { get_false() }
}

pub fn get_true() -> JResult<Ref> {
    Ok(TRUE.with(|t| t.clone()))
}

pub fn get_false() -> JResult<Ref> {
    Ok(FALSE.with(|t| t.clone()))
}

pub fn parse(s: Ref) -> JResult<i32> {
    Ok(match &s {
        Some(o) => String::from_utf16_lossy(chars(o)).eq_ignore_ascii_case("true") as i32,
        None => 0,
    })
}

pub fn to_string_static(v: i32) -> JResult<Ref> {
    Ok(jstr(if v != 0 { "true" } else { "false" }))
}
pub fn boolean_value(this: Ref) -> JResult<i32> {
    get(&this)
}
pub fn hash_code(this: Ref) -> JResult<i32> {
    Ok(if get(&this)? != 0 { 1231 } else { 1237 })
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok((same_class(&o, &C_java_lang_Boolean) && get(&o)? == get(&this)?) as i32)
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    Ok(get(&this)? - get(&o)?)
}
