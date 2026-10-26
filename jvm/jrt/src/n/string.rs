//! `java.lang.String`.
use crate::*;

pub fn ext() -> Ext {
    Ext::Str(OnceCell::new())
}

fn set(this: &Ref, v: Vec<u16>) -> JResult<()> {
    match &nn(this)?.ext {
        Ext::Str(s) => {
            let _ = s.set(v.into_boxed_slice());
            Ok(())
        }
        _ => panic!("not a string"),
    }
}

/// The characters of a non-null string argument.
pub fn of(r: &Ref) -> JResult<&[u16]> {
    Ok(chars(nn(r)?))
}

fn sioobe(i: i32) -> Throw {
    throw(
        &C_java_lang_StringIndexOutOfBoundsException,
        &format!("index {i}"),
    )
}

/// Java's `String.valueOf(Object)`: "null" for null, else `toString()`.
pub fn value_of_any(r: &Ref) -> JResult<Vec<u16>> {
    match r {
        None => Ok("null".encode_utf16().collect()),
        Some(o) if matches!(o.ext, Ext::Str(_)) => Ok(chars(o).to_vec()),
        Some(o) => {
            let s = call_to_string(o)?;
            Ok(match &s {
                Some(s) => chars(s).to_vec(),
                None => "null".encode_utf16().collect(),
            })
        }
    }
}

pub fn init(this: Ref) -> JResult<()> {
    set(&this, Vec::new())
}

pub fn init_str(this: Ref, s: Ref) -> JResult<()> {
    let v = value_of_any(&Some(nn(&s)?.clone()))?;
    set(&this, v)
}

pub fn init_chars(this: Ref, a: Ref) -> JResult<()> {
    let v = match &nn(&a)?.ext {
        Ext::Chars(c) => c.borrow().clone(),
        _ => panic!("not a char[]"),
    };
    set(&this, v)
}

pub fn init_chars_range(this: Ref, a: Ref, off: i32, n: i32) -> JResult<()> {
    let v = match &nn(&a)?.ext {
        Ext::Chars(c) => {
            let c = c.borrow();
            if off < 0 || n < 0 || (off as usize + n as usize) > c.len() {
                return Err(sioobe(off));
            }
            c[off as usize..(off + n) as usize].to_vec()
        }
        _ => panic!("not a char[]"),
    };
    set(&this, v)
}

pub fn equals(this: Ref, o: Ref) -> JResult<i32> {
    let a = of(&this)?;
    Ok(match &o {
        Some(o) if matches!(o.ext, Ext::Str(_)) => (a == chars(o)) as i32,
        _ => 0,
    })
}

pub fn equals_ignore_case(this: Ref, o: Ref) -> JResult<i32> {
    let a = of(&this)?;
    Ok(match &o {
        Some(o) if matches!(o.ext, Ext::Str(_)) => {
            let b = chars(o);
            (a.len() == b.len() && a.iter().zip(b).all(|(x, y)| fold(*x) == fold(*y))) as i32
        }
        _ => 0,
    })
}

fn fold(c: u16) -> u16 {
    char::from_u32(u32::from(c))
        .map(|c| c.to_lowercase().next().unwrap_or(c) as u32 as u16)
        .unwrap_or(c)
}

pub fn hash_code(this: Ref) -> JResult<i32> {
    Ok(of(&this)?
        .iter()
        .fold(0i32, |h, &c| h.wrapping_mul(31).wrapping_add(i32::from(c))))
}

pub fn to_string(this: Ref) -> JResult<Ref> {
    Ok(this)
}

pub fn length(this: Ref) -> JResult<i32> {
    Ok(of(&this)?.len() as i32)
}

pub fn is_empty(this: Ref) -> JResult<i32> {
    Ok(of(&this)?.is_empty() as i32)
}

pub fn char_at(this: Ref, i: i32) -> JResult<i32> {
    of(&this)?
        .get(i as usize)
        .map(|&c| i32::from(c))
        .ok_or_else(|| sioobe(i))
}

