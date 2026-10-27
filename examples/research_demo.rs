//! Native layout fixture. Pass a disposable vault path; no agent process is started.
#[cfg(all(
    target_os = "linux",
    feature = "codex",
    feature = "gui",
    debug_assertions
))]
fn main() -> eframe::Result {
    let vault = std::env::args_os()
        .nth(1)
        .expect("pass a disposable vault path");
    tanglewood::ui::run_research_preview(vault.into())
}

#[cfg(not(all(
    target_os = "linux",
    feature = "codex",
    feature = "gui",
    debug_assertions
)))]
fn main() {
    eprintln!("Use a debug build with --features gui,codex on Linux.");
}
