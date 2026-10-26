//! Lays out the native classes of `src/spec.rs`: their method tables
//! (inherited slots first, then new ones) and interface tables, written as
//! `Class` statics to `$OUT_DIR/classes.rs`.

#[allow(dead_code)]
mod spec {
    include!("src/spec.rs");
}

use spec::*;
use std::collections::BTreeSet;
use std::fmt::Write;

fn class(name: &str) -> &'static NativeClass {
    NATIVE_CLASSES
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("native class {name} is not in the spec"))
}

pub fn ident(name: &str) -> String {
    format!(
        "C_{}",
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    )
}

/// (signature, Rust path or None if abstract) per slot.
fn vtable(c: &NativeClass) -> Vec<(String, Option<String>)> {
    let mut t = match c.sup {
        Some(s) if !c.interface => vtable(class(s)),
        _ => Vec::new(),
    };
    if c.interface {
        return t;
    }
    for m in c.methods.iter().filter(|m| m.kind == Kind::Virtual) {
        let path = (!m.rust.is_empty()).then(|| format!("crate::{}::{}", c.module, m.rust));
        match t.iter_mut().find(|(sig, _)| sig == m.sig) {
            Some(slot) => slot.1 = path,
            None => t.push((m.sig.to_string(), path)),
        }
    }
    t
}

/// Every interface a class implements, through its superclasses and
/// superinterfaces too.
fn all_interfaces(c: &NativeClass, out: &mut BTreeSet<&'static str>) {
    for i in c.interfaces {
        if out.insert(i) {
            all_interfaces(class(i), out);
        }
    }
    if let Some(s) = c.sup.filter(|_| !c.interface) {
        all_interfaces(class(s), out);
    }
}

/// The implementation of `sig` that a class's instances run.
fn implementation(c: &NativeClass, sig: &str) -> Option<String> {
    vtable(c)
        .into_iter()
        .find(|(s, _)| s == sig)
        .and_then(|(_, p)| p)
        .or_else(|| {
            c.methods
                .iter()
                .find(|m| m.sig == sig && m.kind == Kind::Direct && !m.rust.is_empty())
                .map(|m| format!("crate::{}::{}", c.module, m.rust))
        })
}

fn fp(path: &Option<String>) -> String {
    match path {
        Some(p) => format!("Fp({p} as *const ())"),
        None => "Fp(std::ptr::null())".into(),
    }
}

fn main() {
    println!("cargo:rerun-if-changed=src/spec.rs");
    let mut out = String::new();
    for c in NATIVE_CLASSES {
        let id = ident(c.name);
        let (mut prims, mut refs) = (c.prims, c.refs);
        let mut s = c.sup;
        while let Some(n) = s.filter(|_| !c.interface) {
            let p = class(n);
            prims += p.prims;
            refs += p.refs;
            s = p.sup;
        }
        let mut ifaces = BTreeSet::new();
        all_interfaces(c, &mut ifaces);
        let vt: Vec<String> = vtable(c).iter().map(|(_, p)| fp(p)).collect();
        let mut it = Vec::new();
        if !c.interface {
            for i in &ifaces {
                let entries: Vec<String> = class(i)
                    .methods
                    .iter()
                    .filter(|m| m.kind == Kind::Virtual)
                    .map(|m| fp(&implementation(c, m.sig)))
                    .collect();
                it.push(format!("(&{}, &[{}])", ident(i), entries.join(", ")));
            }
        }
        writeln!(
            out,
            "pub static {id}: Class = Class {{ name: {:?}, sup: {}, ifaces: &[{}], interface: {}, prims: {prims}, refs: {refs}, vtable: &[{}], itable: &[{}], ext: {}, component: Component::None }};",
            c.name,
            match c.sup.filter(|_| !c.interface) {
                Some(s) => format!("Some(&{})", ident(s)),
                None => "None".into(),
            },
            ifaces.iter().map(|i| format!("&{}", ident(i))).collect::<Vec<_>>().join(", "),
            c.interface,
            vt.join(", "),
            it.join(", "),
            if c.ext.is_empty() {
                "None".into()
            } else {
                format!("Some(crate::{}::{})", c.module, c.ext)
            },
        )
        .unwrap();
    }
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("classes.rs");
    std::fs::write(path, out).unwrap();
}
