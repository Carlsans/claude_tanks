//! Rust names for Java ones. Collision-free: `_` itself is escaped.

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' => out.push(c),
            '_' => out.push_str("_u"),
            '/' => out.push('_'),
            '$' => out.push_str("_d"),
            // `<init>` and `<clinit>`: apart from methods named `init`.
            '<' => out.push_str("_l"),
            '>' => {}
            c => out.push_str(&format!("_x{:x}_", c as u32)),
        }
    }
}

/// Module name of a class: `java/util/ArrayList` gives `java_util_ArrayList`.
pub fn class(name: &str) -> String {
    let mut out = String::from("c_");
    escape(name, &mut out);
    out
}

fn fnv(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// Function name of a method: its name and a hash of its descriptor.
pub fn method(name: &str, desc: &str) -> String {
    let mut out = String::from("m_");
    escape(name, &mut out);
    out.push_str(&format!("_{:08x}", fnv(desc)));
    out
}

/// Name of a static field's storage.
pub fn field(name: &str, desc: &str) -> String {
    let mut out = String::from("f_");
    escape(name, &mut out);
    out.push_str(&format!("_{:04x}", fnv(desc) & 0xffff));
    out
}

/// The identifier of a native class's `Class` static in the runtime.
pub fn native_class(name: &str) -> String {
    format!(
        "C_{}",
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    )
}
