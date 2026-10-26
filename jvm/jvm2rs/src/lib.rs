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
    /// A classic Robocode robot class: the output is a bot program that
    /// runs it through the translated bridge and bot API, connecting to the
    /// server at `SERVER_URL` like a Java bot.
    pub robot: Option<Robot>,
    /// Translating many robots: each robot's own classes apart, everything
    /// else once.
    pub split: Option<Split>,
}

pub enum Split {
    /// A robot's own classes (the first class-path entry) as module
    /// `module` of a larger crate, referring to the crate `common` for the
    /// rest. `out_dir` is the module's folder; it gets `needs.json`, what the
    /// robot uses of the shared code.
    Robot { module: String, common: String },
    /// The shared crate: everything the robots' `needs.json` files (found
    /// under this folder) ask for.
    Common { needs: PathBuf },
}

/// What a robot uses of the shared code.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct Needs {
    pub methods: Vec<(String, String)>,
    pub instantiated: Vec<String>,
    pub used: Vec<String>,
}

fn collect_needs(dir: &Path, out: &mut Needs) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_needs(&p, out);
        } else if p.file_name().is_some_and(|n| n == "needs.json")
            && let Ok(n) =
                serde_json::from_str::<Needs>(&std::fs::read_to_string(&p).unwrap_or_default())
        {
            out.methods.extend(n.methods);
            out.instantiated.extend(n.instantiated);
            out.used.extend(n.used);
        }
    }
}

pub struct Robot {
    pub class: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
}

