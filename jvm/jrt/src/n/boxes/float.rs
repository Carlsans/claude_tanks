//! `java.lang.Float`.
use super::*;

fn get(r: &Ref) -> JResult<f32> {
    Ok(f32::from_bits(bits(r)? as u32))
}

fn canonical(v: f32) -> i32 {
    if v.is_nan() {
        0x7fc0_0000
    } else {
        v.to_bits() as i32
    }
}

pub fn init(this: Ref, v: f32) -> JResult<()> {
    nn(&this)?.prims[0].set(u64::from(v.to_bits()));
    Ok(())
}

pub fn value_of(v: f32) -> JResult<Ref> {
    Ok(boxed(&C_java_lang_Float, u64::from(v.to_bits())))
}

pub fn parse(s: Ref) -> JResult<f32> {
    let t = String::from_utf16_lossy(crate::n::string::of(&s)?);
    crate::fmt::parse_double(&t)
        .map(|v| v as f32)
        .ok_or_else(|| nfe(&t))
}

pub fn compare(a: f32, b: f32) -> JResult<i32> {
    Ok(if a < b {
        -1
    } else if a > b {
        1
    } else {
        let (x, y) = (canonical(a), canonical(b));
        (x > y) as i32 - (x < y) as i32
    })
}

pub fn to_string_static(v: f32) -> JResult<Ref> {
    Ok(jstr(&crate::fmt::float_to_string(v)))
}
pub fn is_nan(v: f32) -> JResult<i32> {
    Ok(v.is_nan() as i32)
}
pub fn is_infinite(v: f32) -> JResult<i32> {
    Ok(v.is_infinite() as i32)
}
pub fn to_int_bits(v: f32) -> JResult<i32> {
    Ok(canonical(v))
}
pub fn to_raw_int_bits(v: f32) -> JResult<i32> {
    Ok(v.to_bits() as i32)
}
pub fn from_int_bits(v: i32) -> JResult<f32> {
    Ok(f32::from_bits(v as u32))
}
pub fn max(a: f32, b: f32) -> JResult<f32> {
    crate::n::math::max_f(a, b)
}
pub fn min(a: f32, b: f32) -> JResult<f32> {
    crate::n::math::min_f(a, b)
}
pub fn int_value(this: Ref) -> JResult<i32> {
    Ok(get(&this)? as i32)
}
pub fn long_value(this: Ref) -> JResult<i64> {
    Ok(get(&this)? as i64)
}
pub fn float_value(this: Ref) -> JResult<f32> {
    get(&this)
}
pub fn double_value(this: Ref) -> JResult<f64> {
    Ok(f64::from(get(&this)?))
}
pub fn hash_code(this: Ref) -> JResult<i32> {
    Ok(canonical(get(&this)?))
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(
        (same_class(&o, &C_java_lang_Float) && canonical(get(&o)?) == canonical(get(&this)?))
            as i32,
    )
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    compare(get(&this)?, get(&o)?)
}
