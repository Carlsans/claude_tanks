//! `java.lang.Double`.
use super::*;

fn get(r: &Ref) -> JResult<f64> {
    Ok(f64::from_bits(bits(r)?))
}

/// `doubleToLongBits`: every NaN as the canonical one.
fn canonical(v: f64) -> i64 {
    if v.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        v.to_bits() as i64
    }
}

pub fn init(this: Ref, v: f64) -> JResult<()> {
    nn(&this)?.prims[0].set(v.to_bits());
    Ok(())
}

pub fn value_of(v: f64) -> JResult<Ref> {
    Ok(boxed(&C_java_lang_Double, v.to_bits()))
}

pub fn parse(s: Ref) -> JResult<f64> {
    let t = String::from_utf16_lossy(crate::n::string::of(&s)?);
    crate::fmt::parse_double(&t).ok_or_else(|| nfe(&t))
}

pub fn value_of_str(s: Ref) -> JResult<Ref> {
    value_of(parse(s)?)
}

/// Java's total order: -0.0 below 0.0, NaN above everything.
pub fn compare(a: f64, b: f64) -> JResult<i32> {
    Ok(if a < b {
        -1
    } else if a > b {
        1
    } else {
        let (x, y) = (canonical(a), canonical(b));
        (x > y) as i32 - (x < y) as i32
    })
}

pub fn to_string_static(v: f64) -> JResult<Ref> {
    Ok(jstr(&crate::fmt::double_to_string(v)))
}
pub fn is_nan(v: f64) -> JResult<i32> {
    Ok(v.is_nan() as i32)
}
pub fn is_infinite(v: f64) -> JResult<i32> {
    Ok(v.is_infinite() as i32)
}
pub fn is_finite(v: f64) -> JResult<i32> {
    Ok(v.is_finite() as i32)
}
pub fn to_long_bits(v: f64) -> JResult<i64> {
    Ok(canonical(v))
}
pub fn to_raw_long_bits(v: f64) -> JResult<i64> {
    Ok(v.to_bits() as i64)
}
pub fn from_long_bits(v: i64) -> JResult<f64> {
    Ok(f64::from_bits(v as u64))
}
pub fn max(a: f64, b: f64) -> JResult<f64> {
    crate::n::math::max_d(a, b)
}
pub fn min(a: f64, b: f64) -> JResult<f64> {
    crate::n::math::min_d(a, b)
}
pub fn sum(a: f64, b: f64) -> JResult<f64> {
    Ok(a + b)
}
pub fn hash_code_static(v: f64) -> JResult<i32> {
    let b = canonical(v);
    Ok((b ^ ((b as u64) >> 32) as i64) as i32)
}
pub fn is_nan_inst(this: Ref) -> JResult<i32> {
    Ok(get(&this)?.is_nan() as i32)
}
pub fn int_value(this: Ref) -> JResult<i32> {
    Ok(get(&this)? as i32)
}
pub fn long_value(this: Ref) -> JResult<i64> {
    Ok(get(&this)? as i64)
}
pub fn float_value(this: Ref) -> JResult<f32> {
    Ok(get(&this)? as f32)
}
pub fn double_value(this: Ref) -> JResult<f64> {
    get(&this)
}
pub fn hash_code(this: Ref) -> JResult<i32> {
    hash_code_static(get(&this)?)
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok(
        (same_class(&o, &C_java_lang_Double) && canonical(get(&o)?) == canonical(get(&this)?))
            as i32,
    )
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    compare(get(&this)?, get(&o)?)
}