impl Robot {
    /// The methods a robot program starts from.
    pub fn roots(&self) -> (Vec<Impl>, Vec<String>) {
        let i = |c: &str, s: &str| Impl {
            class: c.to_string(),
            sig: s.to_string(),
        };
        (
            vec![
                i(&self.class, "<init>()V"),
                i(
                    "dev/robocode/tankroyale/botapi/BotInfo",
                    "<init>(Ljava/lang/String;Ljava/lang/String;Ljava/util/List;Ljava/lang/String;Ljava/lang/String;Ljava/util/List;Ljava/util/Collection;Ljava/lang/String;Ljava/lang/String;Ldev/robocode/tankroyale/botapi/InitialPosition;)V",
                ),
                i(
                    "dev/robocode/tankroyale/bridge/BotPeer",
                    "<init>(Lrobocode/robotinterfaces/IBasicRobot;Ldev/robocode/tankroyale/botapi/BotInfo;)V",
                ),
                i("dev/robocode/tankroyale/bridge/BotPeer", "start()V"),
            ],
            vec![
                self.class.clone(),
                "dev/robocode/tankroyale/botapi/BotInfo".into(),
                "dev/robocode/tankroyale/bridge/BotPeer".into(),
            ],
        )
    }
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
    let mut lits = codegen::Lits::default();
    if let Some(Split::Robot { module, common }) = &opts.split {
        world.paths = world::Paths {
            own: format!("crate::{module}"),
            other: common.clone(),
            own_source: Some(0),
        };
        lits.prefix = format!("crate::{module}");
    }
    let mut roots = opts.roots.clone();
    if let Some(m) = &opts.main {
        roots.push(Impl {
            class: m.clone(),
            sig: "main([Ljava/lang/String;)V".into(),
        });
    }
    let mut instantiated = opts.instantiated.clone();
    if let Some(r) = &opts.robot {
        let (rr, ri) = r.roots();
        roots.extend(rr);
        instantiated.extend(ri);
    }
    let mut used = Vec::new();
    if let Some(Split::Common { needs }) = &opts.split {
        let mut n = Needs::default();
        collect_needs(needs, &mut n);
        roots.extend(
            n.methods
                .into_iter()
                .map(|(class, sig)| Impl { class, sig }),
        );
        instantiated.extend(n.instantiated);
        used = n.used;
        roots.sort();
        roots.dedup();
        instantiated.sort();
        instantiated.dedup();
        used.sort();
        used.dedup();
    }
    let reach = Reach::run_with(&mut world, &roots, &instantiated, &used);
    let mut notes: BTreeMap<String, usize> = reach.unresolved.clone();
    if let Some(Split::Robot { module, common }) = &opts.split {
        return robot_module(&mut world, &mut lits, &reach, opts, module, common, notes);
    }
    let src = opts.out_dir.join("src");
    let cdir = src.join("c");
    std::fs::create_dir_all(&cdir).map_err(|e| e.to_string())?;
    let mut mods = String::from("//! One module per translated class.\n");
    let mut registry = Vec::new();
    let mut classes = 0;
    for name in &reach.used {
        if world.is_native(name) {
            continue;
        }
        let m = mangle::class(name);
        let code = codegen::class_module(&mut world, &mut lits, &reach, name, &mut notes);
        write_if_changed(&cdir.join(format!("{m}.rs")), &code)?;
        writeln!(mods, "pub mod {m};").unwrap();
        registry.push(format!("&{m}::CLASS"));
        classes += 1;
    }
    writeln!(
        mods,
        "\n/// Every translated class, for lookups by name.\npub static CLASSES: &[&jrt::Class] = &[{}];",
        registry.join(", ")
    )
    .unwrap();
    write_if_changed(&cdir.join("mod.rs"), &mods)?;
    write_if_changed(&src.join("lits.rs"), &lits.module())?;
    let allow = "#![allow(non_snake_case, non_upper_case_globals, unused, unreachable_code, unreachable_patterns, clippy::all)]\n";
    let root = match (&opts.main, &opts.robot) {
        (Some(m), _) => {
            let f = mangle::method("main", "([Ljava/lang/String;)V");
            format!(
                "{allow}//! Translated by jvm2rs.\npub mod c;\npub mod lits;\n\nfn main() {{\n    jrt::reflect::register(c::CLASSES);\n    std::process::exit(jrt::run_main(c::{}::{f}));\n}}\n",
                mangle::class(m)
            )
        }
        (None, Some(r)) => robot_main(allow, r),
        (None, None) => format!("{allow}//! Translated by jvm2rs.\npub mod c;\npub mod lits;\n"),
    };
    let root_file = if opts.main.is_some() || opts.robot.is_some() {
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
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\njrt = {{ path = {:?} }}\n{}\n[profile.release]\ndebug = false\n{}",
        opts.crate_name,
        jrt.display().to_string(),
        if opts.robot.is_some() {
            "tungstenite = \"0.30\"\n"
        } else {
            ""
        },
        // The shared crate is a member of the robots' workspace.
        if matches!(opts.split, Some(Split::Common { .. })) {
            ""
        } else {
            "\n[workspace]\n"
        }
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

/// A robot translated on its own: its classes as a module of a larger crate,
/// with `start()`, and `needs.json` for the shared crate.
fn robot_module(
    world: &mut World,
    lits: &mut codegen::Lits,
    reach: &Reach,
    opts: &Options,
    _module: &str,
    common: &str,
    mut notes: BTreeMap<String, usize>,
) -> Result<Report, String> {
    let r = opts
        .robot
        .as_ref()
        .ok_or("a split robot translation needs --robot")?;
    let cdir = opts.out_dir.join("c");
    std::fs::create_dir_all(&cdir).map_err(|e| e.to_string())?;
    let mut mods = String::from("//! The robot's own classes.\n");
    let mut registry = Vec::new();
    let mut classes = 0;
    let mut needs = Needs::default();
    for name in &reach.used {
        if world.is_native(name) {
            continue;
        }
        if !world.is_own(name) {
            needs.used.push(name.clone());
            continue;
        }
        let m = mangle::class(name);
        let code = codegen::class_module(world, lits, reach, name, &mut notes);
        write_if_changed(&cdir.join(format!("{m}.rs")), &code)?;
        writeln!(mods, "pub mod {m};").unwrap();
        registry.push(format!("&{m}::CLASS"));
        classes += 1;
    }
    for m in &reach.methods {
        if !world.is_own(&m.class) {
            needs.methods.push((m.class.clone(), m.sig.clone()));
        }
    }
    for c in &reach.instantiated {
        if !world.is_own(c) && !world.is_native(c) {
            needs.instantiated.push(c.clone());
        }
    }
    writeln!(
        mods,
        "\npub static CLASSES: &[&jrt::Class] = &[{}];",
        registry.join(", ")
    )
    .unwrap();
    write_if_changed(&cdir.join("mod.rs"), &mods)?;
    write_if_changed(&opts.out_dir.join("lits.rs"), &lits.module())?;
    let authors: Vec<String> = r
        .authors
        .iter()
        .map(|a| format!("{}.to_string()", rust_str(a)))
        .collect();
    let root = format!(
        "//! `{}`, translated by jvm2rs.\npub mod c;\npub mod lits;\n\n/// Starts the robot (see `jrt::robot`).\npub fn start() -> Result<(), String> {{\n    let info = jrt::robot::Info {{ name: {}.to_string(), version: {}.to_string(), authors: vec![{}] }};\n    jrt::robot::start(&[{common}::c::CLASSES, c::CLASSES], {}, info)\n}}\n",
        r.class,
        rust_str(&r.name),
        rust_str(&r.version),
        authors.join(", "),
        rust_str(&r.class)
    );
    write_if_changed(&opts.out_dir.join("mod.rs"), &root)?;
    let n = serde_json::to_string(&needs).map_err(|e| e.to_string())?;
    write_if_changed(&opts.out_dir.join("needs.json"), &n)?;
    Ok(Report {
        classes,
        methods: reach
            .methods
            .iter()
            .filter(|m| world.is_own(&m.class))
            .count(),
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

fn rust_str(s: &str) -> String {
    format!("{s:?}")
}

/// The entry point of a robot program: starts the robot, then relays
/// messages between its in-process socket and the server's websocket.
fn robot_main(allow: &str, r: &Robot) -> String {
    let authors: Vec<String> = r
        .authors
        .iter()
        .map(|a| format!("{}.to_string()", rust_str(a)))
        .collect();
    format!(
        r#"{allow}//! Translated by jvm2rs: the robot `{class}`, its bridge to the Tank Royale
//! bot API, and the bot API, as one program that joins a server like a bot.
pub mod c;
pub mod lits;

use tungstenite::Message;

fn main() {{
    let url = std::env::var("SERVER_URL").unwrap_or_else(|_| "ws://localhost:7654".to_string());
    let info = jrt::robot::Info {{
        name: {name}.to_string(),
        version: {version}.to_string(),
        authors: vec![{authors}],
    }};
    if let Err(e) = jrt::robot::start(&[c::CLASSES], {class_lit}, info) {{
        eprintln!("robot did not start: {{e}}");
        std::process::exit(1);
    }}
    // Replay: the server's messages from a file (JSON lines), the robot's
    // answers to stdout.
    if let Ok(file) = std::env::var("JRT_REPLAY") {{
        let marks = std::env::var_os("JRT_REPLAY_MARKS").is_some();
        for (k, line) in std::fs::read_to_string(&file).expect("replay file").lines().enumerate() {{
            jrt::robot::deliver(line);
            for m in jrt::robot::take_outbox() {{
                if marks {{
                    println!("{{k}}\t{{m}}");
                }} else {{
                    println!("{{m}}");
                }}
            }}
        }}
        if std::env::var_os("JRT_COUNT").is_some() {{
            for (c, n) in jrt::census(25) {{
                eprintln!("{{n:>9}} {{c}}");
            }}
        }}
        return;
    }}
    let (mut ws, _) = match tungstenite::connect(url.as_str()) {{
        Ok(c) => c,
        Err(e) => {{
            eprintln!("cannot connect to {{url}}: {{e}}");
            std::process::exit(1);
        }}
    }};
    loop {{
        match ws.read() {{
            Ok(Message::Text(t)) => {{
                jrt::robot::deliver(t.as_str());
                for m in jrt::robot::take_outbox() {{
                    if ws.send(Message::text(m)).is_err() {{
                        return;
                    }}
                }}
                if jrt::robot::is_closed() {{
                    let _ = ws.close(None);
                    return;
                }}
            }}
            Ok(Message::Close(_)) | Err(_) => return,
            _ => {{}}
        }}
    }}
}}
"#,
        class = r.class,
        class_lit = rust_str(&r.class),
        name = rust_str(&r.name),
        version = rust_str(&r.version),
        authors = authors.join(", "),
    )
}
