//! A GPU driver that crashes during setup must not take the program down:
//! `gpu::get` tries it in a child process first. Here the child is this test
//! binary itself, which aborts (a native crash, like an access violation).

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--crash") {
        std::process::abort();
    }
    claude_tanks::gpu::check_in_child("--crash");
    assert!(claude_tanks::gpu::get().is_none());
    let status = claude_tanks::gpu::status();
    assert!(status.contains("crashed"), "{status}");
    println!("gpu_check: survived a crashing GPU check ({status})");
}
