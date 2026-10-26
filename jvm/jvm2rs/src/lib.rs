//! Translates JVM bytecode (class files and jars) into Rust that links
//! against the `jrt` runtime. Works on bytecode, not source, so it handles
//! any JVM language and obfuscated code alike.

pub mod classfile;
pub mod classpath;
pub mod codegen;
pub mod desc;
pub mod indy;
pub mod insn;
pub mod mangle;
pub mod world;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use world::{Impl, Reach, World};

pub struct Options {
    pub classpath: Vec<PathBuf>,
    /// Methods to start from, and classes instantiated from outside.
    pub roots: Vec<Impl>,
    pub instantiated: Vec<String>,
    pub out_dir: PathBuf,
    pub crate_name: String,
    /// The runtime crate, for the output's `Cargo.toml`.
    pub jrt: PathBuf,
    /// A class whose `main(String[])` becomes the program's entry point.
    pub main: Option<String>,
}

pub struct Report {
    pub classes: usize,
    pub methods: usize,
    /// Unsupported or unresolved things the output throws on, with counts.
    pub notes: BTreeMap<String, usize>,
    pub missing: Vec<String>,
    /// Java library classes left out (see `world::Policy`) that code refers to.
    pub excluded: Vec<String>,
}

pub fn translate(opts: &Options) -> Result<Report, String> {
    let mut cp = classpath::ClassPath::new();
    for p in &opts.classpath {
        cp.add(p).map_err(|e| e.to_string())?;
    }
    let mut world = World::new(cp, world::Policy::robots());
    let mut roots = opts.roots.clone();
    if let Some(m) = &opts.main {
        roots.push(Impl {
            class: m.clone(),
            sig: "main([Ljava/lang/String;)V".into(),
        });
    }
    let reach = Reach::run(&mut world, &roots, &opts.instantiated);
    let mut notes: BTreeMap<String, usize> = reach.unresolved.clone();
    let mut lits = codegen::Lits::default();
    let src = opts.out_dir.join("src");
    let cdir = src.join("c");
    std::fs::create_dir_all(&cdir).map_err(|e| e.to_string())?;
    let mut mods = String::from("//! One module per translated class.\n");
    let mut classes = 0;
    for name in &reach.used {
        if world.is_native(name) {
            continue;
        }
        let m = mangle::class(name);
        let code = codegen::class_module(&mut world, &mut lits, &reach, name, &mut notes);
        write_if_changed(&cdir.join(format!("{m}.rs")), &code)?;
        writeln!(mods, "pub mod {m};").unwrap();
        classes += 1;
    }
    write_if_changed(&cdir.join("mod.rs"), &mods)?;
    write_if_changed(&src.join("lits.rs"), &lits.module())?;
    let allow = "#![allow(non_snake_case, non_upper_case_globals, unused, unreachable_code, unreachable_patterns, clippy::all)]\n";
    let root = match &opts.main {
        Some(m) => {
            let f = mangle::method("main", "([Ljava/lang/String;)V");
            format!(
                "{allow}//! Translated by jvm2rs.\npub mod c;\npub mod lits;\n\nfn main() {{\n    std::process::exit(jrt::run_main(c::{}::{f}));\n}}\n",
                mangle::class(m)
            )
        }
        None => format!("{allow}//! Translated by jvm2rs.\npub mod c;\npub mod lits;\n"),
    };
    let root_file = if opts.main.is_some() {
        "main.rs"
    } else {
        "lib.rs"
    };
    write_if_changed(&src.join(root_file), &root)?;
    let jrt = opts
        .jrt
        .canonicalize()
        .map_err(|e| format!("{}: {e}", opts.jrt.display()))?;
    let toml = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\njrt = {{ path = {:?} }}\n\n[profile.release]\ndebug = false\n\n[workspace]\n",
        opts.crate_name,
        jrt.display().to_string()
    );
    write_if_changed(&opts.out_dir.join("Cargo.toml"), &toml)?;
    Ok(Report {
        classes,
        methods: reach.methods.len(),
        notes,
        missing: world.missing.iter().cloned().collect(),
        excluded: world.excluded.iter().cloned().collect(),
    })
}

/// Writes a file unless it already holds `content` (keeps cargo's
/// incremental builds incremental).
fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|c| c == content) {
        return Ok(());
    }
    std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))
}
