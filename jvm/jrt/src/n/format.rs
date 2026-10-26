//! `String.format` and `PrintStream.printf`: the common part of Java's
//! `Formatter` (conversions `b c d o x X e E f g s S n %`, flags `-0+ ,(#`,
//! width, precision, explicit argument indices). Robots mostly format for
//! printing, so rounding follows Rust's (half to even on the exact binary
//! value) where Java rounds half up.
use crate::*;

enum Arg {
    Null,
    Int(i64, u32),
    Float(f64),
    Char(u16),
    Bool(bool),
    Other,
}

fn arg(r: &Ref) -> Arg {
    let Some(o) = r else {
        return Arg::Null;
    };
    let bits = || o.prims.first().map_or(0, |c| c.get());
    match o.class.name {
        "java/lang/Integer" => Arg::Int(bits() as i32 as i64, 32),
        "java/lang/Short" => Arg::Int(bits() as i16 as i64, 16),
        "java/lang/Byte" => Arg::Int(bits() as i8 as i64, 8),
        "java/lang/Long" => Arg::Int(bits() as i64, 64),
        "java/lang/Double" => Arg::Float(f64::from_bits(bits())),
        "java/lang/Float" => Arg::Float(f64::from(f32::from_bits(bits() as u32))),
        "java/lang/Character" => Arg::Char(bits() as u16),
        "java/lang/Boolean" => Arg::Bool(bits() != 0),
        _ => Arg::Other,
    }
}

fn bad(msg: &str) -> Throw {
    throw(&C_java_lang_IllegalArgumentException, msg)
}

fn text(r: &Ref) -> JResult<String> {
    Ok(String::from_utf16_lossy(&crate::n::string::value_of_any(
        r,
    )?))
}

fn group(digits: &str) -> String {
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Java's `%e` exponent: a sign and at least two digits.
fn java_exp(s: String) -> String {
    match s.split_once('e') {
        Some((m, e)) => {
            let (sign, digits) = match e.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', e),
            };
            format!("{m}e{sign}{digits:0>2}")
        }
        None => s,
    }
}

