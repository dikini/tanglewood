#![cfg(feature = "wasm-extensions")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Demo(PathBuf);
impl Drop for Demo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn invoke(vault: &Path, arguments: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_tanglewood"))
        .args(["--headless", "--vault"])
        .arg(vault)
        .args(arguments)
        .output()
        .expect("launch Tanglewood CLI");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn cli_guests_read_an_actual_note_and_interpreter_source_can_change() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let components = root.join("target/wasm-extensions/wasm32-wasip2/release");
    let compiled = components.join("tanglewood_compiled_guest.wasm");
    let interpreter = components.join("tanglewood_rhai_guest.wasm");
    assert!(
        compiled.is_file() && interpreter.is_file(),
        "run ./scripts/build-wasm-extensions.sh first"
    );
    let demo = Demo(std::env::temp_dir().join(format!("tanglewood-wasm-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir(&demo.0).unwrap();
    let vault = demo.0.join("vault");
    let source = demo.0.join("source.md");
    std::fs::write(&source, "one two three").unwrap();
    let imported = invoke(
        &vault,
        &["--import", source.to_str().unwrap(), "Notes/demo.md"],
    );
    let listing = String::from_utf8(imported.stdout).unwrap();
    let id = listing
        .split_whitespace()
        .next()
        .expect("imported note UUID");
    uuid::Uuid::parse_str(id).expect("valid note UUID");
    let output = invoke(
        &vault,
        &[
            "--wasm-component",
            compiled.to_str().unwrap(),
            "--wasm-note",
            id,
            "--wasm-input",
            "Found ",
        ],
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "Found Notes/demo.md: 3 words"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("summarizing"));

    let script = demo.0.join("source.rhai");
    std::fs::copy(
        root.join("examples/wasm-extensions/scripts/note-summary.rhai"),
        &script,
    )
    .unwrap();
    let arguments = [
        "--wasm-component",
        interpreter.to_str().unwrap(),
        "--wasm-script",
        script.to_str().unwrap(),
        "--wasm-note",
        id,
        "--wasm-input",
        "Script: ",
    ];
    let output = invoke(&vault, &arguments);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "Script: Notes/demo.md: 13 characters"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Rhai read the selected note"));
    // Change only source: reuse the exact same interpreter component path.
    std::fs::copy(
        root.join("examples/wasm-extensions/scripts/note-text.rhai"),
        &script,
    )
    .unwrap();
    let output = invoke(&vault, &arguments);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "Script: Notes/demo.md\none two three"
    );
}
