//! Field and method descriptors.

use crate::insn::K;

/// The value kind of a field descriptor (`I`, `[D`, `Ljava/lang/String;`).
pub fn kind(desc: &str) -> K {
    match desc.as_bytes().first() {
        Some(b'J') => K::J,
        Some(b'F') => K::F,
        Some(b'D') => K::D,
        Some(b'L' | b'[') => K::A,
        _ => K::I,
    }
}

/// Splits a run of field descriptors.
pub fn split(mut s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    while !s.is_empty() {
        let b = s.as_bytes();
        let mut i = 0;
        while b[i] == b'[' {
            i += 1;
        }
        let end = if b[i] == b'L' {
            s[i..].find(';').map_or(s.len(), |e| i + e + 1)
        } else {
            i + 1
        };
        out.push(&s[..end]);
        s = &s[end..];
    }
    out
}

/// A method descriptor's parameters and return type (`None` for void).
pub fn method(desc: &str) -> (Vec<&str>, Option<&str>) {
    let close = desc.find(')').unwrap_or(desc.len());
    let params = split(desc.get(1..close).unwrap_or(""));
    let ret = &desc[(close + 1).min(desc.len())..];
    (params, (ret != "V").then_some(ret))
}

/// Rust type of a value kind.
pub fn rust_type(k: K) -> &'static str {
    match k {
        K::I => "i32",
        K::J => "i64",
        K::F => "f32",
        K::D => "f64",
        K::A => "jrt::Ref",
        K::R => "u32",
    }
}

/// The Rust function-pointer type of a method (`this` first unless static).
pub fn fn_type(desc: &str, is_static: bool) -> String {
    let (params, ret) = method(desc);
    let mut args: Vec<&str> = Vec::new();
    if !is_static {
        args.push("jrt::Ref");
    }
    args.extend(params.iter().map(|p| rust_type(kind(p))));
    format!(
        "fn({}) -> jrt::JResult<{}>",
        args.join(", "),
        ret.map_or("()", |r| rust_type(kind(r)))
    )
}

/// The class named by an array descriptor's element or a plain class:
/// `Ljava/lang/String;` gives `java/lang/String`.
pub fn class_of(desc: &str) -> Option<&str> {
    desc.strip_prefix('L').and_then(|s| s.strip_suffix(';'))
}