pub fn compare_to(this: Ref, o: Ref) -> JResult<i32> {
    let (a, b) = (of(&this)?, of(&o)?);
    for (x, y) in a.iter().zip(b) {
        if x != y {
            return Ok(i32::from(*x) - i32::from(*y));
        }
    }
    Ok(a.len() as i32 - b.len() as i32)
}

pub fn compare_to_ignore_case(this: Ref, o: Ref) -> JResult<i32> {
    let (a, b) = (of(&this)?, of(&o)?);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (fold(*x), fold(*y));
        if x != y {
            return Ok(i32::from(x) - i32::from(y));
        }
    }
    Ok(a.len() as i32 - b.len() as i32)
}

pub fn concat(this: Ref, o: Ref) -> JResult<Ref> {
    let mut v = of(&this)?.to_vec();
    v.extend_from_slice(of(&o)?);
    Ok(new_string(v))
}

pub fn substring(this: Ref, from: i32) -> JResult<Ref> {
    let n = of(&this)?.len() as i32;
    substring_range(this, from, n)
}

pub fn substring_range(this: Ref, from: i32, to: i32) -> JResult<Ref> {
    let a = of(&this)?;
    if from < 0 || to > a.len() as i32 || from > to {
        return Err(sioobe(if from < 0 { from } else { to }));
    }
    Ok(new_string(a[from as usize..to as usize].to_vec()))
}

fn find(hay: &[u16], needle: &[u16], from: usize) -> i32 {
    if needle.is_empty() {
        return from.min(hay.len()) as i32;
    }
    (from..hay.len().saturating_sub(needle.len() - 1))
        .find(|&i| hay[i..].starts_with(needle))
        .map_or(-1, |i| i as i32)
}

pub fn index_of_char(this: Ref, c: i32) -> JResult<i32> {
    index_of_char_from(this, c, 0)
}

pub fn index_of_char_from(this: Ref, c: i32, from: i32) -> JResult<i32> {
    let a = of(&this)?;
    let from = from.max(0) as usize;
    Ok(a.iter()
        .skip(from)
        .position(|&x| i32::from(x) == c)
        .map_or(-1, |p| (p + from) as i32))
}

pub fn index_of(this: Ref, s: Ref) -> JResult<i32> {
    Ok(find(of(&this)?, of(&s)?, 0))
}

pub fn index_of_from(this: Ref, s: Ref, from: i32) -> JResult<i32> {
    Ok(find(of(&this)?, of(&s)?, from.max(0) as usize))
}

pub fn last_index_of_char(this: Ref, c: i32) -> JResult<i32> {
    Ok(of(&this)?
        .iter()
        .rposition(|&x| i32::from(x) == c)
        .map_or(-1, |p| p as i32))
}

pub fn last_index_of(this: Ref, s: Ref) -> JResult<i32> {
    let (a, b) = (of(&this)?, of(&s)?);
    if b.len() > a.len() {
        return Ok(-1);
    }
    Ok((0..=a.len() - b.len())
        .rev()
        .find(|&i| a[i..].starts_with(b))
        .map_or(-1, |i| i as i32))
}

pub fn starts_with(this: Ref, s: Ref) -> JResult<i32> {
    Ok(of(&this)?.starts_with(of(&s)?) as i32)
}

pub fn ends_with(this: Ref, s: Ref) -> JResult<i32> {
    Ok(of(&this)?.ends_with(of(&s)?) as i32)
}

/// The characters of any `CharSequence` (strings and builders here).
pub fn seq_chars(r: &Ref) -> JResult<Vec<u16>> {
    let o = nn(r)?;
    match &o.ext {
        Ext::Str(_) => Ok(chars(o).to_vec()),
        Ext::Buf(b) => Ok(b.borrow().clone()),
        _ => value_of_any(r),
    }
}

