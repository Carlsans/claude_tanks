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

pub fn format(fmt: Ref, args: Ref) -> JResult<Ref> {
    Ok(jstr(&crate::n::format::format_str(&fmt, &args)?))
}

/// `java.util.regex.Pattern`, when the program has it (translated from the
/// JDK); string methods taking a regular expression go through it.
fn pattern() -> Option<&'static Class> {
    crate::reflect::class_named("java/util/regex/Pattern")
}

fn compile(regex: &Ref) -> JResult<Ref> {
    let p = pattern().ok_or_else(|| unsupported("java/util/regex/Pattern"))?;
    Ok(crate::reflect::call(
        p,
        "compile(Ljava/lang/String;)Ljava/util/regex/Pattern;",
        &[crate::reflect::Val::A(regex.clone())],
    )?
    .a())
}

/// Java's own fast path: a separator that is one character other than a
/// regex metacharacter, or a backslash and a non-alphanumeric character.
fn literal_separator(p: &[u16]) -> Option<u16> {
    match p {
        [c] if !".$|()[{^?*+\\".contains(char::from_u32(u32::from(*c)).unwrap_or('a')) => Some(*c),
        [b, c]
            if *b == u16::from(b'\\')
                && !char::from_u32(u32::from(*c)).is_some_and(|x| x.is_ascii_alphanumeric()) =>
        {
            Some(*c)
        }
        _ => None,
    }
}

fn split_literal(a: &[u16], sep: u16, limit: i32) -> Vec<Vec<u16>> {
    let mut parts: Vec<Vec<u16>> = Vec::new();
    let mut start = 0;
    for (i, &c) in a.iter().enumerate() {
        if c == sep && !(limit > 0 && parts.len() as i32 == limit - 1) {
            parts.push(a[start..i].to_vec());
            start = i + 1;
        }
    }
    parts.push(a[start..].to_vec());
    if limit == 0 {
        while parts.len() > 1 && parts.last().is_some_and(|p| p.is_empty()) {
            parts.pop();
        }
        if parts.len() == 1 && parts[0].is_empty() && !a.is_empty() {
            parts.clear();
        }
    }
    parts
}

pub fn split_limit(this: Ref, regex: Ref, limit: i32) -> JResult<Ref> {
    let r = of(&regex)?.to_vec();
    if let Some(sep) = literal_separator(&r) {
        let parts = split_literal(of(&this)?, sep, limit);
        let arr = anewarray(&C_java_lang_String, parts.len() as i32)?;
        for (i, p) in parts.into_iter().enumerate() {
            ast_a(&arr, i as i32, new_string(p))?;
        }
        return Ok(arr);
    }
    let p = compile(&regex)?;
    Ok(crate::reflect::call_virtual(
        &p,
        "split(Ljava/lang/CharSequence;I)[Ljava/lang/String;",
        &[crate::reflect::Val::A(this), crate::reflect::Val::I(limit)],
    )?
    .a())
}

pub fn split(this: Ref, regex: Ref) -> JResult<Ref> {
    split_limit(this, regex, 0)
}

pub fn matches(this: Ref, regex: Ref) -> JResult<i32> {
    match pattern() {
        Some(p) => Ok(crate::reflect::call(
            p,
            "matches(Ljava/lang/String;Ljava/lang/CharSequence;)Z",
            &[crate::reflect::Val::A(regex), crate::reflect::Val::A(this)],
        )?
        .i()),
        None => Ok((of(&this)? == of(&regex)?) as i32),
    }
}

fn replace_regex(this: Ref, regex: Ref, rep: Ref, sig: &str) -> JResult<Ref> {
    let p = compile(&regex)?;
    let m = crate::reflect::call_virtual(
        &p,
        "matcher(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;",
        &[crate::reflect::Val::A(this)],
    )?
    .a();
    Ok(crate::reflect::call_virtual(&m, sig, &[crate::reflect::Val::A(rep)])?.a())
}

pub fn replace_all_regex(this: Ref, regex: Ref, rep: Ref) -> JResult<Ref> {
    replace_regex(
        this,
        regex,
        rep,
        "replaceAll(Ljava/lang/String;)Ljava/lang/String;",
    )
}

pub fn replace_first_regex(this: Ref, regex: Ref, rep: Ref) -> JResult<Ref> {
    replace_regex(
        this,
        regex,
        rep,
        "replaceFirst(Ljava/lang/String;)Ljava/lang/String;",
    )
}

pub fn is_blank(this: Ref) -> JResult<i32> {
    Ok(of(&this)?
        .iter()
        .all(|&c| c <= 32 || char::from_u32(u32::from(c)).is_some_and(char::is_whitespace))
        as i32)
}

