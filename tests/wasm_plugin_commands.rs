#![cfg(all(feature = "wasm-extensions", target_os = "linux"))]
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tanglewood::{
    App,
    commands::{Caller, Command, CommandResult},
    plugin_jobs::{PluginCommand, PluginResult, RunPhase, RunStatus},
};
use uuid::Uuid;

struct Fixture {
    root: PathBuf,
    app: App,
}
impl Fixture {
    fn new(rules: serde_json::Value) -> Self {
        let root = std::env::temp_dir().join(format!("tw-plugin-owner-{}", Uuid::new_v4()));
        let bundle = root.join("registry/rhai");
        fs::create_dir_all(&bundle).unwrap();
        let bytes = fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/wasm-extensions/wasm32-wasip2/release/tanglewood_rhai_v2_guest.wasm"),
        )
        .expect("build Wasm guests first");
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        fs::write(bundle.join("component.wasm"), bytes).unwrap();
        fs::write(bundle.join("manifest.json"), serde_json::to_vec(&serde_json::json!({
            "manifest_version":1,"id":"rhai","version":"1","kind":"interpreter","languages":["rhai"],
            "wit_version":"0.2.0","component":"component.wasm","component_sha256":digest,
            "invocation":{"local":true,"automation":true,"local_source":true,"automation_source":true},
            "permissions":{"rules":rules}
        })).unwrap()).unwrap();
        let mut app = App::open(&root.join("vault")).unwrap();
        app.configure_plugin_registry(&root.join("registry"), &["rhai".into()])
            .unwrap();
        Self { root, app }
    }
    fn start(&mut self, source: &str, input: &str, caller: Caller) -> Uuid {
        match self
            .app
            .execute(
                Command::Plugin(PluginCommand::Evaluate {
                    id: "rhai".into(),
                    source: source.into(),
                    input: input.into(),
                }),
                caller,
            )
            .unwrap()
        {
            CommandResult::Plugin(PluginResult::Started { run_id }) => run_id,
            other => panic!("{other:?}"),
        }
    }
    fn status(&mut self, id: Uuid, caller: Caller) -> RunStatus {
        match self
            .app
            .execute(
                Command::Plugin(PluginCommand::Status { run_id: id }),
                caller,
            )
            .unwrap()
        {
            CommandResult::Plugin(PluginResult::Status(status)) => status,
            other => panic!("{other:?}"),
        }
    }
    fn finish(&mut self, id: Uuid, caller: Caller) -> RunStatus {
        let until = Instant::now() + Duration::from_secs(60);
        loop {
            self.app.tick();
            let status = self.status(id, caller);
            if !matches!(status.phase, RunPhase::Running) {
                return status;
            }
            assert!(Instant::now() < until, "plugin stalled");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.app.shutdown();
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn rules() -> serde_json::Value {
    serde_json::json!([
        {"action":"note.create","note_paths":{"mode":"all"}},
        {"action":"note.edit","notes":{"mode":"all"}},
        {"action":"note.read","notes":{"mode":"all"}},
        {"action":"automation.set_write_enabled"}
    ])
}

#[test]
fn real_interpreter_intersects_caller_rights_and_preserves_effects_after_trap() {
    let mut f = Fixture::new(rules());
    let created = f
        .app
        .execute(
            Command::CreateNote {
                path: "seed.md".into(),
                text: "seed".into(),
            },
            Caller::Local,
        )
        .unwrap();
    let CommandResult::Note(note) = created else {
        panic!()
    };
    f.app.flush().unwrap();
    let id = f.start(
        "read_note(input).text",
        &note.note.id.0.to_string(),
        Caller::AutomationReadOnly,
    );
    let result = f.finish(id, Caller::AutomationReadOnly);
    assert!(matches!(result.phase, RunPhase::Completed), "{result:?}");
    assert_eq!(result.output.as_deref(), Some("seed"));
    let denied = f.start(
        "create_note(input, \"denied\").id",
        "denied.md",
        Caller::AutomationReadOnly,
    );
    assert!(matches!(
        f.finish(denied, Caller::AutomationReadOnly).phase,
        RunPhase::Failed
    ));
    assert_eq!(f.app.notes().len(), 1);
    let partial = f.start(
        "let n=create_note(input, \"accepted\"); throw \"later failure\";",
        "partial.md",
        Caller::Local,
    );
    let result = f.finish(partial, Caller::Local);
    assert!(matches!(result.phase, RunPhase::Failed));
    assert_eq!(result.receipts.len(), 1);
    assert_eq!(result.receipts[0].completion, "persisted");
    assert!(
        f.app
            .notes()
            .iter()
            .any(|note| note.note.path == "partial.md")
    );
    assert!(
        f.app
            .execute(
                Command::Plugin(PluginCommand::Status { run_id: partial }),
                Caller::AutomationReadOnly
            )
            .is_err()
    );
    // Query cursors page a note larger than the entire retained-result cache.
    let huge = f
        .app
        .execute(
            Command::CreateNote {
                path: "large.md".into(),
                text: "é".repeat(5 * 1024 * 1024),
            },
            Caller::Local,
        )
        .unwrap();
    let CommandResult::Note(huge) = huge else {
        panic!()
    };
    f.app.flush().unwrap();
    let paged=f.start(r#"let reply=command(#{ReadNote:#{id:input}}); let page=read_result(reply.Err.TooLarge.token,[#{Field:"Note"},#{Field:"note"},#{Field:"text"}],0,4); page.Ok.Text.text + ":" + page.Ok.Text.total_bytes"#,&huge.note.id.0.to_string(),Caller::AutomationReadOnly);
    let status = f.finish(paged, Caller::AutomationReadOnly);
    assert!(matches!(status.phase, RunPhase::Completed), "{status:?}");
    assert_eq!(status.output.as_deref(), Some("éé:10485760"));
}

#[test]
fn revocation_and_write_disable_cancel_runs_without_hiding_status_or_cancel() {
    let mut f = Fixture::new(rules());
    let run = f.start("while true {} \"never\"", "", Caller::AutomationReadOnly);
    f.app
        .execute(
            Command::SetAutomationWriteEnabled { enabled: false },
            Caller::Local,
        )
        .unwrap();
    f.app
        .execute(
            Command::Plugin(PluginCommand::Cancel { run_id: run }),
            Caller::AutomationReadOnly,
        )
        .unwrap();
    let status = f.finish(run, Caller::AutomationReadOnly);
    assert!(matches!(
        status.phase,
        RunPhase::Cancelled | RunPhase::Failed
    ));
    let run = f.start("while true {} \"never\"", "", Caller::Local);
    f.app.revoke_plugin("rhai");
    assert!(matches!(
        f.finish(run, Caller::Local).phase,
        RunPhase::Cancelled | RunPhase::Failed
    ));
    assert!(
        f.app
            .execute(
                Command::Plugin(PluginCommand::Evaluate {
                    id: "rhai".into(),
                    source: "\"no\"".into(),
                    input: String::new()
                }),
                Caller::Local
            )
            .is_err()
    );
}

#[cfg(feature = "mcp")]
#[test]
fn live_mcp_socket_returns_run_id_and_allows_readonly_scripts() {
    use std::os::unix::fs::PermissionsExt;
    use tanglewood::mcp::{
        self, AccessPolicy, Outcome, OwnerSocket, PROTOCOL_VERSION, RequestEnvelope,
    };
    fn request(
        owner: &mut OwnerSocket,
        app: &mut App,
        path: &std::path::Path,
        command: Command,
    ) -> Outcome {
        let path = path.to_owned();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let reply = mcp::forward(
                &path,
                &RequestEnvelope {
                    version: PROTOCOL_VERSION,
                    request_id: Uuid::new_v4(),
                    command: Some(command),
                },
            );
            send.send(reply).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        let reply = loop {
            owner.poll(app);
            if let Ok(reply) = receive.try_recv() {
                break reply.unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        };
        worker.join().unwrap();
        reply.outcome
    }
    let mut f = Fixture::new(rules());
    fs::set_permissions(&f.root, fs::Permissions::from_mode(0o700)).unwrap();
    let socket = f.root.join("owner.sock");
    let mut owner = OwnerSocket::bind(&socket, AccessPolicy::ReadOnly).unwrap();
    assert!(
        matches!(request(&mut owner,&mut f.app,&socket,Command::Plugin(PluginCommand::List)),Outcome::Result(CommandResult::Plugin(PluginResult::Plugins(plugins))) if plugins.len()==1)
    );
    let started = request(
        &mut owner,
        &mut f.app,
        &socket,
        Command::Plugin(PluginCommand::Evaluate {
            id: "rhai".into(),
            source: "\"agent source\"".into(),
            input: String::new(),
        }),
    );
    let Outcome::Result(CommandResult::Plugin(PluginResult::Started { run_id })) = started else {
        panic!("{started:?}")
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let status = request(
            &mut owner,
            &mut f.app,
            &socket,
            Command::Plugin(PluginCommand::Status { run_id }),
        );
        let Outcome::Result(CommandResult::Plugin(PluginResult::Status(status))) = status else {
            panic!("{status:?}")
        };
        if !matches!(status.phase, RunPhase::Running) {
            assert_eq!(status.output.as_deref(), Some("agent source"));
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    for (enabled, source, path, completed) in [
        (false, "set_automation_write_enabled(true)", "", false),
        (
            true,
            "create_note(input, \"agent write\").id",
            "agent-allowed.md",
            true,
        ),
        (
            false,
            "create_note(input, \"denied\").id",
            "agent-denied.md",
            false,
        ),
    ] {
        f.app
            .execute(
                Command::SetAutomationWriteEnabled { enabled },
                Caller::Local,
            )
            .unwrap();
        f.app.flush().unwrap();
        let started = request(
            &mut owner,
            &mut f.app,
            &socket,
            Command::Plugin(PluginCommand::Evaluate {
                id: "rhai".into(),
                source: source.into(),
                input: path.into(),
            }),
        );
        let Outcome::Result(CommandResult::Plugin(PluginResult::Started { run_id })) = started
        else {
            panic!("{started:?}")
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status = request(
                &mut owner,
                &mut f.app,
                &socket,
                Command::Plugin(PluginCommand::Status { run_id }),
            );
            let Outcome::Result(CommandResult::Plugin(PluginResult::Status(status))) = status
            else {
                panic!("{status:?}")
            };
            if !matches!(status.phase, RunPhase::Running) {
                assert_eq!(
                    matches!(status.phase, RunPhase::Completed),
                    completed,
                    "{status:?}"
                );
                if completed {
                    assert_eq!(status.receipts[0].completion, "persisted");
                }
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(
        f.app
            .notes()
            .iter()
            .any(|note| note.note.path == "agent-allowed.md")
    );
    assert!(
        !f.app
            .notes()
            .iter()
            .any(|note| note.note.path == "agent-denied.md")
    );
    let local_admin = f.start("set_automation_write_enabled(true)", "", Caller::Local);
    let status = f.finish(local_admin, Caller::Local);
    assert!(matches!(status.phase, RunPhase::Completed), "{status:?}");
    assert_eq!(f.app.automation_write_preference(), Some(true));
}
