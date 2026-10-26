//! `java.io.PrintStream` for `System.out` and `System.err` (see
//! `set_printing`).
use crate::*;
use std::io::Write;

fn emit(this: &Ref, s: &str, newline: bool) -> JResult<()> {
    let fd = nn(this)?.prims[0].get();
    if printing() {
        if fd == 2 {
            let mut e = std::io::stderr().lock();
            let _ = e.write_all(s.as_bytes());
            if newline {
                let _ = e.write_all(b"\n");
            }
        } else {
            let mut o = std::io::stdout().lock();
            let _ = o.write_all(s.as_bytes());
            if newline {
                let _ = o.write_all(b"\n");
            }
        }
    }
    Ok(())
}

fn obj(o: &Ref) -> JResult<String> {
    Ok(String::from_utf16_lossy(&crate::n::string::value_of_any(
        o,
    )?))
}

pub fn println(this: Ref) -> JResult<()> {
    emit(&this, "", true)
}
pub fn println_obj(this: Ref, o: Ref) -> JResult<()> {
    let s = obj(&o)?;
    emit(&this, &s, true)
}
pub fn println_i(this: Ref, v: i32) -> JResult<()> {
    emit(&this, &v.to_string(), true)
}
pub fn println_j(this: Ref, v: i64) -> JResult<()> {
    emit(&this, &v.to_string(), true)
}
pub fn println_f(this: Ref, v: f32) -> JResult<()> {
    emit(&this, &crate::fmt::float_to_string(v), true)
}
pub fn println_d(this: Ref, v: f64) -> JResult<()> {
    emit(&this, &crate::fmt::double_to_string(v), true)
}
pub fn println_z(this: Ref, v: i32) -> JResult<()> {
    emit(&this, if v != 0 { "true" } else { "false" }, true)
}
pub fn println_c(this: Ref, v: i32) -> JResult<()> {
    emit(&this, &String::from_utf16_lossy(&[v as u16]), true)
}
pub fn print_obj(this: Ref, o: Ref) -> JResult<()> {
    let s = obj(&o)?;
    emit(&this, &s, false)
}
pub fn print_i(this: Ref, v: i32) -> JResult<()> {
    emit(&this, &v.to_string(), false)
}
pub fn print_j(this: Ref, v: i64) -> JResult<()> {
    emit(&this, &v.to_string(), false)
}
pub fn print_f(this: Ref, v: f32) -> JResult<()> {
    emit(&this, &crate::fmt::float_to_string(v), false)
}
pub fn print_d(this: Ref, v: f64) -> JResult<()> {
    emit(&this, &crate::fmt::double_to_string(v), false)
}
pub fn print_z(this: Ref, v: i32) -> JResult<()> {
    emit(&this, if v != 0 { "true" } else { "false" }, false)
}
pub fn print_c(this: Ref, v: i32) -> JResult<()> {
    emit(&this, &String::from_utf16_lossy(&[v as u16]), false)
}

pub fn println_chars(this: Ref, a: Ref) -> JResult<()> {
    let s = match &nn(&a)?.ext {
        Ext::Chars(c) => String::from_utf16_lossy(&c.borrow()),
        _ => panic!("not a char[]"),
    };
    emit(&this, &s, true)
}

pub fn flush(_this: Ref) -> JResult<()> {
    let _ = std::io::stdout().flush();
    Ok(())
}
