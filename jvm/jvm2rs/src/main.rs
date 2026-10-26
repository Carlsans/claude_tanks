//! `jvm2rs --cp <jar|dir>[:...] --out <dir> [--main <class>] [--name <crate>]
//! [--jrt <runtime crate>] [--root <class>.<name><desc>]... [--new <class>]...`
//!
//! Translates what is reachable from the roots (or `main`) into a Rust crate.
use jvm2rs::world::Impl;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut opts = jvm2rs::Options {
        classpath: Vec::new(),
        roots: Vec::new(),
        instantiated: Vec::new(),
        out_dir: PathBuf::from("out"),
        crate_name: "translated".into(),
        jrt: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../jrt")),
        main: None,
    };
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--cp" => opts.classpath.extend(std::env::split_paths(&val())),
            "--out" => opts.out_dir = PathBuf::from(val()),
            "--main" => opts.main = Some(val().replace('.', "/")),
            "--name" => opts.crate_name = val(),
            "--jrt" => opts.jrt = PathBuf::from(val()),
            "--new" => opts.instantiated.push(val().replace('.', "/")),
            "--root" => {
                let v = val();
                let i = v.find('(').expect("--root class.name(desc)ret");
                let dot = v[..i].rfind('.').expect("--root class.name(desc)ret");
                opts.roots.push(Impl {
                    class: v[..dot].replace('.', "/"),
                    sig: v[dot + 1..].to_string(),
                });
            }
            _ => panic!("unknown argument {a}"),
        }
    }
    match jvm2rs::translate(&opts) {
        Ok(r) => {
            eprintln!(
                "{} classes, {} methods translated into {}",
                r.classes,
                r.methods,
                opts.out_dir.display()
            );
            if !r.missing.is_empty() {
                eprintln!(
                    "missing classes ({}): {}",
                    r.missing.len(),
                    r.missing.join(", ")
                );
            }
            if !r.excluded.is_empty() {
                eprintln!(
                    "library classes left out ({}): {}",
                    r.excluded.len(),
                    r.excluded.join(", ")
                );
            }
            for (n, c) in &r.notes {
                eprintln!(
                    "  unsupported: {n}{}",
                    if *c > 1 {
                        format!(" ({c}x)")
                    } else {
                        String::new()
                    }
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
