//! Differential tests: every program under `tests/java/<name>/` (a `Main`
//! class) must print the same thing translated as on the JVM. Skipped when
//! no JDK is installed.

use std::path::Path;
use std::process::Command;

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), s)
}

#[test]
fn programs_print_what_the_jvm_prints() {
    if Command::new("javac").arg("-version").output().is_err() {
        eprintln!("no javac: skipped");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("diff");
    let jrt = root.join("../jrt");
    let mut dirs: Vec<_> = std::fs::read_dir(root.join("tests/java"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut failures = Vec::new();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let classes = tmp.join(&name).join("classes");
        let _ = std::fs::remove_dir_all(&classes);
        std::fs::create_dir_all(&classes).unwrap();
        let sources: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "java"))
            .collect();
        let (ok, msg) = run(Command::new("javac")
            .args(["--release", "8", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources));
        assert!(ok, "{name}: javac failed:\n{msg}");
        let (_, want) = run(Command::new("java").arg("-cp").arg(&classes).arg("Main"));
        let out = tmp.join(&name).join("rs");
        let report = jvm2rs::translate(&jvm2rs::Options {
            classpath: vec![classes.clone()],
            roots: Vec::new(),
            instantiated: Vec::new(),
            out_dir: out.clone(),
            crate_name: format!("diff_{name}"),
            jrt: jrt.clone(),
            main: Some("Main".into()),
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        for (n, _) in &report.notes {
            eprintln!("{name}: unsupported: {n}");
        }
        let (ok, msg) = run(Command::new(env!("CARGO"))
            .arg("build")
            .arg("--manifest-path")
            .arg(out.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", tmp.join("target")));
        assert!(ok, "{name}: the translation does not compile:\n{msg}");
        let bin = tmp.join("target/debug").join(format!("diff_{name}"));
        let (_, got) = run(&mut Command::new(bin));
        if got != want {
            let first = want.lines().zip(got.lines()).position(|(a, b)| a != b);
            failures.push(format!(
                "{name}: output differs{}\n--- JVM\n{want}\n--- translated\n{got}",
                first.map_or(String::new(), |i| format!(" from line {}", i + 1))
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
