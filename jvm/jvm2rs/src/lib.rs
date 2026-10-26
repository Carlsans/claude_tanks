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
    let reach = Reach::run(&mut world, &roots, &instantiated);
    let mut notes: BTreeMap<String, usize> = reach.unresolved.clone();
    let mut lits = codegen::Lits::default();
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
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\njrt = {{ path = {:?} }}\n{}\n[profile.release]\ndebug = false\n\n[workspace]\n",
        opts.crate_name,
        jrt.display().to_string(),
        if opts.robot.is_some() {
            "tungstenite = \"0.30\"\n"
        } else {
            ""
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
    if let Err(e) = jrt::robot::start(c::CLASSES, {class_lit}, info) {{
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
