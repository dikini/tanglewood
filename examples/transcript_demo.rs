//! Local presentation fixture: no vault, network or agent process.
#[cfg(all(
    target_os = "linux",
    feature = "codex",
    feature = "gui",
    debug_assertions
))]
fn main() -> eframe::Result {
    tanglewood::ui::run_transcript_preview()
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
