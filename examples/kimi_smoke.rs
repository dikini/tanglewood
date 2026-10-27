//! Disposable Kimi ACP acceptance harness. No model prompt is sent without --live.
#[cfg(all(target_os = "linux", feature = "kimi"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a != "--live") || args.len() > 1 {
        return Err("usage: kimi_smoke [--live]".into());
    }
    let live = args.first().is_some_and(|a| a == "--live");
    let directory =
        std::env::temp_dir().join(format!("tanglewood-kimi-smoke-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory)?;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    let result = run(&directory, live);
    if result.is_ok() {
        std::fs::remove_dir_all(&directory)?;
    } else {
        eprintln!("Retained disposable diagnostics at {}", directory.display());
    }
    result
}
#[cfg(all(target_os = "linux", feature = "kimi"))]
fn run(directory: &std::path::Path, live: bool) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use tanglewood::{
        App,
        commands::{Caller, Command, CommandResult},
        kimi::{ConfigOption, KimiAdapter, KimiConfig, KimiEvent, KimiRequest},
        mcp::{AccessPolicy, OwnerSocket},
    };
    struct Runner {
        app: Option<App>,
        owner: Option<OwnerSocket>,
        adapter: KimiAdapter,
        session: Option<String>,
        turning: bool,
    }
    impl Runner {
        fn next(&mut self, deadline: Instant) -> Result<KimiEvent, Box<dyn std::error::Error>> {
            loop {
                self.owner
                    .as_mut()
                    .unwrap()
                    .poll(self.app.as_mut().unwrap());
                if let Some(event) = self.adapter.try_recv() {
                    return Ok(event);
                }
                if Instant::now() >= deadline {
                    return Err("Kimi smoke timed out".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn set(
            &mut self,
            id: String,
            value: String,
        ) -> Result<Vec<ConfigOption>, Box<dyn std::error::Error>> {
            self.adapter
                .try_send(KimiRequest::SetOption { id, value })?;
            let deadline = Instant::now() + Duration::from_secs(40);
            loop {
                match self.next(deadline)? {
                    KimiEvent::Options {
                        session,
                        options,
                        settled: true,
                    } if self.session.as_ref() == Some(&session) => {
                        return Ok(options);
                    }
                    KimiEvent::OptionFailed { error, .. }
                    | KimiEvent::Error(error)
                    | KimiEvent::Disconnected(error) => return Err(error.into()),
                    _ => {}
                }
            }
        }
        fn cleanup(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            let Some(session) = self.session.clone() else {
                return Ok(());
            };
            if self.turning {
                let _ = self.adapter.try_send(KimiRequest::Cancel);
                let deadline = Instant::now() + Duration::from_secs(20);
                loop {
                    match self.next(deadline)? {
                        KimiEvent::TurnFinished { .. } | KimiEvent::TurnFailed { .. } => {
                            self.turning = false;
                            break;
                        }
                        KimiEvent::Disconnected(e) => return Err(e.into()),
                        _ => {}
                    }
                }
            }
            self.adapter
                .try_send(KimiRequest::DeleteCreatedSession(session.clone()))?;
            let deadline = Instant::now() + Duration::from_secs(40);
            loop {
                match self.next(deadline)? {
                    KimiEvent::Deleted { session: id } if id == session => {
                        println!("PASS closed and deleted disposable session {session}");
                        self.session = None;
                        return Ok(());
                    }
                    KimiEvent::Error(e) | KimiEvent::Disconnected(e) => return Err(e.into()),
                    _ => {}
                }
            }
        }
    }
    let context = directory.join("context");
    std::fs::create_dir(&context)?;
    let context = std::fs::canonicalize(context)?;
    let socket = directory.join("owner.sock");
    let vault = directory.join("vault");
    let mut app = App::open(&vault)?;
    let CommandResult::Note(note) = app.execute(
        Command::CreateNote {
            path: "smoke.md".into(),
            text: "before".into(),
        },
        Caller::Local,
    )?
    else {
        return Err("temporary note creation failed".into());
    };
    app.flush()?;
    let owner = OwnerSocket::bind(&socket, AccessPolicy::ReadWrite)?;
    let executable = std::env::current_exe()?;
    let bridge = executable
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("example path has no target directory")?
        .join("tanglewood-mcp");
    if !bridge.is_file() {
        return Err("build tanglewood-mcp in the same target directory first".into());
    }
    let adapter = KimiAdapter::spawn(
        KimiConfig {
            executable: std::env::var_os("TANGLEWOOD_KIMI_SMOKE_EXECUTABLE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "kimi".into()),
            cwd: context,
            mcp_bridge: bridge,
            socket: socket.clone(),
            resume_session: None,
        },
        || {},
    )?;
    let mut runner = Runner {
        app: Some(app),
        owner: Some(owner),
        adapter,
        session: None,
        turning: false,
    };
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            match runner.next(deadline)? {
                KimiEvent::Connected { name, version } => {
                    println!("PASS initialize {name} {version}");
                    break;
                }
                KimiEvent::Error(e) | KimiEvent::Disconnected(e) => return Err(e.into()),
                _ => {}
            }
        }
        runner.adapter.try_send(KimiRequest::NewSession)?;
        let deadline = Instant::now() + Duration::from_secs(40);
        let mut options = loop {
            match runner.next(deadline)? {
                KimiEvent::Opened {
                    session, options, ..
                } => {
                    println!("Created disposable session {session}");
                    runner.session = Some(session);
                    break options;
                }
                KimiEvent::OpenFailed { error, .. }
                | KimiEvent::Error(error)
                | KimiEvent::Disconnected(error) => return Err(error.into()),
                _ => {}
            }
        };
        let model = options
            .iter()
            .find(|o| o.category.as_deref() == Some("model"))
            .cloned();
        if let Some(model) = model {
            let original = model.current.clone();
            let alternate = model.values().find(|v| v.value != original).cloned();
            if let Some(alternate) = alternate {
                options = runner.set(model.id.clone(), alternate.value.clone())?;
                println!("PASS set advertised model {}", alternate.name);
                if let Some(thinking) = options
                    .iter()
                    .find(|o| o.category.as_deref() == Some("thought_level"))
                {
                    println!(
                        "Alternate model thinking choices: {}",
                        thinking
                            .values()
                            .map(|v| v.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                } else {
                    println!("Alternate model reports no thinking selector");
                }
                options = runner.set(model.id, original.clone())?;
                if !options
                    .iter()
                    .any(|o| o.category.as_deref() == Some("model") && o.current == original)
                {
                    return Err("original model was not restored".into());
                }
                println!("PASS restored original model from confirmed snapshot");
            } else {
                println!("No alternate advertised model; model-change check unavailable");
            }
        } else {
            println!("No model selector advertised");
        }
        if !live {
            return Ok(());
        }
        let prompt = format!(
            "Disposable acceptance test. Use only the tanglewood MCP tools. ReadNote for note ID {}. Then call EditNote with the revision just read and replace bytes 0..6 with after. ReadNote again and reply briefly. Do not run shell commands or write files directly.",
            note.note.id.0
        );
        runner.adapter.try_send(KimiRequest::Prompt {
            session: runner.session.clone().ok_or("missing session")?,
            submission: uuid::Uuid::new_v4().to_string(),
            text: prompt,
            attachments: Vec::new(),
        })?;
        runner.turning = true;
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut deltas = 0;
        let mut tools = 0;
        let mut permissions = 0;
        loop {
            match runner.next(deadline)? {
                KimiEvent::Delta { .. } => deltas += 1,
                KimiEvent::Activity { .. } => tools += 1,
                KimiEvent::Permission {
                    id,
                    details,
                    options,
                    ..
                } => {
                    permissions += 1;
                    let allowed = details.contains(&note.note.id.0.to_string())
                        && details.to_lowercase().contains("tanglewood");
                    let choice = if allowed {
                        options
                            .iter()
                            .find(|o| o.kind == "allow_once")
                            .map(|o| o.id.clone())
                    } else {
                        None
                    };
                    runner
                        .adapter
                        .try_send(KimiRequest::AnswerPermission { id, option: choice })?;
                }
                KimiEvent::TurnFinished { status, .. } => {
                    runner.turning = false;
                    if status != "end_turn" {
                        return Err(format!("first turn ended {status}").into());
                    }
                    break;
                }
                KimiEvent::TurnFailed { error, .. }
                | KimiEvent::Error(error)
                | KimiEvent::Disconnected(error) => return Err(error.into()),
                _ => {}
            }
        }
        println!(
            "Observed first-turn stream chunks={deltas}, tool events={tools}, permission requests={permissions}"
        );
        if deltas == 0 {
            return Err("no streamed response observed".into());
        }
        let current = runner.app.as_mut().unwrap().note(note.note.id)?;
        if current.note.text != "after" || current.save.persisted != Some(current.note.revision) {
            return Err("expected durable temporary note edit was not confirmed".into());
        }
        runner.owner.take();
        runner.app.as_mut().unwrap().shutdown()?;
        runner.app.take();
        let mut reopened = App::open(&vault)?;
        if reopened.note(note.note.id)?.note.text != "after" {
            return Err("temporary edit did not survive restart".into());
        }
        reopened.execute(
            Command::SetAutomationWriteEnabled { enabled: false },
            Caller::Local,
        )?;
        runner.owner = Some(OwnerSocket::bind(&socket, AccessPolicy::ReadWrite)?);
        runner.app = Some(reopened);
        println!("PASS temporary note survived restart; owner writes revoked");
        runner.adapter.try_send(KimiRequest::Prompt{session:runner.session.clone().ok_or("missing session")?,submission:uuid::Uuid::new_v4().to_string(),text:format!("Attempt once to edit temporary note {} through the tanglewood MCP execute tool, then report the result. Do not use shell or write files directly.",note.note.id.0),attachments:Vec::new()})?;
        runner.turning = true;
        let deadline = Instant::now() + Duration::from_secs(45);
        let stop_after = Instant::now() + Duration::from_secs(3);
        let mut stopped = false;
        let mut denied = false;
        loop {
            if !stopped && Instant::now() >= stop_after {
                runner.adapter.try_send(KimiRequest::Cancel)?;
                stopped = true;
            }
            let event = match runner.next(if stopped { deadline } else { stop_after }) {
                Ok(event) => event,
                Err(_) if !stopped && Instant::now() >= stop_after => continue,
                Err(error) => return Err(error),
            };
            match event {
                KimiEvent::Activity { activity, .. } => {
                    if activity.details.to_lowercase().contains("unauthoriz")
                        || activity.status == "failed"
                    {
                        denied = true;
                    }
                }
                KimiEvent::Permission { id, .. } => {
                    runner
                        .adapter
                        .try_send(KimiRequest::AnswerPermission { id, option: None })?;
                }
                KimiEvent::TurnFinished { status, .. } => {
                    runner.turning = false;
                    println!(
                        "Second turn status={status}; stop requested={stopped}; rejected tool event observed={denied}"
                    );
                    break;
                }
                KimiEvent::TurnFailed { error, .. } => {
                    runner.turning = false;
                    println!(
                        "Second turn failed: {error}; stop requested={stopped}; rejected tool event observed={denied}"
                    );
                    break;
                }
                KimiEvent::Error(error) | KimiEvent::Disconnected(error) => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        if runner.app.as_mut().unwrap().note(note.note.id)?.note.text != "after" {
            return Err("revoked owner allowed the note to change".into());
        }
        if !denied {
            println!(
                "LIMITATION: the live session did not expose a rejected mutation event; owner revocation remains covered by the concurrent bridge regression."
            );
        }
        Ok(())
    })();
    let cleanup = runner.cleanup();
    if let Err(error) = &cleanup {
        eprintln!(
            "Could not clean disposable Kimi session {:?}: {error}",
            runner.session
        );
    }
    let _ = runner.adapter.try_send(KimiRequest::Disconnect);
    drop(runner.owner.take());
    if let Some(app) = &mut runner.app {
        let _ = app.shutdown();
    }
    result?;
    cleanup?;
    Ok(())
}
#[cfg(not(all(target_os = "linux", feature = "kimi")))]
fn main() {
    eprintln!("kimi_smoke requires Linux and --features kimi");
}
