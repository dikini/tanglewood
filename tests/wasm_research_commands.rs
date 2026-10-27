#![cfg(all(feature = "wasm-extensions", target_os = "linux"))]

use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tanglewood::{
    App,
    commands::{Caller, Command, CommandError, CommandResult},
    plugin_jobs::{PluginCommand, PluginResult, RunPhase, RunStatus},
    plugin_permissions::{Action, Origin, Scope},
    research::{Acquisition, AcquisitionStatus, ArtifactInput, ResearchCommand, ResearchWorkspace},
};
use uuid::Uuid;

struct Demo(PathBuf);
impl Drop for Demo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_manifest(bundle: &Path, digest: &str, imports: Vec<serde_json::Value>, wrong: &Path) {
    let mut rules = imports;
    rules.extend([
        serde_json::json!({
            "action": "research.acquire_source", "workspaces": {"mode": "all"},
            "filesystem_roots": {"mode": "all"}
        }),
        // Both rules match the workspace; only the second matches the saved input.
        serde_json::json!({
            "action": "research.retry_acquisition", "workspaces": {"mode": "all"},
            "filesystem_roots": {"mode": "only", "values": [wrong]}
        }),
        serde_json::json!({
            "action": "research.retry_acquisition", "workspaces": {"mode": "all"},
            "filesystem_roots": {"mode": "all"}
        }),
    ]);
    let manifest = serde_json::json!({
        "manifest_version": 1, "id": "rhai", "version": "1", "kind": "interpreter",
        "languages": ["rhai"], "wit_version": "0.2.0", "component": "component.wasm",
        "component_sha256": digest,
        "invocation": {"local": true, "local_source": true},
        "permissions": {"rules": rules}
    });
    fs::write(
        bundle.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}

fn import_rule(root: Option<&Path>) -> serde_json::Value {
    let scope = match root {
        Some(root) => serde_json::json!({"mode": "only", "values": [root]}),
        None => serde_json::json!({"mode": "all"}),
    };
    serde_json::json!({
        "action": "research.import_source", "workspaces": {"mode": "all"},
        "filesystem_roots": scope
    })
}

fn render(template: &str, values: &[(&str, String)]) -> String {
    let mut source = template.to_owned();
    for (name, value) in values {
        source = source.replace(name, &serde_json::to_string(value).unwrap());
    }
    source
}

fn start(app: &mut App, source: String) -> Uuid {
    match app
        .execute(
            Command::Plugin(PluginCommand::Evaluate {
                id: "rhai".into(),
                source,
                input: String::new(),
            }),
            Caller::Local,
        )
        .unwrap()
    {
        CommandResult::Plugin(PluginResult::Started { run_id }) => run_id,
        other => panic!("unexpected plugin start: {other:?}"),
    }
}

fn finish(app: &mut App, run_id: Uuid) -> RunStatus {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        app.tick();
        let result = app
            .execute(
                Command::Plugin(PluginCommand::Status { run_id }),
                Caller::Local,
            )
            .unwrap();
        let CommandResult::Plugin(PluginResult::Status(status)) = result else {
            panic!("unexpected plugin status: {result:?}")
        };
        if !matches!(status.phase, RunPhase::Running) {
            return status;
        }
        assert!(Instant::now() < deadline, "plugin did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn job_for_path(workspace: &ResearchWorkspace, path: &Path) -> Option<Acquisition> {
    workspace
        .acquisitions
        .iter()
        .find(|job| matches!(&job.input, ArtifactInput::LocalFile { path: value } if Path::new(value) == path))
        .cloned()
}

fn queued_before_worker(
    app: &mut App,
    run_id: Uuid,
    vault: &Path,
    workspace_id: Uuid,
    path: &Path,
) -> Acquisition {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        app.tick();
        if let Some(workspace) = app.research_workspace(workspace_id) {
            if let Some(job) = job_for_path(workspace, path) {
                assert_eq!(job.status, AcquisitionStatus::Queued);
                assert!(job.authority.is_some(), "queued job lacks authority");
                // The owner starts a worker only on a later tick after the
                // running transition is stored. Inspect the committed payload now.
                let db = rusqlite::Connection::open_with_flags(
                    vault.join("notes.sqlite3"),
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .unwrap();
                let payload: String = db
                    .query_row(
                        "SELECT payload FROM research_workspaces WHERE id = ?1",
                        [workspace_id.to_string()],
                        |row| row.get(0),
                    )
                    .unwrap();
                let saved: ResearchWorkspace = serde_json::from_str(&payload).unwrap();
                let stored = job_for_path(&saved, path).expect("queued job persisted");
                assert_eq!(
                    serde_json::to_value(&stored.authority).unwrap(),
                    serde_json::to_value(&job.authority).unwrap(),
                    "committed authority differs from the queued job"
                );
                assert!(saved.artifacts.is_empty());
                return job;
            }
        }
        let status = app
            .execute(
                Command::Plugin(PluginCommand::Status { run_id }),
                Caller::Local,
            )
            .unwrap();
        if let CommandResult::Plugin(PluginResult::Status(status)) = status {
            assert!(
                matches!(status.phase, RunPhase::Running),
                "plugin ended before queueing a job: {status:?}"
            );
        }
        assert!(Instant::now() < deadline, "job was not queued");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn wait_failed(app: &mut App, workspace_id: Uuid, job_id: Uuid) -> Acquisition {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        app.tick();
        let workspace = app.research_workspace(workspace_id).unwrap();
        if let Some(job) = workspace.acquisitions.iter().find(|job| job.id == job_id) {
            if job.status == AcquisitionStatus::Failed && !app.research_pending(workspace_id) {
                return job.clone();
            }
        }
        assert!(Instant::now() < deadline, "research job did not fail");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn queued_research_preserves_and_rechecks_plugin_authority() {
    let demo = Demo(std::env::temp_dir().join(format!("tw-research-plugin-{}", Uuid::new_v4())));
    let bundle = demo.0.join("registry/rhai");
    let allowed = demo.0.join("allowed");
    let wrong = demo.0.join("other");
    let vault = demo.0.join("vault");
    fs::create_dir_all(&bundle).unwrap();
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&wrong).unwrap();
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/wasm-extensions/wasm32-wasip2/release/tanglewood_rhai_v2_guest.wasm");
    let bytes = fs::read(&component).unwrap_or_else(|_| {
        panic!(
            "missing {}: run ./scripts/build-wasm-extensions.sh first",
            component.display()
        )
    });
    let digest = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    fs::write(bundle.join("component.wasm"), bytes).unwrap();
    // Keep an explicit all-root rule after the narrow original grant.
    write_manifest(
        &bundle,
        &digest,
        vec![import_rule(Some(&allowed)), import_rule(None)],
        &wrong,
    );
    let mut app = App::open(&vault).unwrap();
    app.configure_plugin_registry(&demo.0.join("registry"), &["rhai".into()])
        .unwrap();
    let CommandResult::Accepted { request_id } = app
        .execute(
            Command::Research(ResearchCommand::Create {
                title: "authority".into(),
                question: String::new(),
            }),
            Caller::Local,
        )
        .unwrap()
    else {
        panic!("research create did not return a request")
    };
    app.flush().unwrap();
    let workspace_id = app
        .research_created_workspace(request_id)
        .expect("created workspace");
    let missing = allowed.join("missing.md");
    let revision = app.research_workspace(workspace_id).unwrap().revision.0;
    let import = render(
        r#"let reply = command(#{Research: #{ImportSource: #{id: WORKSPACE, expected: REVISION, path: PATH}}}); if "Err" in reply {reply.Err.to_string()} else {"ok"}"#,
        &[
            ("WORKSPACE", workspace_id.to_string()),
            ("REVISION", revision.to_string()),
            ("PATH", missing.to_str().unwrap().into()),
        ],
    );
    let run = start(&mut app, import);
    let queued = queued_before_worker(&mut app, run, &vault, workspace_id, &missing);
    let authority = queued.authority.as_ref().unwrap();
    assert_eq!(authority.plugin_id, "rhai");
    assert_eq!(authority.component_sha256, digest);
    assert_eq!(authority.origin, Origin::Local);
    assert!(
        authority
            .guards
            .iter()
            .any(|guard| guard.rule.action == Action::ResearchImportSource
                && guard.rule.filesystem_roots == Some(Scope::Only(vec![allowed.clone()])))
    );
    let status = finish(&mut app, run);
    assert!(matches!(status.phase, RunPhase::Completed), "{status:?}");
    assert_eq!(status.output.as_deref(), Some("ok"));
    assert!(
        status
            .receipts
            .iter()
            .any(|receipt| receipt.command == "research.import_source"
                && receipt.completion == "persisted")
    );
    let failed = wait_failed(&mut app, workspace_id, queued.id);
    assert!(failed.error.is_some());

    // AcquireSource independently queues and persists authority for the same source.
    let second = allowed.join("also-missing.md");
    let revision = app.research_workspace(workspace_id).unwrap().revision.0;
    let acquire = render(
        r#"let reply = command(#{Research: #{AcquireSource: #{id: WORKSPACE, expected: REVISION, source_id: SOURCE, input: #{LocalFile: #{path: PATH}}, ocr: false}}}); if "Err" in reply {reply.Err.to_string()} else {"ok"}"#,
        &[
            ("WORKSPACE", workspace_id.to_string()),
            ("REVISION", revision.to_string()),
            ("SOURCE", queued.source_id.to_string()),
            ("PATH", second.to_str().unwrap().into()),
        ],
    );
    let run = start(&mut app, acquire);
    let queued_acquire = queued_before_worker(&mut app, run, &vault, workspace_id, &second);
    assert!(
        queued_acquire
            .authority
            .as_ref()
            .unwrap()
            .guards
            .iter()
            .any(|guard| guard.rule.action == Action::ResearchAcquireSource)
    );
    let status = finish(&mut app, run);
    assert!(matches!(status.phase, RunPhase::Completed), "{status:?}");
    assert_eq!(status.output.as_deref(), Some("ok"));
    wait_failed(&mut app, workspace_id, queued_acquire.id);

    let retry_source = |revision: Uuid| {
        render(
            r#"let reply = command(#{Research: #{RetryAcquisition: #{id: WORKSPACE, expected: REVISION, job_id: JOB}}}); if "Err" in reply {reply.Err.to_string()} else {"ok"}"#,
            &[
                ("WORKSPACE", workspace_id.to_string()),
                ("REVISION", revision.to_string()),
                ("JOB", queued.id.to_string()),
            ],
        )
    };
    // A matching retry rule cannot override a revoked original import grant.
    write_manifest(&bundle, &digest, vec![import_rule(Some(&wrong))], &wrong);
    app.reload_plugin("rhai").unwrap();
    let revision = app.research_workspace(workspace_id).unwrap().revision.0;
    let run = start(&mut app, retry_source(revision));
    let denied = finish(&mut app, run);
    assert!(matches!(denied.phase, RunPhase::Completed), "{denied:?}");
    assert!(
        denied
            .output
            .as_deref()
            .is_some_and(|error| error.contains("Unauthorized")),
        "{denied:?}"
    );
    assert_eq!(
        app.research_workspace(workspace_id)
            .unwrap()
            .acquisitions
            .iter()
            .find(|job| job.id == queued.id)
            .unwrap()
            .status,
        AcquisitionStatus::Failed
    );

    // Restore an all-root import grant. The first retry rule still mismatches
    // the file; the second must be selected without dropping the original rule.
    write_manifest(&bundle, &digest, vec![import_rule(None)], &wrong);
    app.reload_plugin("rhai").unwrap();
    let revision = app.research_workspace(workspace_id).unwrap().revision.0;
    let run = start(&mut app, retry_source(revision));
    let status = finish(&mut app, run);
    assert!(matches!(status.phase, RunPhase::Completed), "{status:?}");
    assert_eq!(status.output.as_deref(), Some("ok"));
    let retried = wait_failed(&mut app, workspace_id, queued.id);
    let guards = &retried.authority.as_ref().unwrap().guards;
    assert!(
        guards
            .iter()
            .any(|guard| guard.rule.action == Action::ResearchImportSource
                && guard.rule.filesystem_roots == Some(Scope::Only(vec![allowed.clone()])))
    );
    assert!(guards.iter().any(
        |guard| guard.rule.action == Action::ResearchRetryAcquisition
            && guard.rule.filesystem_roots == Some(Scope::All)
    ));
    assert!(!guards.iter().any(
        |guard| guard.rule.action == Action::ResearchRetryAcquisition
            && guard.rule.filesystem_roots == Some(Scope::Only(vec![wrong.clone()]))
    ));

    // Once the input exists, an unconfigured restart must not run it with Local
    // authority merely because a local caller retries the saved plugin job.
    fs::write(&missing, "not authorized after restart").unwrap();
    app.shutdown().unwrap();
    drop(app);
    let mut reopened = App::open(&vault).unwrap();
    let workspace = reopened.research_workspace(workspace_id).unwrap();
    assert!(
        workspace
            .acquisitions
            .iter()
            .find(|job| job.id == queued.id)
            .unwrap()
            .authority
            .is_some()
    );
    let original_revision = workspace.revision;
    let outcome = reopened.execute(
        Command::Research(ResearchCommand::RetryAcquisition {
            id: workspace_id,
            expected: original_revision,
            job_id: queued.id,
        }),
        Caller::Local,
    );
    match outcome {
        Err(CommandError::Unauthorized | CommandError::Unavailable(_)) => {}
        Ok(CommandResult::Accepted { .. }) => {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                reopened.tick();
                let workspace = reopened.research_workspace(workspace_id).unwrap();
                let job = workspace
                    .acquisitions
                    .iter()
                    .find(|job| job.id == queued.id)
                    .unwrap();
                if workspace.revision != original_revision
                    && job.status == AcquisitionStatus::Failed
                    && !reopened.research_pending(workspace_id)
                {
                    assert!(
                        job.error
                            .as_deref()
                            .is_some_and(|error| error.contains("authorized")),
                        "{job:?}"
                    );
                    break;
                }
                assert!(Instant::now() < deadline, "unconfigured retry did not fail");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        other => panic!("unexpected restart retry outcome: {other:?}"),
    }
    let workspace = reopened.research_workspace(workspace_id).unwrap();
    assert!(workspace.artifacts.is_empty());
    assert!(
        workspace
            .sources
            .iter()
            .find(|source| source.id == queued.source_id)
            .unwrap()
            .paper
            .is_none()
    );
    reopened.shutdown().unwrap();
}