pub fn repeat(this: Ref, n: i32) -> JResult<Ref> {
    if n < 0 {
        return Err(throw(
            &C_java_lang_IllegalArgumentException,
            &format!("count is negative: {n}"),
        ));
    }
    Ok(new_string(of(&this)?.repeat(n as usize)))
}

pub fn replace_first(this: Ref, from: Ref, to: Ref) -> JResult<Ref> {
    let (a, f, t) = (of(&this)?, seq_chars(&from)?, seq_chars(&to)?);
    let i = find(a, &f, 0);
    if i < 0 || f.is_empty() {
        return Ok(new_string(a.to_vec()));
    }
    let i = i as usize;
    let mut out = a[..i].to_vec();
    out.extend_from_slice(&t);
    out.extend_from_slice(&a[i + f.len()..]);
    Ok(new_string(out))
}

fn bytes_array(b: &[u8]) -> JResult<Ref> {
    let r = newarray(b'B', b.len() as i32)?;
    if let Ext::Bytes(v) = &nn(&r)?.ext {
        *v.borrow_mut() = b.iter().map(|&x| x as i8).collect();
    }
    Ok(r)
}

/// Strings encode as UTF-8 whatever the charset asked for.
pub fn get_bytes(this: Ref) -> JResult<Ref> {
    bytes_array(String::from_utf16_lossy(of(&this)?).as_bytes())
}

pub fn get_bytes_charset(this: Ref, _cs: Ref) -> JResult<Ref> {
    get_bytes(this)
}

pub fn to_lower_locale(this: Ref, _l: Ref) -> JResult<Ref> {
    to_lower(this)
}

pub fn to_upper_locale(this: Ref, _l: Ref) -> JResult<Ref> {
    to_upper(this)
}

fn from_bytes(a: &Ref, off: i32, n: Option<i32>) -> JResult<Vec<u16>> {
    match &nn(a)?.ext {
        Ext::Bytes(b) => {
            let b = b.borrow();
            let off = off.max(0) as usize;
            let end = n.map_or(b.len(), |n| (off + n.max(0) as usize).min(b.len()));
            let raw: Vec<u8> = b[off.min(end)..end].iter().map(|&x| x as u8).collect();
            Ok(String::from_utf8_lossy(&raw).encode_utf16().collect())
        }
        _ => panic!("not a byte[]"),
    }
}

pub fn init_bytes(this: Ref, a: Ref) -> JResult<()> {
    let v = from_bytes(&a, 0, None)?;
    set(&this, v)
}

pub fn init_bytes_charset(this: Ref, a: Ref, _cs: Ref) -> JResult<()> {
    init_bytes(this, a)
}

pub fn init_bytes_range(this: Ref, a: Ref, off: i32, n: i32) -> JResult<()> {
    let v = from_bytes(&a, off, Some(n))?;
    set(&this, v)
}

pub fn init_bytes_range_charset(this: Ref, a: Ref, off: i32, n: i32, _cs: Ref) -> JResult<()> {
    init_bytes_range(this, a, off, n)
}

fn charset(name: &str) -> Ref {
    let c = alloc(&C_java_nio_charset_Charset);
    *c.as_ref().unwrap().refs[0].borrow_mut() = jstr(name);
    c
}

pub fn charset_name(this: Ref) -> JResult<Ref> {
    Ok(nn(&this)?.refs[0].borrow().clone())
}

pub fn charset_for_name(name: Ref) -> JResult<Ref> {
    Ok(charset(&rust_string(nn(&name)?)))
}

pub fn charset_utf8() -> JResult<Ref> {
    Ok(charset("UTF-8"))
}

pub fn charset_ascii() -> JResult<Ref> {
    Ok(charset("US-ASCII"))
}

pub fn charset_latin1() -> JResult<Ref> {
    Ok(charset("ISO-8859-1"))
}

pub fn init_code_points(this: Ref, a: Ref, off: i32, n: i32) -> JResult<()> {
    let mut out = Vec::new();
    for i in off..off + n {
        let c = ald_i(&a, i)?;
        match char::from_u32(c as u32) {
            Some(ch) => {
                let mut buf = [0u16; 2];
                out.extend_from_slice(ch.encode_utf16(&mut buf));
            }
            None => {
                return Err(throw(
                    &C_java_lang_IllegalArgumentException,
                    &format!("invalid code point {c}"),
                ));
            }
        }
    }
    set(&this, out)
}

pub fn code_point_at(this: Ref, i: i32) -> JResult<i32> {
    crate::n::boxes::character::code_point_at(this, i)
}

pub fn code_point_count(this: Ref, begin: i32, end: i32) -> JResult<i32> {
    let a = of(&this)?;
    if begin < 0 || end as usize > a.len() || begin > end {
        return Err(sioobe(begin));
    }
    Ok(String::from_utf16_lossy(&a[begin as usize..end as usize])
        .chars()
        .count() as i32)
}
