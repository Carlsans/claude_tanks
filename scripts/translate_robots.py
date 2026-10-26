#!/usr/bin/env python3
"""Translates classic Robocode robots to Rust with jvm2rs and builds them
into one program, `rumble-robots`, that runs any of them as a Tank Royale
bot (`rumble-robots <key>`). Installs a bot folder per robot that starts it,
so translated robots replace the Java ones wherever bot folders are used.

    scripts/translate_robots.py [--bots bots/rumble] [--out target/jvmbots]
        [--install bots/rumble-rs] [--jobs N] [--only key,...] [--skip-build]

Each robot's own classes (from its jar) are translated apart; what all of
them use (the Java bridge, the bot API, the Java library) is translated once
into the crate `jcommon`. Robots are grouped into batch crates so cargo
builds them in parallel. A robot whose translation or Rust does not compile
is left out (and listed in <out>/failed.json).
"""
import argparse, concurrent.futures as cf, json, os, re, shutil, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIG_KB = 5000  # a robot with more generated code than this builds on its own
# Left out: rustc needs over 14 GB and 20 minutes for each of these.
SKIP = {"r_krabb_slink_garm_0_9u": "too large to compile"}
# rustc needs over 10 GB for 40 robots at once.
BATCH = 10


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def jdk_modules():
    props = run(["java", "-XshowSettings:properties", "-version"]).stderr
    home = next(l.split("=", 1)[1].strip() for l in props.splitlines() if l.strip().startswith("java.home"))
    return [os.path.join(home, "jmods", m) for m in ("java.base.jmod", "java.desktop.jmod")]


def key_of(d):
    return "r_" + re.sub(r"[^a-z0-9]", "_", d.lower())


def robots(bots):
    out = []
    for d in sorted(os.listdir(bots)):
        p = os.path.join(bots, d)
        w, jar, js = os.path.join(p, "Wrapper.java"), p + ".jar", os.path.join(p, d + ".json")
        if not (os.path.isfile(w) and os.path.isfile(jar) and os.path.isfile(js)):
            continue
        m = re.search(r'forName\("([^"]+)"\)', open(w).read())
        if m:
            out.append({"dir": d, "key": key_of(d), "class": m.group(1), "jar": jar, "json": js})
    return out


def build_shims(out):
    src = os.path.join(ROOT, "jvm", "shims", "src")
    dst = os.path.join(out, "shims")
    os.makedirs(dst, exist_ok=True)
    files = [os.path.join(r, f) for r, _, fs in os.walk(src) for f in fs if f.endswith(".java")]
    r = run(["javac", "-nowarn", "--patch-module", "java.base=" + src, "-d", dst] + files)
    if r.returncode:
        sys.exit("shims: " + r.stderr)
    return dst


def sync_dir(new, old):
    """Makes `old` the same as `new`, touching only files whose content changed:
    cargo rebuilds a crate when a source file is newer than its last build."""
    for root, _, files in os.walk(new):
        for f in files:
            src = os.path.join(root, f)
            dst = os.path.join(old, os.path.relpath(src, new))
            if os.path.exists(dst) and open(src, "rb").read() == open(dst, "rb").read():
                continue
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copyfile(src, dst)
    for root, _, files in os.walk(old, topdown=False):
        for f in files:
            dst = os.path.join(root, f)
            if not os.path.exists(os.path.join(new, os.path.relpath(dst, old))):
                os.remove(dst)
        if root != old and not os.listdir(root):
            os.rmdir(root)
    shutil.rmtree(new)


def translate_robot(jvm2rs, common_cp, out, r):
    mod_dir = os.path.join(out, "robots", r["key"])
    tmp = mod_dir + ".new"
    shutil.rmtree(tmp, ignore_errors=True)
    cp = os.pathsep.join([r["jar"]] + common_cp)
    res = run([jvm2rs, "--cp", cp, "--robot", r["class"], r["json"], "--split-robot", r["key"], "jcommon", "--out", tmp])
    if res.returncode == 0:
        sync_dir(tmp, mod_dir)
    with open(mod_dir + ".report", "w") as f:
        f.write(res.stderr)
    return r["key"], res.returncode == 0


def dir_kb(path):
    return sum(os.path.getsize(os.path.join(r, f)) for r, _, fs in os.walk(path) for f in fs) // 1024


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if os.path.exists(path) and open(path).read() == text:
        return
    with open(path, "w") as f:
        f.write(text)


