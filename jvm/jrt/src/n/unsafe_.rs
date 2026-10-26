//! The parts of `jdk.internal.misc.Unsafe` the translated library needs:
//! array layout constants, and the one method it reads memory with
//! (`ArraysSupport.vectorizedMismatch`), done on real arrays instead.
use crate::*;

/// Arrays "start" this many bytes into their object, as on HotSpot.
const BASE: i64 = 16;

pub fn get_unsafe() -> JResult<Ref> {
    Ok(alloc(&C_jdk_internal_misc_Unsafe))
}

pub fn is_big_endian(_this: Ref) -> JResult<i32> {
    Ok(0)
}

pub fn base_offset() -> JResult<i32> {
    Ok(BASE as i32)
}
pub fn scale_1() -> JResult<i32> {
    Ok(1)
}
pub fn scale_2() -> JResult<i32> {
    Ok(2)
}
pub fn scale_4() -> JResult<i32> {
    Ok(4)
}
pub fn scale_8() -> JResult<i32> {
    Ok(8)
}

/// Compares `length` elements of two arrays from the byte offsets
/// `a_off` and `b_off` (element size `1 << log2_scale`), bit for bit.
/// Returns the first mismatching element, or `!0` (none left to check).
pub fn vectorized_mismatch(
    a: Ref,
    a_off: i64,
    b: Ref,
    b_off: i64,
    length: i32,
    log2_scale: i32,
) -> JResult<i32> {
    let (ai, bi) = (
        ((a_off - BASE) >> log2_scale) as usize,
        ((b_off - BASE) >> log2_scale) as usize,
    );
    let n = length.max(0) as usize;
    macro_rules! cmp {
        ($x:expr, $y:expr, $bits:expr) => {{
            let (x, y) = ($x.borrow(), $y.borrow());
            for k in 0..n {
                if $bits(x[ai + k]) != $bits(y[bi + k]) {
                    return Ok(k as i32);
                }
            }
        }};
    }
    match (&nn(&a)?.ext, &nn(&b)?.ext) {
        (Ext::Bytes(x), Ext::Bytes(y)) => cmp!(x, y, |v: i8| v),
        (Ext::Chars(x), Ext::Chars(y)) => cmp!(x, y, |v: u16| v),
        (Ext::Shorts(x), Ext::Shorts(y)) => cmp!(x, y, |v: i16| v),
        (Ext::Ints(x), Ext::Ints(y)) => cmp!(x, y, |v: i32| v),
        (Ext::Longs(x), Ext::Longs(y)) => cmp!(x, y, |v: i64| v),
        (Ext::Floats(x), Ext::Floats(y)) => cmp!(x, y, |v: f32| v.to_bits()),
        (Ext::Doubles(x), Ext::Doubles(y)) => cmp!(x, y, |v: f64| v.to_bits()),
        _ => {
            return Err(throw(
                &C_java_lang_InternalError,
                "vectorizedMismatch on unlike arrays",
            ));
        }
    }
    Ok(!0)
}
