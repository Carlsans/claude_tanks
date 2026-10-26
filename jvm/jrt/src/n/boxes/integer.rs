//! `java.lang.Integer`.
use super::*;

thread_local! {
    static CACHE: Vec<Rc<Obj>> = (-128..=127).map(|v: i32| boxed(&C_java_lang_Integer, v as i64 as u64).unwrap()).collect();
}

fn get(r: &Ref) -> JResult<i32> {
    Ok(bits(r)? as i32)
}

pub fn init(this: Ref, v: i32) -> JResult<()> {
    nn(&this)?.prims[0].set(v as i64 as u64);
    Ok(())
}

pub fn value_of(v: i32) -> JResult<Ref> {
    if (-128..=127).contains(&v) {
        return Ok(Some(CACHE.with(|c| c[(v + 128) as usize].clone())));
    }
    Ok(boxed(&C_java_lang_Integer, v as i64 as u64))
}

pub fn parse(s: Ref) -> JResult<i32> {
    let t = String::from_utf16_lossy(crate::n::string::of(&s).map_err(|_| nfe("null"))?);
    crate::fmt::parse_long(&t, i64::from(i32::MIN), i64::from(i32::MAX))
        .map(|v| v as i32)
        .ok_or_else(|| nfe(&t))
}

pub fn value_of_str(s: Ref) -> JResult<Ref> {
    value_of(parse(s)?)
}

pub fn to_string_static(v: i32) -> JResult<Ref> {
    Ok(jstr(&v.to_string()))
}
pub fn to_hex_string(v: i32) -> JResult<Ref> {
    Ok(jstr(&format!("{:x}", v as u32)))
}
pub fn to_binary_string(v: i32) -> JResult<Ref> {
    Ok(jstr(&format!("{:b}", v as u32)))
}
pub fn bit_count(v: i32) -> JResult<i32> {
    Ok(v.count_ones() as i32)
}
pub fn signum(v: i32) -> JResult<i32> {
    Ok(v.signum())
}
pub fn compare(a: i32, b: i32) -> JResult<i32> {
    Ok((a > b) as i32 - (a < b) as i32)
}
pub fn sum(a: i32, b: i32) -> JResult<i32> {
    Ok(a.wrapping_add(b))
}

pub fn max(a: i32, b: i32) -> JResult<i32> {
    Ok(a.max(b))
}
pub fn min(a: i32, b: i32) -> JResult<i32> {
    Ok(a.min(b))
}
pub fn abs(a: i32) -> JResult<i32> {
    Ok(a.wrapping_abs())
}
pub fn trailing_zeros(v: i32) -> JResult<i32> {
    Ok(v.trailing_zeros() as i32)
}
pub fn leading_zeros(v: i32) -> JResult<i32> {
    Ok(v.leading_zeros() as i32)
}
pub fn highest_one_bit(v: i32) -> JResult<i32> {
    Ok(if v == 0 {
        0
    } else {
        (1u32 << (31 - v.leading_zeros())) as i32
    })
}
pub fn reverse(v: i32) -> JResult<i32> {
    Ok(v.reverse_bits())
}
pub fn rotate_left(v: i32, n: i32) -> JResult<i32> {
    Ok(v.rotate_left(n as u32 & 31))
}
pub fn rotate_right(v: i32, n: i32) -> JResult<i32> {
    Ok(v.rotate_right(n as u32 & 31))
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
pub fn hash_code(this: Ref) -> JResult<i32> {
    get(&this)
}
pub fn to_string(this: Ref) -> JResult<Ref> {
    to_string_static(get(&this)?)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    Ok((same_class(&o, &C_java_lang_Integer) && get(&o)? == get(&this)?) as i32)
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    compare(get(&this)?, get(&o)?)
}
