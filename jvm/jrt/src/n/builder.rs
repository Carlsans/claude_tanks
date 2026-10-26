//! `StringBuilder` and `StringBuffer` (one implementation; there are no
//! threads to synchronize with).
use crate::*;

pub fn ext() -> Ext {
    Ext::Buf(RefCell::new(Vec::new()))
}

fn with<T>(this: &Ref, f: impl FnOnce(&mut Vec<u16>) -> T) -> JResult<T> {
    match &nn(this)?.ext {
        Ext::Buf(b) => Ok(f(&mut b.borrow_mut())),
        _ => panic!("not a string builder"),
    }
}

fn push(this: Ref, s: &[u16]) -> JResult<Ref> {
    with(&this, |b| b.extend_from_slice(s))?;
    Ok(this)
}

fn push_str(this: Ref, s: &str) -> JResult<Ref> {
    let v: Vec<u16> = s.encode_utf16().collect();
    push(this, &v)
}

fn sioobe(i: i32) -> Throw {
    throw(
        &C_java_lang_StringIndexOutOfBoundsException,
        &format!("index {i}"),
    )
}

pub fn init(this: Ref) -> JResult<()> {
    with(&this, |_| ())
}

pub fn init_capacity(this: Ref, n: i32) -> JResult<()> {
    if n < 0 {
        return Err(throw(
            &C_java_lang_NegativeArraySizeException,
            &n.to_string(),
        ));
    }
    with(&this, |b| b.reserve(n as usize))
}

pub fn init_str(this: Ref, s: Ref) -> JResult<()> {
    let v = crate::n::string::seq_chars(&s)?;
    with(&this, |b| *b = v)
}

pub fn append_obj(this: Ref, o: Ref) -> JResult<Ref> {
    // Appending a builder to itself reads it before writing.
    let v = crate::n::string::value_of_any(&o)?;
    push(this, &v)
}

pub fn append_i(this: Ref, v: i32) -> JResult<Ref> {
    push_str(this, &v.to_string())
}

pub fn append_j(this: Ref, v: i64) -> JResult<Ref> {
    push_str(this, &v.to_string())
}

pub fn append_f(this: Ref, v: f32) -> JResult<Ref> {
    push_str(this, &crate::fmt::float_to_string(v))
}

pub fn append_d(this: Ref, v: f64) -> JResult<Ref> {
    push_str(this, &crate::fmt::double_to_string(v))
}

pub fn append_z(this: Ref, v: i32) -> JResult<Ref> {
    push_str(this, if v != 0 { "true" } else { "false" })
}

pub fn append_c(this: Ref, v: i32) -> JResult<Ref> {
    push(this, &[v as u16])
}

pub fn append_chars(this: Ref, a: Ref) -> JResult<Ref> {
    let v = match &nn(&a)?.ext {
        Ext::Chars(c) => c.borrow().clone(),
        _ => panic!("not a char[]"),
    };
    push(this, &v)
}

pub fn insert_str(this: Ref, at: i32, s: Ref) -> JResult<Ref> {
    let v = crate::n::string::value_of_any(&s)?;
    let ok = with(&this, |b| {
        if at < 0 || at as usize > b.len() {
            return false;
        }
        b.splice(at as usize..at as usize, v);
        true
    })?;
    if ok { Ok(this) } else { Err(sioobe(at)) }
}

pub fn insert_c(this: Ref, at: i32, c: i32) -> JResult<Ref> {
    let ok = with(&this, |b| {
        if at < 0 || at as usize > b.len() {
            return false;
        }
        b.insert(at as usize, c as u16);
        true
    })?;
    if ok { Ok(this) } else { Err(sioobe(at)) }
}

pub fn reverse(this: Ref) -> JResult<Ref> {
    with(&this, |b| b.reverse())?;
    Ok(this)
}

pub fn delete_char_at(this: Ref, i: i32) -> JResult<Ref> {
    let ok = with(&this, |b| {
        if i < 0 || i as usize >= b.len() {
            return false;
        }
        b.remove(i as usize);
        true
    })?;
    if ok { Ok(this) } else { Err(sioobe(i)) }
}

pub fn delete(this: Ref, from: i32, to: i32) -> JResult<Ref> {
    let ok = with(&this, |b| {
        let to = (to.max(0) as usize).min(b.len());
        if from < 0 || from as usize > to {
            return false;
        }
        b.drain(from as usize..to);
        true
    })?;
    if ok { Ok(this) } else { Err(sioobe(from)) }
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    let v = with(&this, |b| b.clone())?;
    Ok(new_string(v))
}

pub fn length(this: Ref) -> JResult<i32> {
    with(&this, |b| b.len() as i32)
}

pub fn char_at(this: Ref, i: i32) -> JResult<i32> {
    with(&this, |b| b.get(i as usize).map(|&c| i32::from(c)))?.ok_or_else(|| sioobe(i))
}

pub fn substring_range(this: Ref, from: i32, to: i32) -> JResult<Ref> {
    let v = with(&this, |b| {
        (from >= 0 && to as usize <= b.len() && from <= to)
            .then(|| b[from as usize..to as usize].to_vec())
    })?;
    v.map(new_string).ok_or_else(|| sioobe(from))
}

pub fn set_length(this: Ref, n: i32) -> JResult<()> {
    if n < 0 {
        return Err(sioobe(n));
    }
    with(&this, |b| b.resize(n as usize, 0))
}

pub fn set_char_at(this: Ref, i: i32, c: i32) -> JResult<()> {
    let ok = with(&this, |b| match b.get_mut(i as usize) {
        Some(x) => {
            *x = c as u16;
            true
        }
        None => false,
    })?;
    if ok { Ok(()) } else { Err(sioobe(i)) }
}

pub fn append_seq_range(this: Ref, s: Ref, start: i32, end: i32) -> JResult<Ref> {
    let v = crate::n::string::seq_chars(&s)?;
    if start < 0 || end as usize > v.len() || start > end {
        return Err(sioobe(start));
    }
    push(this, &v[start as usize..end as usize])
}

pub fn append_chars_range(this: Ref, a: Ref, off: i32, n: i32) -> JResult<Ref> {
    let v = match &nn(&a)?.ext {
        Ext::Chars(c) => {
            let c = c.borrow();
            if off < 0 || n < 0 || (off + n) as usize > c.len() {
                return Err(sioobe(off));
            }
            c[off as usize..(off + n) as usize].to_vec()
        }
        _ => panic!("not a char[]"),
    };
    push(this, &v)
}

pub fn index_of(this: Ref, s: Ref) -> JResult<i32> {
    let me = with(&this, |b| b.clone())?;
    let needle = crate::n::string::seq_chars(&s)?;
    if needle.is_empty() {
        return Ok(0);
    }
    Ok(me
        .windows(needle.len())
        .position(|w| w == needle.as_slice())
        .map_or(-1, |i| i as i32))
}

pub fn append_code_point(this: Ref, c: i32) -> JResult<Ref> {
    match char::from_u32(c as u32) {
        Some(ch) => {
            let mut buf = [0u16; 2];
            let v = ch.encode_utf16(&mut buf).to_vec();
            push(this, &v)
        }
        None => Err(throw(
            &C_java_lang_IllegalArgumentException,
            &format!("invalid code point {c}"),
        )),
    }
}
