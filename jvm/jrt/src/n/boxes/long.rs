//! `java.lang.Long`.
use super::*;

fn get(r: &Ref) -> JResult<i64> {
    Ok(bits(r)? as i64)
}

pub fn init(this: Ref, v: i64) -> JResult<()> {
    nn(&this)?.prims[0].set(v as u64);
    Ok(())
}

pub fn value_of(v: i64) -> JResult<Ref> {
    Ok(boxed(&C_java_lang_Long, v as u64))
}

pub fn parse(s: Ref) -> JResult<i64> {
    let t = String::from_utf16_lossy(crate::n::string::of(&s).map_err(|_| nfe("null"))?);
    crate::fmt::parse_long(&t, i64::MIN, i64::MAX).ok_or_else(|| nfe(&t))
}

pub fn to_string_static(v: i64) -> JResult<Ref> {
    Ok(jstr(&v.to_string()))
}
pub fn compare(a: i64, b: i64) -> JResult<i32> {
    Ok(lcmp(a, b))
}
pub fn bit_count(v: i64) -> JResult<i32> {
    Ok(v.count_ones() as i32)
}
pub fn signum(v: i64) -> JResult<i32> {
    Ok(v.signum() as i32)
}
pub fn trailing_zeros(v: i64) -> JResult<i32> {
    Ok(v.trailing_zeros() as i32)
}
pub fn leading_zeros(v: i64) -> JResult<i32> {
    Ok(v.leading_zeros() as i32)
}
pub fn to_hex_string(v: i64) -> JResult<Ref> {
    Ok(jstr(&format!("{:x}", v as u64)))
}
pub fn to_binary_string(v: i64) -> JResult<Ref> {
    Ok(jstr(&format!("{:b}", v as u64)))
}
pub fn max(a: i64, b: i64) -> JResult<i64> {
    Ok(a.max(b))
}
pub fn min(a: i64, b: i64) -> JResult<i64> {
    Ok(a.min(b))
}
pub fn int_value(this: Ref) -> JResult<i32> {
    Ok(get(&this)? as i32)
}
pub fn long_value(this: Ref) -> JResult<i64> {
    get(&this)
}
pub fn float_value(this: Ref) -> JResult<f32> {
    Ok(get(&this)? as f32)
}
pub fn double_value(this: Ref) -> JResult<f64> {
    Ok(get(&this)? as f64)
}
pub fn hash_code(this: Ref) -> JResult<i32> {
    let v = get(&this)?;
    Ok((v ^ ((v as u64) >> 32) as i64) as i32)
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok((same_class(&o, &C_java_lang_Long) && get(&o)? == get(&this)?) as i32)
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    compare(get(&this)?, get(&o)?)
}
