#![cfg(feature = "wasm-extensions")]

use std::path::PathBuf;
use tanglewood::wasm_extensions::{
    Capabilities, ExtensionError, Invocation, NoteSnapshot, Runtime,
};

fn component(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/wasm-extensions/wasm32-wasip2/release")
        .join(name);
    assert!(
        path.is_file(),
        "missing {}: run ./scripts/build-wasm-extensions.sh first",
        path.display()
    );
    path
}

fn capabilities() -> Capabilities {
    Capabilities {
        log: true,
        note: Some(NoteSnapshot {
            id: "note-id".into(),
            path: "notes/example.md".into(),
            revision: "revision".into(),
            text: "one two three".into(),
        }),
    }
}

#[test]
fn components_are_bounded_isolated_and_use_authorized_host_capabilities() {
    let compiled = component("tanglewood_compiled_guest.wasm");
    let interpreter = component("tanglewood_rhai_guest.wasm");
    let fixture = component("tanglewood_wasm_fixture.wasm");
    let unauthorized = component("tanglewood_wasm_unauthorized_fixture.wasm");
    let runtime = Runtime::new();

    let run = |path: &PathBuf, invocation, capabilities| {
        runtime
            .start(path.clone(), invocation, capabilities)
            .expect("start worker")
            .wait()
    };
    let compiled_invocation = |input: &str| Invocation::Compiled {
        input: input.into(),
    };
    let evaluated = |source: &str| Invocation::Interpreter {
        source: source.into(),
        input: "Input: ".into(),
    };

    let result = run(&compiled, compiled_invocation("Found "), capabilities()).unwrap();
    assert_eq!(result.output, "Found notes/example.md: 3 words");
    assert_eq!(result.diagnostics.len(), 1);

    let result = run(
        &interpreter,
        evaluated(
            "let note = read_note(); log(\"Rhai read the selected note\"); input + note.path",
        ),
        capabilities(),
    )
    .unwrap();
    assert_eq!(result.output, "Input: notes/example.md");
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        run(
            &interpreter,
            evaluated("input + \"changed\""),
            capabilities()
        )
        .unwrap()
        .output,
        "Input: changed"
    );
    assert!(matches!(
        run(&interpreter, evaluated("let = ;"), capabilities()),
        Err(ExtensionError::Application(_))
    ));
    assert!(matches!(
        run(
            &interpreter,
            evaluated("for n in 0..200 { try { log(\"x\"); } catch (err) {} } \"done\""),
            Capabilities {
                log: false,
                ..capabilities()
            }
        ),
        Err(ExtensionError::ResourceExhausted(_))
    ));
    assert!(matches!(
        run(&interpreter, compiled_invocation(""), capabilities()),
        Err(ExtensionError::Interface(_))
    ));

    assert!(matches!(
        run(
            &compiled,
            compiled_invocation(""),
            Capabilities {
                log: true,
                note: None
            }
        ),
        Err(ExtensionError::Application(_))
    ));
    assert!(matches!(
        run(
            &compiled,
            compiled_invocation(""),
            Capabilities {
                log: false,
                ..capabilities()
            }
        ),
        Err(ExtensionError::Application(_))
    ));
    assert!(matches!(
        run(&unauthorized, compiled_invocation(""), capabilities()),
        Err(ExtensionError::Interface(_))
    ));
    assert!(matches!(
        run(&fixture, compiled_invocation("trap"), capabilities()),
        Err(ExtensionError::Trap(_))
    ));
    assert!(matches!(
        run(&fixture, compiled_invocation("spin"), capabilities()),
        Err(ExtensionError::ResourceExhausted(_))
    ));
    assert!(matches!(
        run(&fixture, compiled_invocation("grow"), capabilities()),
        Err(ExtensionError::ResourceExhausted(_))
    ));
    let started = std::time::Instant::now();
    assert!(matches!(
        run(&fixture, compiled_invocation("sleep"), capabilities()),
        Err(ExtensionError::Trap(_))
    ));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    for _ in 0..2 {
        assert_eq!(
            run(&fixture, compiled_invocation("bump"), capabilities())
                .unwrap()
                .output,
            "0"
        );
    }

    // A completed job's cancellation must not interrupt a subsequent store.
    let previous = runtime
        .start(fixture.clone(), compiled_invocation("bump"), capabilities())
        .unwrap();
    assert_eq!(previous.wait().unwrap().output, "0");
    let next = runtime
        .start(fixture.clone(), compiled_invocation("spin"), capabilities())
        .unwrap();
    previous.cancel();
    assert!(matches!(
        next.wait(),
        Err(ExtensionError::ResourceExhausted(_))
    ));

    let job = runtime
        .start(fixture.clone(), compiled_invocation("spin"), capabilities())
        .unwrap();
    assert!(matches!(
        runtime.start(fixture.clone(), compiled_invocation("bump"), capabilities()),
        Err(ExtensionError::Busy)
    ));
    job.cancel();
    assert!(matches!(job.wait(), Err(ExtensionError::Cancelled)));
    assert_eq!(
        run(&fixture, compiled_invocation("bump"), capabilities())
            .unwrap()
            .output,
        "0"
    );
}

#[test]
fn component_instance_count_limit_is_resource_exhaustion() {
    // Append a real core module and 33 instantiations to the compiled guest.
    // The original component's exports and WIT contract remain intact.
    fn read_leb(bytes: &[u8], offset: &mut usize) -> u32 {
        let mut value = 0;
        for shift in (0..35).step_by(7) {
            let byte = bytes[*offset];
            *offset += 1;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
        }
        panic!("invalid section length");
    }
    fn write_leb(mut value: u32, bytes: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    let mut bytes = std::fs::read(component("tanglewood_compiled_guest.wasm")).unwrap();
    let mut position = 8;
    let mut module_index = 0;
    while position < bytes.len() {
        let section = bytes[position];
        position += 1;
        let size = read_leb(&bytes, &mut position) as usize;
        module_index += u32::from(section == 1);
        position += size;
    }
    let empty_core_module = b"\0asm\x01\0\0\0";
    bytes.push(1); // Component core-module section.
    write_leb(empty_core_module.len() as u32, &mut bytes);
    bytes.extend_from_slice(empty_core_module);
    let mut instances = Vec::new();
    write_leb(33, &mut instances);
    for _ in 0..33 {
        instances.push(0); // Instantiate core module.
        write_leb(module_index, &mut instances);
        instances.push(0); // No imports.
    }
    bytes.push(2); // Component core-instance section.
    write_leb(instances.len() as u32, &mut bytes);
    bytes.extend_from_slice(&instances);

    let path = std::env::temp_dir().join(format!(
        "tanglewood-wasm-limit-{}.wasm",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, bytes).unwrap();
    let outcome = Runtime::new()
        .start(
            path.clone(),
            Invocation::Compiled {
                input: String::new(),
            },
            capabilities(),
        )
        .unwrap()
        .wait();
    std::fs::remove_file(path).unwrap();
    assert!(
        matches!(outcome, Err(ExtensionError::ResourceExhausted(_))),
        "{outcome:?}"
    );
}
