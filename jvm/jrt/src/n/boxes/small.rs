//! `java.lang.Byte` and `java.lang.Short`.
use super::*;

fn get(r: &Ref) -> JResult<i32> {
    Ok(bits(r)? as i32)
}

pub fn init(this: Ref, v: i32) -> JResult<()> {
    nn(&this)?.prims[0].set(v as i64 as u64);
    Ok(())
}

pub fn byte_value_of(v: i32) -> JResult<Ref> {
    Ok(boxed(&C_java_lang_Byte, v as i8 as i64 as u64))
}

pub fn short_value_of(v: i32) -> JResult<Ref> {
    Ok(boxed(&C_java_lang_Short, v as i16 as i64 as u64))
}

fn parse(s: &Ref, min: i64, max: i64) -> JResult<i32> {
    let t = String::from_utf16_lossy(crate::n::string::of(s).map_err(|_| nfe("null"))?);
    crate::fmt::parse_long(&t, min, max)
        .map(|v| v as i32)
        .ok_or_else(|| nfe(&t))
}

pub fn parse_byte(s: Ref) -> JResult<i32> {
    parse(&s, -128, 127)
}

pub fn parse_short(s: Ref) -> JResult<i32> {
    parse(&s, -32768, 32767)
}

pub fn to_string_static(v: i32) -> JResult<Ref> {
    Ok(jstr(&v.to_string()))
}
pub fn int_value(this: Ref) -> JResult<i32> {
    get(&this)
}
pub fn long_value(this: Ref) -> JResult<i64> {
    Ok(i64::from(get(&this)?))
}
pub fn float_value(this: Ref) -> JResult<f32> {
    Ok(get(&this)? as f32)
}
pub fn double_value(this: Ref) -> JResult<f64> {
    Ok(f64::from(get(&this)?))
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    let c = nn(&this)?.class;
    Ok((same_class(&o, c) && get(&o)? == get(&this)?) as i32)
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    Ok(get(&this)? - get(&o)?)
}
