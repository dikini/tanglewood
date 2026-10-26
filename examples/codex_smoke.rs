//! Opt-in live acceptance smoke. Uses only an owned temporary vault and working directory.
#[cfg(all(target_os = "linux", feature = "codex"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() != Some("--live") {
        return Err(
            "use --live to authorize two short model turns using existing Codex authentication"
                .into(),
        );
    }
    let directory =
        std::env::temp_dir().join(format!("tanglewood-codex-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    let result = live(&directory);
    if result.is_ok() {
        std::fs::remove_dir_all(&directory)?;
    } else {
        eprintln!("Retained live diagnostics in {}", directory.display());
    }
    result
}

#[cfg(all(target_os = "linux", feature = "codex"))]
fn live(directory: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use tanglewood::{
        App,
        codex::{ApprovalDecision, CodexAdapter, CodexConfig, CodexEvent, CodexRequest},
        commands::{Caller, Command, CommandResult},
        mcp::{AccessPolicy, OwnerSocket},
    };
    let cwd = directory.join("context");
    std::fs::create_dir(&cwd)?;
    let vault = directory.join("vault");
    let socket = directory.join("owner.sock");
    let mut app = App::open(&vault)?;
    let CommandResult::Note(note) = app.execute(
        Command::CreateNote {
            path: "live.md".into(),
            text: "# Live\nbefore\n".into(),
        },
        Caller::Local,
    )?
    else {
        return Err("create returned no note".into());
    };
    app.flush()?;
    let mut owner = OwnerSocket::bind(&socket, AccessPolicy::ReadWrite)?;
    let executable = std::env::current_exe()?;
    let bridge = executable
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("example executable has no target directory")?
        .join("tanglewood-mcp");
    if !bridge.is_file() {
        return Err("build tanglewood-mcp in the same target profile first".into());
    }
    let adapter = CodexAdapter::spawn(
        CodexConfig {
            executable: std::env::var_os("TANGLEWOOD_CODEX_SMOKE_EXECUTABLE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "codex".into()),
            cwd,
            mcp_bridge: bridge,
            socket,
            resume_thread: None,
        },
        || {},
    )?;
    let mut thread = None;
    let deadline = Instant::now() + Duration::from_secs(40);
    while thread.is_none() {
        owner.poll(&mut app);
        while let Some(event) = adapter.try_recv() {
            match event {
                CodexEvent::Ready { thread: id } => {
                    println!("Live thread {id}");
                    thread = Some(id);
                }
                CodexEvent::Error(error) | CodexEvent::Disconnected(error) => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            return Err("Codex readiness timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("PASS live initialize and thread start with confirmed read-only process policy");
    adapter.try_send(CodexRequest::SendPrompt(format!("Acceptance smoke in a temporary vault. Use only the Tanglewood MCP execute tool. First ReadNote for id {}. Then EditNote using the revision just read, replacing byte range 7..13 with the exact text after. Finally read it again and reply briefly. Do not run shell commands or write filesystem files.", note.note.id.0)))?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut complete = false;
    let mut deltas = 0;
    let mut approved_tools = 0;
    while !complete {
        owner.poll(&mut app);
        while let Some(event) = adapter.try_recv() {
            match event {
                CodexEvent::Delta { .. } => deltas += 1,
                CodexEvent::MessageComplete { text, .. } => eprintln!("Live message: {text}"),
                CodexEvent::TurnCompleted { status, .. } => {
                    if status != "completed" {
                        return Err(format!("live turn status {status}").into());
                    }
                    complete = true;
                }
                CodexEvent::Approval {
                    id,
                    method,
                    details,
                    ..
                } => {
                    eprintln!("Observed live approval method={method}, details={details}");
                    let command = details
                        .pointer("/_meta/tool_params/command")
                        .cloned()
                        .and_then(|value| serde_json::from_value::<Command>(value).ok());
                    let allowed = match command {
                        Some(Command::ReadNote { id }) => id == note.note.id,
                        Some(Command::EditNote {
                            id,
                            expected,
                            edits,
                        }) => {
                            id == note.note.id
                                && app
                                    .note(id)
                                    .is_ok_and(|view| view.note.revision == expected)
                                && edits
                                    == vec![tanglewood::notes::TextEdit {
                                        start: 7,
                                        end: 13,
                                        replacement: "after".into(),
                                    }]
                        }
                        _ => false,
                    } && method == "mcpServer/elicitation/request"
                        && details
                            .get("serverName")
                            .and_then(serde_json::Value::as_str)
                            == Some("tanglewood")
                        && approved_tools < 4;
                    if allowed {
                        approved_tools += 1;
                    }
                    adapter.try_send(CodexRequest::AnswerApproval {
                        id,
                        decision: if allowed {
                            ApprovalDecision::Accept
                        } else {
                            ApprovalDecision::Decline
                        },
                    })?
                }
                CodexEvent::Error(error) | CodexEvent::Disconnected(error) => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            return Err("live tool turn timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let edited = app.note(note.note.id)?;
    if edited.note.text != "# Live\nafter\n"
        || edited.save.persisted != Some(edited.note.revision)
        || deltas == 0
    {
        eprintln!(
            "Live outcome deltas={deltas}, source={:?}, save={:?}",
            edited.note.text, edited.save
        );
        return Err("live turn did not stream and durably edit exact requested source".into());
    }
    println!(
        "PASS streamed live turn and revision-checked MCP read/edit/read with durable source ({approved_tools} individually scoped approvals)"
    );
    adapter.try_send(CodexRequest::SendPrompt(
        "Use no tools. Count integers from 1 to 100000, one per line.".into(),
    ))?;
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut interrupt = false;
    let mut interrupted = false;
    while !interrupted {
        owner.poll(&mut app);
        while let Some(event) = adapter.try_recv() {
            match event {
                CodexEvent::TurnStarted { .. } if !interrupt => {
                    adapter.try_send(CodexRequest::Interrupt)?;
                    interrupt = true;
                }
                CodexEvent::TurnCompleted { status, .. } => {
                    if status != "interrupted" {
                        return Err(format!("cancel status {status}").into());
                    }
                    interrupted = true;
                }
                CodexEvent::Error(error) | CodexEvent::Disconnected(error) => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        if Instant::now() >= deadline {
            return Err("live cancellation timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("PASS live turn cancellation");
    adapter.try_send(CodexRequest::Disconnect)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        owner.poll(&mut app);
        if matches!(adapter.try_recv(), Some(CodexEvent::Disconnected(_))) {
            break;
        }
        if Instant::now() >= deadline {
            return Err("disconnect teardown timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(adapter);
    app.execute(
        Command::CreateNote {
            path: "still-editable.md".into(),
            text: "Editor commands remain usable after disconnect.\n".into(),
        },
        Caller::Local,
    )?;
    app.flush()?;
    drop(owner);
    app.shutdown()?;
    let mut restarted = App::open(&vault)?;
    if restarted.note(note.note.id)?.note.text != "# Live\nafter\n" {
        return Err("live tool save did not survive restart".into());
    }
    restarted.shutdown()?;
    println!("PASS disconnect leaves editing/saving usable and live tool edit survives restart");
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "codex")))]
fn main() {
    eprintln!("This live smoke requires Linux and --features codex");
    std::process::exit(1);
}