pub fn contains(this: Ref, s: Ref) -> JResult<i32> {
    Ok((find(of(&this)?, &seq_chars(&s)?, 0) >= 0) as i32)
}

pub fn replace_char(this: Ref, from: i32, to: i32) -> JResult<Ref> {
    Ok(new_string(
        of(&this)?
            .iter()
            .map(|&c| if i32::from(c) == from { to as u16 } else { c })
            .collect(),
    ))
}

pub fn replace_seq(this: Ref, from: Ref, to: Ref) -> JResult<Ref> {
    let (a, f, t) = (of(&this)?, seq_chars(&from)?, seq_chars(&to)?);
    let mut out = Vec::with_capacity(a.len());
    let mut i = 0;
    while i < a.len() {
        if !f.is_empty() && a[i..].starts_with(&f) {
            out.extend_from_slice(&t);
            i += f.len();
        } else {
            out.push(a[i]);
            i += 1;
        }
    }
    Ok(new_string(out))
}

pub fn trim(this: Ref) -> JResult<Ref> {
    let a = of(&this)?;
    let start = a.iter().position(|&c| c > 32).unwrap_or(a.len());
    let end = a.iter().rposition(|&c| c > 32).map_or(start, |e| e + 1);
    Ok(new_string(a[start..end].to_vec()))
}

pub fn to_lower(this: Ref) -> JResult<Ref> {
    let s = String::from_utf16_lossy(of(&this)?).to_lowercase();
    Ok(jstr(&s))
}

pub fn to_upper(this: Ref) -> JResult<Ref> {
    let s = String::from_utf16_lossy(of(&this)?).to_uppercase();
    Ok(jstr(&s))
}

pub fn to_char_array(this: Ref) -> JResult<Ref> {
    let a = of(&this)?.to_vec();
    let r = newarray(b'C', a.len() as i32)?;
    if let Ext::Chars(c) = &nn(&r)?.ext {
        *c.borrow_mut() = a;
    }
    Ok(r)
}

pub fn get_chars(this: Ref, begin: i32, end: i32, dst: Ref, at: i32) -> JResult<()> {
    let a = of(&this)?;
    if begin < 0 || end > a.len() as i32 || begin > end {
        return Err(sioobe(begin));
    }
    for (k, &c) in a[begin as usize..end as usize].iter().enumerate() {
        ast_c(&dst, at + k as i32, c)?;
    }
    Ok(())
}

pub fn intern(this: Ref) -> JResult<Ref> {
    Ok(Some(crate::intern(of(&this)?)))
}

pub fn value_of_obj(o: Ref) -> JResult<Ref> {
    Ok(new_string(value_of_any(&o)?))
}

pub fn value_of_i(v: i32) -> JResult<Ref> {
    Ok(jstr(&v.to_string()))
}

pub fn value_of_j(v: i64) -> JResult<Ref> {
    Ok(jstr(&v.to_string()))
}

pub fn value_of_f(v: f32) -> JResult<Ref> {
    Ok(jstr(&crate::fmt::float_to_string(v)))
}

pub fn value_of_d(v: f64) -> JResult<Ref> {
    Ok(jstr(&crate::fmt::double_to_string(v)))
}

pub fn value_of_z(v: i32) -> JResult<Ref> {
    Ok(jstr(if v != 0 { "true" } else { "false" }))
}

pub fn value_of_c(v: i32) -> JResult<Ref> {
    Ok(new_string(vec![v as u16]))
}

pub fn value_of_chars(a: Ref) -> JResult<Ref> {
    match &nn(&a)?.ext {
        Ext::Chars(c) => Ok(new_string(c.borrow().clone())),
        _ => panic!("not a char[]"),
    }
}

pub fn value_of_chars_range(a: Ref, off: i32, n: i32) -> JResult<Ref> {
    let s = alloc(&C_java_lang_String);
    init_chars_range(s.clone(), a, off, n)?;
    Ok(s)
}
