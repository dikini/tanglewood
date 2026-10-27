//! Opt-in live regression: one short plan/read turn in an owned temporary vault.
#[cfg(all(target_os = "linux", feature = "codex"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    use tanglewood::{
        App,
        codex::{
            ApprovalDecision, ApprovalMode, CodexAdapter, CodexConfig, CodexEvent, CodexRequest,
            TurnControls, WorkMode,
        },
        commands::{Caller, Command, CommandResult},
        mcp::{AccessPolicy, OwnerSocket},
    };
    if std::env::args().nth(1).as_deref() != Some("--live") {
        return Err(
            "use --live for one short authenticated model turn in a disposable vault".into(),
        );
    }
    let directory =
        std::env::temp_dir().join(format!("tanglewood-plan-smoke-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory)?;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    println!("Diagnostics: {}", directory.display());
    let cwd = directory.join("context");
    std::fs::create_dir(&cwd)?;
    let mut app = App::open(&directory.join("vault"))?;
    let CommandResult::Note(seed) = app.execute(
        Command::CreateNote {
            path: "Seed.md".into(),
            text: "# Seed\nPLAN-SMOKE-SEED\n".into(),
        },
        Caller::Local,
    )?
    else {
        return Err("seed creation failed".into());
    };
    app.flush()?;
    let socket = directory.join("owner.sock");
    let mut owner = OwnerSocket::bind(&socket, AccessPolicy::ReadWrite)?;
    let bridge = std::env::current_exe()?
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("example path")?
        .join("tanglewood-mcp");
    let adapter = CodexAdapter::spawn(
        CodexConfig {
            executable: "codex".into(),
            cwd,
            mcp_bridge: bridge,
            socket,
            resume_thread: None,
            project: None,
        },
        || {},
    )?;
    let mut model = None;
    let mut effort = None;
    let mut choices = Vec::new();
    let mut ready = false;
    let deadline = Instant::now() + Duration::from_secs(45);
    while !ready
        || model.as_ref().is_none_or(|id| {
            !choices
                .iter()
                .any(|m: &tanglewood::codex::ModelChoice| &m.model == id)
        })
    {
        owner.poll(&mut app);
        while let Some(event) = adapter.try_recv() {
            match event {
                CodexEvent::Ready { thread } => {
                    std::fs::write(directory.join("thread-id"), &thread)?;
                    println!("Owned thread: {thread}");
                    ready = true;
                    adapter.try_send(CodexRequest::ListModels { cursor: None })?;
                }
                CodexEvent::ThreadModel {
                    model: id,
                    effort: value,
                    ..
                } => {
                    model = Some(id);
                    effort = value;
                }
                CodexEvent::Models {
                    models,
                    next_cursor,
                } => {
                    choices.extend(models);
                    if let Some(cursor) = next_cursor {
                        adapter.try_send(CodexRequest::ListModels {
                            cursor: Some(cursor),
                        })?;
                    }
                }
                CodexEvent::Error(error) | CodexEvent::Disconnected(error) => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        if Instant::now() > deadline {
            return Err("model/readiness deadline".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let prompt = format!(
        "This is a bounded integration check, not an open-ended task. Use the actual update_plan tool to track exactly three steps: list the vault notes; read Seed.md; report its last nonempty line. Use the tanglewood read tool for SearchNotes (empty query) and ReadNote for id {}. Mark each plan step completed through update_plan. Do not repeat the numbered plan in assistant messages. Final response: only the note's last nonempty line. Do not run commands, access files directly, use network, or modify notes.",
        seed.note.id.0
    );
    adapter.try_send(CodexRequest::SendPromptWithControls {
        text: prompt,
        model,
        effort,
        attachments: vec![],
        skills: vec![],
        controls: TurnControls {
            mode: WorkMode::Work,
            approval: ApprovalMode::NoPrompts,
            network_access: false,
        },
    })?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut plans = 0;
    let mut completed_plan = false;
    let mut reads = 0;
    let mut approvals = 0;
    let mut numbered_messages = 0;
    let mut reply = String::new();
    let mut done = false;
    while !done {
        owner.poll(&mut app);
        while let Some(event) = adapter.try_recv() {
            match event {
                CodexEvent::Plan { steps, .. } if !steps.is_empty() => {
                    plans += 1;
                    completed_plan =
                        steps.len() == 3 && steps.iter().all(|s| s.status == "completed");
                }
                CodexEvent::Activity { activity, .. }
                    if activity.kind == "mcpToolCall"
                        && activity.status == "completed"
                        && activity.title.contains("read") =>
                {
                    reads += 1;
                }
                CodexEvent::MessageComplete { text, .. } => {
                    if text.lines().any(|l| l.trim_start().starts_with("1.")) {
                        numbered_messages += 1;
                    }
                    reply = text;
                }
                CodexEvent::Approval { id, .. } => {
                    approvals += 1;
                    adapter.try_send(CodexRequest::AnswerApproval {
                        id,
                        decision: ApprovalDecision::Decline,
                    })?;
                }
                CodexEvent::TurnCompleted { status, .. } => {
                    if status != "completed" {
                        return Err(format!("turn ended {status}").into());
                    }
                    done = true;
                }
                CodexEvent::TurnRejected { error, .. }
                | CodexEvent::Error(error)
                | CodexEvent::Disconnected(error) => return Err(error.into()),
                _ => {}
            }
        }
        if Instant::now() > deadline {
            let _ = adapter.try_send(CodexRequest::Interrupt);
            return Err("live plan deadline; diagnostic thread-id retained for cleanup".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    adapter.try_send(CodexRequest::Disconnect)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    'disconnect: loop {
        while let Some(event) = adapter.try_recv() {
            if matches!(event, CodexEvent::Disconnected(_)) {
                break 'disconnect;
            }
        }
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let unchanged = app.note(seed.note.id)?.note.text == seed.note.text && app.notes().len() == 1;
    app.shutdown()?;
    println!(
        "plans={plans}; completed_plan={completed_plan}; read_calls={reads}; approvals={approvals}; numbered_progress_messages={numbered_messages}; source_unchanged={unchanged}"
    );
    if plans < 2
        || !completed_plan
        || reads < 2
        || approvals != 0
        || numbered_messages != 0
        || !reply.contains("PLAN-SMOKE-SEED")
        || !unchanged
    {
        return Err(
            "live structured-plan/read regression failed; inspect retained diagnostics".into(),
        );
    }
    println!("PASS. Retained thread-id for explicit scoped remote cleanup; no user vault touched.");
    Ok(())
}
#[cfg(not(all(target_os = "linux", feature = "codex")))]
fn main() {
    eprintln!("requires Linux --features codex");
}
