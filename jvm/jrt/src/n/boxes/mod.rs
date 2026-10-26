//! `java.lang.Number` and the boxes of the primitive types. A box keeps its
//! value in `prims[0]`, as bits.
use crate::*;

pub mod boolean;
pub mod character;
pub mod double;
pub mod float;
pub mod integer;
pub mod long;
pub mod small;

/// Number's method-table slots after Object's five.
const INT_VALUE: usize = 5;

pub fn number_init(_this: Ref) -> JResult<()> {
    Ok(())
}

pub fn number_byte_value(this: Ref) -> JResult<i32> {
    let f: fn(Ref) -> JResult<i32> = unsafe { std::mem::transmute(vslot(nn(&this)?, INT_VALUE)?) };
    Ok(f(this)? as i8 as i32)
}

pub fn number_short_value(this: Ref) -> JResult<i32> {
    let f: fn(Ref) -> JResult<i32> = unsafe { std::mem::transmute(vslot(nn(&this)?, INT_VALUE)?) };
    Ok(f(this)? as i16 as i32)
}

pub(crate) fn bits(r: &Ref) -> JResult<u64> {
    Ok(nn(r)?.prims[0].get())
}

pub(crate) fn boxed(c: &'static Class, bits: u64) -> Ref {
    let o = alloc(c);
    o.as_ref().unwrap().prims[0].set(bits);
    o
}

/// Whether `o` is a box of class `c` (for `equals`).
pub(crate) fn same_class(o: &Ref, c: &'static Class) -> bool {
    o.as_ref().is_some_and(|o| std::ptr::eq(o.class, c))
}

pub(crate) fn nfe(s: &str) -> Throw {
    throw(
        &C_java_lang_NumberFormatException,
        &format!("For input string: \"{s}\""),
    )
}
