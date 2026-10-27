#[cfg(not(feature = "wasm-extensions"))]
#[test]
fn registered_plugins_report_the_missing_feature() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tanglewood"))
        .args([
            "--headless",
            "--plugin-registry",
            "/tmp/plugins",
            "--plugin",
            "demo",
        ])
        .output()
        .expect("launch Tanglewood CLI");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--features wasm-extensions"));
}

#[cfg(feature = "wasm-extensions")]
#[test]
fn plugin_invocation_rejects_missing_or_ambiguous_source() {
    for (arguments, expected) in [
        (
            vec!["--plugin-evaluate", "rhai"],
            "--plugin-evaluate requires --wasm-script",
        ),
        (
            vec!["--plugin-run", "compiled", "--wasm-script", "script.rhai"],
            "--plugin-run does not accept --wasm-script",
        ),
        (
            vec![
                "--plugin-run",
                "compiled",
                "--wasm-component",
                "component.wasm",
            ],
            "choose --wasm-component or a registered plugin invocation",
        ),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tanglewood"))
            .args(["--headless", "--plugin-registry", "/tmp/plugins"])
            .args(arguments)
            .output()
            .expect("launch Tanglewood CLI");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
}

#[cfg(all(feature = "wasm-extensions", target_os = "linux"))]
mod enabled {
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    struct Demo(PathBuf);
    impl Drop for Demo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn install(registry: &Path, id: &str, kind: &str, artifact: &Path) {
        let bytes = std::fs::read(artifact).unwrap_or_else(|_| {
            panic!(
                "missing {}: run ./scripts/build-wasm-extensions.sh first",
                artifact.display()
            )
        });
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let dir = registry.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("component.wasm"), bytes).unwrap();
        let mut manifest = serde_json::json!({
            "manifest_version": 1, "id": id, "version": "1",
            "kind": kind, "wit_version": "0.2.0", "component": "component.wasm",
            "component_sha256": digest,
            "invocation": {"local": true, "local_source": kind == "interpreter"},
            "permissions": {"rules": [
                {"action": "note.create", "note_paths": {"mode": "all"}},
                {"action": "note.edit", "notes": {"mode": "all"}},
                {"action": "note.read", "notes": {"mode": "all"}}
            ]}
        });
        if kind == "interpreter" {
            manifest["languages"] = serde_json::json!(["rhai"]);
        }
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn invoke(vault: &Path, registry: &Path, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tanglewood"))
            .args(["--headless", "--vault"])
            .arg(vault)
            .arg("--plugin-registry")
            .arg(registry)
            .args(arguments)
            .output()
            .expect("launch Tanglewood CLI")
    }

    fn success(output: Output) -> String {
        assert!(
            output.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn registered_compiled_and_interpreter_plugins_use_real_vault() {
        let demo = Demo(
            std::env::temp_dir().join(format!("tanglewood-plugin-cli-{}", uuid::Uuid::new_v4())),
        );
        let registry = demo.0.join("registry");
        let vault = demo.0.join("vault");
        let artifacts = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/wasm-extensions/wasm32-wasip2/release");
        install(
            &registry,
            "compiled",
            "compiled",
            &artifacts.join("tanglewood_compiled_v2_guest.wasm"),
        );
        install(
            &registry,
            "rhai",
            "interpreter",
            &artifacts.join("tanglewood_rhai_v2_guest.wasm"),
        );

        let created = success(invoke(
            &vault,
            &registry,
            &[
                "--plugin-run",
                "compiled",
                "--wasm-input",
                "Notes/compiled.md",
            ],
        ));
        let (id, text) = created.split_once(':').expect("compiled guest result");
        uuid::Uuid::parse_str(id).expect("note UUID");
        assert_eq!(text, "Compiled extension edited");

        let script = demo.0.join("source.rhai");
        std::fs::write(
            &script,
            "let note = read_note(input); \"Script: \" + note.text",
        )
        .unwrap();
        let arguments = [
            "--plugin-evaluate",
            "rhai",
            "--wasm-script",
            script.to_str().unwrap(),
            "--wasm-input",
            id,
        ];
        assert_eq!(
            success(invoke(&vault, &registry, &arguments)),
            "Script: Compiled extension edited"
        );
        std::fs::write(&script, "read_note(input).path").unwrap();
        assert_eq!(
            success(invoke(&vault, &registry, &arguments)),
            "Notes/compiled.md"
        );

        // The guest creates the note before its edit is denied. The CLI must
        // retain an observable receipt and must not suggest a rollback.
        install(
            &registry,
            "denied",
            "compiled",
            &artifacts.join("tanglewood_compiled_v2_guest.wasm"),
        );
        let manifest_path = registry.join("denied/manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["permissions"]["rules"] = serde_json::json!([
            {"action": "note.create", "note_paths": {"mode": "all"}}
        ]);
        std::fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let denied = invoke(
            &vault,
            &registry,
            &["--plugin-run", "denied", "--wasm-input", "Notes/denied.md"],
        );
        assert!(!denied.status.success());
        let error = String::from_utf8_lossy(&denied.stderr);
        assert!(
            error.contains("note.create") && error.contains("Plugin effect:"),
            "{error}"
        );
        assert!(success(invoke(&vault, &registry, &[])).contains("Notes/denied.md"));
    }
}