pub fn format_str(fmt: &Ref, args: &Ref) -> JResult<String> {
    let f: Vec<char> = String::from_utf16_lossy(crate::n::string::of(fmt)?)
        .chars()
        .collect();
    let argv: Vec<Ref> = match args {
        Some(a) => match &a.ext {
            Ext::Refs(v) => v.borrow().clone(),
            _ => Vec::new(),
        },
        None => vec![None],
    };
    let mut out = String::new();
    let mut next = 0usize;
    let mut i = 0;
    while i < f.len() {
        if f[i] != '%' {
            out.push(f[i]);
            i += 1;
            continue;
        }
        i += 1;
        // [index$][flags][width][.precision]conversion
        let start = i;
        while i < f.len() && f[i].is_ascii_digit() {
            i += 1;
        }
        let mut index = None;
        if i < f.len() && f[i] == '$' {
            index = f[start..i].iter().collect::<String>().parse::<usize>().ok();
            i += 1;
        } else {
            i = start;
        }
        let mut flags = String::new();
        while i < f.len() && "-#+ 0,(".contains(f[i]) {
            flags.push(f[i]);
            i += 1;
        }
        let ws = i;
        while i < f.len() && f[i].is_ascii_digit() {
            i += 1;
        }
        let width: usize = f[ws..i].iter().collect::<String>().parse().unwrap_or(0);
        let mut precision = None;
        if i < f.len() && f[i] == '.' {
            i += 1;
            let ps = i;
            while i < f.len() && f[i].is_ascii_digit() {
                i += 1;
            }
            precision = f[ps..i].iter().collect::<String>().parse::<usize>().ok();
        }
        let Some(&conv) = f.get(i) else {
            return Err(bad("format ends in the middle of a specifier"));
        };
        i += 1;
        if conv == 'n' {
            out.push('\n');
            continue;
        }
        if conv == '%' {
            out.push('%');
            continue;
        }
        let a = match index {
            Some(k) if k >= 1 => argv.get(k - 1),
            _ => {
                next += 1;
                argv.get(next - 1)
            }
        }
        .ok_or_else(|| bad("missing format argument"))?;
        let left = flags.contains('-');
        let zero = flags.contains('0');
        let num = |v: String, neg: bool| -> String {
            let mut body = v;
            if neg {
                if flags.contains('(') {
                    body = format!("({body})");
                } else {
                    body = format!("-{body}");
                }
            } else if flags.contains('+') {
                body = format!("+{body}");
            } else if flags.contains(' ') {
                body = format!(" {body}");
            }
            body
        };
        let s = match (conv.to_ascii_lowercase(), arg(a)) {
            ('d', Arg::Int(v, _)) => {
                let digits = v.unsigned_abs().to_string();
                num(
                    if flags.contains(',') {
                        group(&digits)
                    } else {
                        digits
                    },
                    v < 0,
                )
            }
            ('x' | 'o', Arg::Int(v, w)) => {
                let u = if w == 64 {
                    v as u64
                } else {
                    (v as u64) & ((1u64 << w) - 1)
                };
                let s = if conv.eq_ignore_ascii_case(&'x') {
                    format!("{u:x}")
                } else {
                    format!("{u:o}")
                };
                if conv == 'X' { s.to_uppercase() } else { s }
            }
            ('f', Arg::Float(v)) => {
                if v.is_nan() || v.is_infinite() {
                    crate::fmt::double_to_string(v)
                } else {
                    let p = precision.unwrap_or(6);
                    let digits = format!("{:.p$}", v.abs());
                    let digits = if flags.contains(',') {
                        let (int, frac) = digits
                            .split_once('.')
                            .map_or((digits.as_str(), None), |(a, b)| (a, Some(b)));
                        let g = group(int);
                        frac.map_or(g.clone(), |fr| format!("{g}.{fr}"))
                    } else {
                        digits
                    };
                    num(digits, v.is_sign_negative() && v != 0.0)
                }
            }
            ('e', Arg::Float(v)) => {
                let p = precision.unwrap_or(6);
                let s = num(java_exp(format!("{:.p$e}", v.abs())), v < 0.0);
                if conv == 'E' { s.to_uppercase() } else { s }
            }
            ('g', Arg::Float(v)) => {
                let p = precision.unwrap_or(6).max(1);
                let a = v.abs();
                let s = if a != 0.0 && (a < 1e-4 || a >= 10f64.powi(p as i32)) {
                    java_exp(format!("{:.*e}", p - 1, a))
                } else {
                    let int_digits = if a < 1.0 {
                        1
                    } else {
                        a.log10().floor() as usize + 1
                    };
                    format!("{:.*}", p.saturating_sub(int_digits), a)
                };
                num(s, v < 0.0)
            }
            ('c', Arg::Char(c)) => String::from_utf16_lossy(&[c]),
            ('c', Arg::Int(v, _)) => {
                char::from_u32(v as u32).map_or(String::new(), |c| c.to_string())
            }
            ('b', Arg::Null) => "false".into(),
            ('b', Arg::Bool(b)) => b.to_string(),
            ('b', _) => "true".into(),
            ('s', _) => {
                let s = text(a)?;
                match precision {
                    Some(p) => s.chars().take(p).collect(),
                    None => s,
                }
            }
            (_, Arg::Null) => "null".into(),
            _ => return Err(bad(&format!("bad conversion %{conv} for its argument"))),
        };
        let s = if conv == 'S' { s.to_uppercase() } else { s };
        let n = s.chars().count();
        if n < width {
            let pad = width - n;
            if left {
                out.push_str(&s);
                out.push_str(&" ".repeat(pad));
            } else if zero && "dxXofeEgG".contains(conv) {
                // Zeros go after the sign.
                let (sign, rest) = match s.chars().next() {
                    Some(c @ ('-' | '+' | ' ')) => (c.to_string(), &s[1..]),
                    _ => (String::new(), s.as_str()),
                };
                out.push_str(&sign);
                out.push_str(&"0".repeat(pad));
                out.push_str(rest);
            } else {
                out.push_str(&" ".repeat(pad));
                out.push_str(&s);
            }
        } else {
            out.push_str(&s);
        }
    }
    Ok(out)
}