ALLOW = "#![allow(non_snake_case, non_upper_case_globals, unused, unreachable_code, unreachable_patterns, clippy::all)]\n"

MAIN = r'''//! Runs one translated robot as a Tank Royale bot: `rumble-robots <key>`
//! joins the server at `SERVER_URL` (or replays `JRT_REPLAY`, a file of the
//! server's messages, printing the robot's answers).
use tungstenite::Message;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn start(key: &str) -> Option<fn() -> Result<(), String>> {
    Some(match key {
%ARMS%
        _ => return None,
    })
}

fn main() {
    let key = std::env::args().nth(1).unwrap_or_default();
    let Some(start) = start(&key) else {
        eprintln!("no translated robot {key}");
        std::process::exit(2);
    };
    if let Err(e) = start() {
        eprintln!("robot did not start: {e}");
        std::process::exit(1);
    }
    if let Ok(file) = std::env::var("JRT_REPLAY") {
        for line in std::fs::read_to_string(&file).expect("replay file").lines() {
            jrt::robot::deliver(line);
            for m in jrt::robot::take_outbox() {
                println!("{m}");
            }
        }
        return;
    }
    let url = std::env::var("SERVER_URL").unwrap_or_else(|_| "ws://localhost:7654".to_string());
    let (mut ws, _) = match tungstenite::connect(url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot connect to {url}: {e}");
            std::process::exit(1);
        }
    };
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                jrt::robot::deliver(t.as_str());
                for m in jrt::robot::take_outbox() {
                    if ws.send(Message::text(m)).is_err() {
                        return;
                    }
                }
                if jrt::robot::is_closed() {
                    let _ = ws.close(None);
                    return;
                }
            }
            Ok(Message::Close(_)) | Err(_) => return,
            _ => {}
        }
    }
}
'''


