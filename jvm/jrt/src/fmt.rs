//! Java's number formatting and parsing (`Double.toString`, `parseInt`...).

/// `Double.toString`: the shortest digits that read back the same, in plain
/// notation from 10^-3 to 10^7 and computerized scientific notation
/// (`1.0E10`) outside, always with a digit after the point.
pub fn double_to_string(d: f64) -> String {
    if d.is_nan() {
        return "NaN".into();
    }
    if d.is_infinite() {
        return if d > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if d == 0.0 {
        return if d.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    java_layout(d < 0.0, &format!("{:e}", d.abs()))
}

/// `Float.toString`, by the same rules with float precision.
pub fn float_to_string(f: f32) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    java_layout(f < 0.0, &format!("{:e}", f.abs()))
}

/// Lays out Rust's shortest scientific form (`1.2345e-5`) the Java way.
fn java_layout(neg: bool, sci: &str) -> String {
    let (mant, exp) = sci.split_once('e').unwrap_or((sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if (-3..7).contains(&exp) {
        if exp >= 0 {
            let int_len = (exp + 1) as usize;
            if digits.len() <= int_len {
                out += &digits;
                out += &"0".repeat(int_len - digits.len());
                out += ".0";
            } else {
                out += &digits[..int_len];
                out.push('.');
                out += &digits[int_len..];
            }
        } else {
            out += "0.";
            out += &"0".repeat((-exp - 1) as usize);
            out += &digits;
        }
    } else {
        out += &digits[..1];
        out.push('.');
        if digits.len() > 1 {
            out += &digits[1..];
        } else {
            out.push('0');
        }
        out.push('E');
        out += &exp.to_string();
    }
    out
}

/// `Integer.parseInt` / `Long.parseLong` in base 10. `None` where Java
/// throws `NumberFormatException`.
pub fn parse_long(s: &str, min: i64, max: i64) -> Option<i64> {
    let (neg, digits) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: i128 = 0;
    for b in digits.bytes() {
        v = v * 10 + i128::from(b - b'0');
        if v > i128::from(max) + 1 {
            return None;
        }
    }
    let v = if neg { -v } else { v };
    (i128::from(min)..=i128::from(max))
        .contains(&v)
        .then_some(v as i64)
}

/// `Double.parseDouble`: leading and trailing whitespace allowed, an optional
/// `d`/`f` suffix, `NaN` and `Infinity`.
pub fn parse_double(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let t = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    match t {
        "NaN" | "+NaN" | "-NaN" => Some(f64::NAN),
        "Infinity" | "+Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ if t.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E') => None,
        _ => t.parse().ok(),
    }
}