def generate(out, batches, jrt):
    members = ["jcommon", "rumble_robots"]
    deps = []
    arms = []
    for b, ks in enumerate(batches):
        name = f"batch{b:02}"
        members.append(f"batches/{name}")
        deps.append(f'{name} = {{ path = "../batches/{name}" }}')
        lib = ALLOW + "//! Translated robots.\n"
        for k in ks:
            lib += f'#[path = "{os.path.join(out, "robots", k, "mod.rs")}"]\npub mod {k};\n'
            arms.append(f'        "{k}" => {name}::{k}::start,')
        write(os.path.join(out, "batches", name, "src", "lib.rs"), lib)
        write(os.path.join(out, "batches", name, "Cargo.toml"),
              f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\njrt = {{ path = "{jrt}" }}\njcommon = {{ path = "../../jcommon" }}\n')
    # Remove batches from an earlier, larger run.
    bdir = os.path.join(out, "batches")
    for d in os.listdir(bdir) if os.path.isdir(bdir) else []:
        if f"batches/{d}" not in members:
            shutil.rmtree(os.path.join(bdir, d))
    write(os.path.join(out, "rumble_robots", "src", "main.rs"), ALLOW + MAIN.replace("%ARMS%", "\n".join(arms)))
    write(os.path.join(out, "rumble_robots", "Cargo.toml"),
          '[package]\nname = "rumble-robots"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\n'
          f'jrt = {{ path = "{jrt}" }}\njcommon = {{ path = "../jcommon" }}\ntungstenite = "0.30"\nmimalloc = "0.1"\n' + "\n".join(deps) + "\n")
    write(os.path.join(out, "Cargo.toml"),
          "[workspace]\nresolver = \"2\"\nmembers = [" + ", ".join(f'"{m}"' for m in members) + "]\n\n"
          "[profile.release]\ndebug = false\nincremental = false\npanic = \"abort\"\nlto = \"thin\"\n")


def build(out, jobs):
    """Builds; returns the robot keys whose Rust failed to compile."""
    r = run(["cargo", "build", "--release", "--keep-going", "-j", str(jobs), "--manifest-path",
             os.path.join(out, "Cargo.toml"), "--message-format", "short"], cwd=out)
    with open(os.path.join(out, "build.log"), "w") as f:
        f.write(r.stderr)
    bad = set(re.findall(r"robots/(r_[a-z0-9_]+)/", r.stderr)) if r.returncode else set()
    if r.returncode and not bad:
        sys.exit("build failed outside the robots:\n" + r.stderr[-6000:])
    return bad, r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bots", default=os.path.join(ROOT, "bots", "rumble"))
    ap.add_argument("--out", default=os.path.join(ROOT, "target", "jvmbots"))
    ap.add_argument("--install", default=os.path.join(ROOT, "bots", "rumble-rs"))
    ap.add_argument("--jobs", type=int, default=4, help="translations at once, and compilations unless --build-jobs")
    ap.add_argument("--build-jobs", type=int, default=0)
    ap.add_argument("--only", default="")
    ap.add_argument("--skip-build", action="store_true")
    a = ap.parse_args()
    out = os.path.abspath(a.out)
    os.makedirs(out, exist_ok=True)
    jvm2rs = os.path.join(ROOT, "target", "release", "jvm2rs")
    r = run(["cargo", "build", "--release", "-p", "jvm2rs"], cwd=ROOT)
    if r.returncode:
        sys.exit(r.stderr)
    lib = os.path.join(a.bots, "lib")
    shims = build_shims(out)
    common_cp = [shims, os.path.join(lib, "robocode-api.jar"), os.path.join(lib, "bot-api.jar")] + jdk_modules()
    rs = robots(a.bots)
    if a.only:
        keep = set(a.only.split(","))
        rs = [r for r in rs if r["key"] in keep]
    print(f"{len(rs)} robots", flush=True)
    failed = {}
    ok = []
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        for i, (k, good) in enumerate(ex.map(lambda r: translate_robot(jvm2rs, common_cp, out, r), rs)):
            (ok.append(k) if good else failed.setdefault(k, "translation"))
            if (i + 1) % 100 == 0:
                print(f"  translated {i + 1}", flush=True)
    print(f"translated {len(ok)}, failed {len(failed)}", flush=True)
    # The shared crate, from everything the robots need.
    common = os.path.join(out, "jcommon")
    res = run([jvm2rs, "--cp", os.pathsep.join(common_cp), "--common", os.path.join(out, "robots"),
               "--out", common + ".new", "--name", "jcommon"])
    print(res.stderr.splitlines()[0] if res.stderr else "jcommon", flush=True)
    if res.returncode:
        sys.exit(res.stderr)
    if os.path.isdir(common):
        sync_dir(common + ".new", common)
    else:
        os.rename(common + ".new", common)
    jrt = os.path.join(ROOT, "jvm", "jrt")
    keys = sorted(ok)
    # Robots keep their batch when others are left out, so only the batches
    # that lost a robot build again.
    # The few huge ones (rustc needs a quarter of an hour for each) get a crate
    # of their own, after the others, so they build side by side with the rest.
    big = [k for k in keys if dir_kb(os.path.join(out, "robots", k)) > BIG_KB]
    rest = [k for k in keys if k not in big]
    batches = [rest[i:i + BATCH] for i in range(0, len(rest), BATCH)] + [[k] for k in big]
    # Kept as empty crates, so the others keep their numbers (and their builds).
    for k, why in SKIP.items():
        if k in keys:
            failed[k] = why
    batches = [[k for k in b if k not in SKIP] for b in batches]
    while not a.skip_build:
        generate(out, batches, jrt)
        bad, r = build(out, a.build_jobs or a.jobs)
        if not bad:
            break
        print(f"  {len(bad)} robots do not compile: {sorted(bad)[:8]}", flush=True)
        for k in bad:
            failed[k] = "compile"
        batches = [[k for k in b if k not in bad] for b in batches]
    else:
        generate(out, batches, jrt)
    keys = [k for b in batches for k in b]
    with open(os.path.join(out, "failed.json"), "w") as f:
        json.dump(failed, f, indent=1)
    if a.skip_build:
        return
    # Install: the program and a bot folder per robot that starts it.
    inst = os.path.abspath(a.install)
    os.makedirs(inst, exist_ok=True)
    shutil.copy2(os.path.join(out, "target", "release", "rumble-robots"), os.path.join(inst, "rumble-robots"))
    by_key = {r["key"]: r for r in rs}
    for k in keys:
        r = by_key[k]
        d = os.path.join(inst, r["dir"])
        os.makedirs(d, exist_ok=True)
        shutil.copy2(r["json"], os.path.join(d, r["dir"] + ".json"))
        with open(os.path.join(d, r["dir"] + ".sh"), "w") as f:
            f.write(f"#!/bin/sh\nexec ../rumble-robots {k}\n")
    print(f"installed {len(keys)} robots in {inst}; {len(failed)} left out (see {out}/failed.json)")


main()
