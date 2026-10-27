use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("Tanglewood: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut vault = None;
    let mut headless = false;
    let mut wasm = WasmOptions::default();
    let mut plugins = PluginOptions::default();
    let mut startup = Vec::new();
    let mut mcp_path = None;
    let mut mcp_write = false;
    let mut codex_enabled = false;
    let mut codex_cwd = None;
    let mut codex_resume = None;
    let mut kimi_enabled = false;
    let mut kimi_cwd = None;
    let mut kimi_resume = None;
    let mut lan_requested = false;
    let mut lan_serve = false;
    let mut lan_listen_requested = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vault" => {
                vault = Some(PathBuf::from(
                    args.next().ok_or("--vault needs a directory")?,
                ))
            }
            "--headless" => headless = true,
            "--wasm-component" => {
                wasm.component = Some(PathBuf::from(
                    args.next().ok_or("--wasm-component needs a path")?,
                ));
            }
            "--wasm-script" => {
                wasm.script = Some(PathBuf::from(
                    args.next().ok_or("--wasm-script needs a path")?,
                ));
            }
            "--wasm-input" => {
                wasm.input = Some(args.next().ok_or("--wasm-input needs a string")?);
            }
            "--wasm-note" => {
                wasm.note = Some(tanglewood::notes::NoteId(uuid::Uuid::parse_str(
                    &args.next().ok_or("--wasm-note needs a note UUID")?,
                )?));
            }
            "--plugin-registry" => {
                plugins.registry = Some(PathBuf::from(
                    args.next().ok_or("--plugin-registry needs a directory")?,
                ));
            }
            "--plugin" => plugins.ids.push(args.next().ok_or("--plugin needs an ID")?),
            "--plugin-run" | "--plugin-evaluate" => {
                let id = args.next().ok_or("plugin invocation needs an ID")?;
                let invocation = if arg == "--plugin-run" {
                    PluginInvocation::Run(id)
                } else {
                    PluginInvocation::Evaluate(id)
                };
                if plugins.invocation.replace(invocation).is_some() {
                    return Err("choose exactly one plugin invocation".into());
                }
            }
            "--mcp-socket" => {
                mcp_path = Some(PathBuf::from(
                    args.next().ok_or("--mcp-socket needs a path")?,
                ))
            }
            "--mcp-write" => mcp_write = true,
            "--codex" => codex_enabled = true,
            "--codex-cwd" => {
                codex_cwd = Some(PathBuf::from(
                    args.next().ok_or("--codex-cwd needs a directory")?,
                ))
            }
            "--codex-resume" => {
                codex_resume = Some(args.next().ok_or("--codex-resume needs a thread ID")?)
            }
            "--kimi" => kimi_enabled = true,
            "--kimi-cwd" => {
                kimi_cwd = Some(PathBuf::from(
                    args.next().ok_or("--kimi-cwd needs a directory")?,
                ))
            }
            "--kimi-resume" => {
                kimi_resume = Some(args.next().ok_or("--kimi-resume needs a session ID")?)
            }
            "--lan-listen" => {
                lan_requested = true;
                lan_listen_requested = true;
                startup.push(tanglewood::commands::Command::ListenLan {
                    bind: args.next().ok_or("--lan-listen needs ADDRESS:PORT")?,
                    advertised_ip: None,
                });
            }
            "--lan-advertise" => {
                let ip = args.next().ok_or("--lan-advertise needs an IP address")?;
                let Some(tanglewood::commands::Command::ListenLan { advertised_ip, .. }) =
                    startup.last_mut()
                else {
                    return Err(
                        "--lan-advertise must immediately follow --lan-listen ADDRESS:PORT".into(),
                    );
                };
                *advertised_ip = Some(ip);
            }
            "--lan-ticket" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::GetLanTicket);
            }
            "--prepare-join" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::PrepareJoin {
                    ticket: args.next().ok_or("--prepare-join needs a source ticket")?,
                });
            }
            "--cancel-join" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::CancelJoin);
            }
            "--confirm-peer" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::ConfirmPeer {
                    ticket: args.next().ok_or("--confirm-peer needs a peer ticket")?,
                });
            }
            "--unpair-peer" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::UnpairPeer {
                    fingerprint: args.next().ok_or("--unpair-peer needs a fingerprint")?,
                });
            }
            "--connect-peer" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::ConnectPeer {
                    fingerprint: args.next().ok_or("--connect-peer needs a fingerprint")?,
                });
            }
            "--lan-status" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::GetLanStatus);
            }
            "--pause-lan" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::PauseLan);
            }
            "--resume-lan" => {
                lan_requested = true;
                startup.push(tanglewood::commands::Command::ResumeLan);
            }
            "--lan-serve" => {
                lan_requested = true;
                lan_serve = true;
            }
            "--mirror" => {
                startup.push(tanglewood::commands::Command::SetMirrorEnabled { enabled: true })
            }
            "--no-mirror" => {
                startup.push(tanglewood::commands::Command::SetMirrorEnabled { enabled: false })
            }
            "--rescan" => startup.push(tanglewood::commands::Command::RescanMirror),
            "--enable-sync" => startup.push(tanglewood::commands::Command::EnableSync),
            "--import" => {
                let source_path = args.next().ok_or("--import needs SOURCE and NOTE_PATH")?;
                let path = args.next().ok_or("--import needs SOURCE and NOTE_PATH")?;
                startup.push(tanglewood::commands::Command::ImportMarkdown {
                    source_path,
                    destination: tanglewood::commands::ImportDestination::Create { path },
                });
            }
            "--export" => {
                let id = tanglewood::notes::NoteId(uuid::Uuid::parse_str(
                    &args
                        .next()
                        .ok_or("--export needs NOTE_UUID and DESTINATION")?,
                )?);
                let path = args
                    .next()
                    .ok_or("--export needs NOTE_UUID and DESTINATION")?;
                startup.push(tanglewood::commands::Command::ExportMarkdown { id, path });
            }
            "--help" | "-h" => {
                println!(
                    "tanglewood [--vault DIRECTORY] [--headless] [--mirror|--no-mirror] [--rescan] [--enable-sync] [--import SOURCE NOTE_PATH] [--export NOTE_UUID DESTINATION] [--mcp-socket SOCKET] [--mcp-write] [--codex [--codex-cwd DIRECTORY] [--codex-resume THREAD]] [--kimi [--kimi-cwd DIRECTORY] [--kimi-resume SESSION]]\nHeadless mode opens the vault and lists note paths. Build with --features gui for the editor.\nLAN (--features lan): --prepare-join TICKET | --cancel-join | --lan-listen ADDRESS:PORT [--lan-advertise IP] | --lan-ticket | --confirm-peer TICKET | --connect-peer FINGERPRINT | --unpair-peer FINGERPRINT | --lan-status | --pause-lan | --resume-lan | --lan-serve. Startup actions run in argument order; use fixed ports for ticket exchange across restarts."
                );
                println!(
                    "Wasm (--features wasm-extensions, --headless): --wasm-component PATH [--wasm-script SOURCE.rhai] [--wasm-input STRING] [--wasm-note NOTE_UUID]. Grants logging and read-only access to the selected note; interpreter mode is selected by --wasm-script."
                );
                println!(
                    "Registered plugins (--features wasm-extensions): --plugin-registry ABSOLUTE_DIRECTORY [--plugin ID ...] [--plugin-run ID | --plugin-evaluate ID --wasm-script SOURCE.rhai] [--wasm-input STRING]. Plugin invocation requires --headless; registered IDs are also available to the GUI and MCP server."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if (plugins.registry.is_some() || !plugins.ids.is_empty() || plugins.invocation.is_some())
        && !cfg!(feature = "wasm-extensions")
    {
        return Err("registered plugins require --features wasm-extensions".into());
    }
    if plugins.registry.is_none() && (!plugins.ids.is_empty() || plugins.invocation.is_some()) {
        return Err("registered plugins require --plugin-registry".into());
    }
    if plugins.ids.iter().any(String::is_empty)
        || plugins
            .invocation
            .as_ref()
            .is_some_and(|invocation| invocation.id().is_empty())
    {
        return Err("plugin IDs must not be empty".into());
    }
    if plugins
        .registry
        .as_ref()
        .is_some_and(|root| !root.is_absolute())
    {
        return Err("--plugin-registry requires an absolute directory".into());
    }
    if plugins.invocation.is_some() && wasm.component.is_some() {
        return Err("choose --wasm-component or a registered plugin invocation".into());
    }
    if wasm.component.is_none() && wasm.note.is_some() {
        return Err("--wasm-note requires --wasm-component".into());
    }
    match &plugins.invocation {
        Some(PluginInvocation::Run(_)) if wasm.script.is_some() => {
            return Err("--plugin-run does not accept --wasm-script".into());
        }
        Some(PluginInvocation::Evaluate(_)) if wasm.script.is_none() => {
            return Err("--plugin-evaluate requires --wasm-script".into());
        }
        Some(_) if !headless => return Err("plugin invocation requires --headless".into()),
        None if wasm.component.is_none() && (wasm.script.is_some() || wasm.input.is_some()) => {
            return Err(
                "Wasm options require --wasm-component or a registered plugin invocation".into(),
            );
        }
        _ => {}
    }
    if wasm.component.is_some() {
        if !cfg!(feature = "wasm-extensions") {
            return Err("Wasm extensions require --features wasm-extensions".into());
        }
        if !headless {
            return Err("--wasm-component requires --headless".into());
        }
    }
    if lan_requested && !cfg!(feature = "lan") {
        return Err("LAN commands require --features lan".into());
    }
    if lan_serve && !lan_listen_requested {
        return Err("--lan-serve requires --lan-listen ADDRESS:PORT".into());
    }
    let vault = vault.unwrap_or_else(|| {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tanglewood/vault")
    });
    if (mcp_path.is_some() || mcp_write) && !cfg!(all(target_os = "linux", feature = "mcp")) {
        return Err("MCP requires Linux and --features mcp".into());
    }
    if mcp_write && mcp_path.is_none() {
        return Err("--mcp-write requires --mcp-socket".into());
    }
    if codex_enabled
        && (!cfg!(all(target_os = "linux", feature = "gui", feature = "codex"))
            || headless
            || mcp_path.is_none())
    {
        return Err(
            "--codex needs Linux, --features gui,codex, a GUI session and --mcp-socket".into(),
        );
    }
    if !codex_enabled && (codex_cwd.is_some() || codex_resume.is_some()) {
        return Err("Codex options require --codex".into());
    }
    if kimi_enabled
        && (!cfg!(all(target_os = "linux", feature = "gui", feature = "kimi"))
            || headless
            || mcp_path.is_none())
    {
        return Err(
            "--kimi needs Linux, --features gui,kimi, a GUI session and --mcp-socket".into(),
        );
    }
    if !kimi_enabled && (kimi_cwd.is_some() || kimi_resume.is_some()) {
        return Err("Kimi options require --kimi".into());
    }
    #[cfg(all(target_os = "linux", feature = "gui", feature = "kimi"))]
    let kimi_config = if kimi_enabled {
        let cwd = match kimi_cwd {
            Some(path) => std::fs::canonicalize(path)?,
            None => {
                let directory = vault.join("kimi-context");
                std::fs::create_dir_all(&directory)?;
                std::fs::canonicalize(directory)?
            }
        };
        Some(tanglewood::kimi::KimiConfig {
            executable: PathBuf::from("kimi"),
            cwd,
            mcp_bridge: std::env::current_exe()?.with_file_name("tanglewood-mcp"),
            socket: std::path::absolute(mcp_path.as_ref().expect("validated MCP socket"))?,
            resume_session: kimi_resume,
        })
    } else {
        None
    };
    #[cfg(all(target_os = "linux", feature = "gui", feature = "codex"))]
    let codex_config = if codex_enabled {
        let cwd = match codex_cwd {
            Some(path) => std::fs::canonicalize(path)?,
            None => {
                let directory = vault.join("codex-context");
                std::fs::create_dir_all(&directory)?;
                std::fs::canonicalize(directory)?
            }
        };
        Some(tanglewood::codex::CodexConfig {
            executable: PathBuf::from("codex"),
            cwd,
            mcp_bridge: std::env::current_exe()?.with_file_name("tanglewood-mcp"),
            socket: std::path::absolute(mcp_path.as_ref().expect("validated MCP socket"))?,
            resume_thread: codex_resume,
            project: None,
        })
    } else {
        None
    };
    let mut app = tanglewood::App::open(&vault)?;
    #[cfg(feature = "wasm-extensions")]
    if let Some(root) = &plugins.registry {
        if let Some(invocation) = &plugins.invocation {
            let id = invocation.id();
            if !plugins.ids.iter().any(|registered| registered == id) {
                plugins.ids.push(id.to_owned());
            }
        }
        app.configure_plugin_registry(root, &plugins.ids)?;
    }
    #[cfg(feature = "lan")]
    let _lan_runtime = {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        app.set_lan_runtime(runtime.handle().clone());
        runtime
    };
    for command in startup {
        let result = app.execute(command, tanglewood::commands::Caller::Local)?;
        if let tanglewood::commands::CommandResult::LanTicket { ticket } = &result {
            println!("{ticket}");
        }
        if let tanglewood::commands::CommandResult::LanStatus(status) = &result {
            #[cfg(feature = "lan")]
            println!("{}", serde_json::to_string(status)?);
            #[cfg(not(feature = "lan"))]
            let _ = status;
        }
        if let tanglewood::commands::CommandResult::Accepted { request_id } = result {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                app.tick();
                match app.io_status(request_id) {
                    Some(tanglewood::app::IoStatus::Completed) => break,
                    Some(tanglewood::app::IoStatus::Failed(error)) => {
                        return Err(error.clone().into());
                    }
                    Some(tanglewood::app::IoStatus::Conflict(id)) => {
                        return Err(format!("recoverable file conflict {id}").into());
                    }
                    _ if std::time::Instant::now() >= deadline => {
                        return Err(
                            "I/O request timed out; its outcome may still be pending".into()
                        );
                    }
                    _ => std::thread::sleep(std::time::Duration::from_millis(2)),
                }
            }
            app.flush()?;
        }
    }
    #[cfg(feature = "wasm-extensions")]
    if let Some(invocation) = plugins.invocation {
        let result = run_registered_plugin(&mut app, invocation, wasm);
        // Accepted effects may need flushing even when the guest fails.
        app.flush_files()?;
        app.shutdown()?;
        return result;
    }
    #[cfg(feature = "wasm-extensions")]
    if wasm.component.is_some() {
        let result = run_wasm_extension(&mut app, wasm);
        // Flush accepted startup work even when the guest fails.
        app.flush_files()?;
        app.shutdown()?;
        return result;
    }
    #[cfg(all(target_os = "linux", feature = "mcp"))]
    let mut owner = mcp_path
        .map(|path| {
            tanglewood::mcp::OwnerSocket::bind(
                &path,
                if mcp_write {
                    tanglewood::mcp::AccessPolicy::ReadWrite
                } else {
                    tanglewood::mcp::AccessPolicy::ReadOnly
                },
            )
        })
        .transpose()?;
    if lan_serve && (headless || !cfg!(feature = "gui")) {
        loop {
            app.tick();
            #[cfg(all(target_os = "linux", feature = "mcp"))]
            if let Some(socket) = &mut owner {
                socket.poll(&mut app);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    if headless || !cfg!(feature = "gui") {
        #[cfg(all(target_os = "linux", feature = "mcp"))]
        if let Some(socket) = &mut owner {
            loop {
                socket.poll(&mut app);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        for view in app.notes() {
            println!(
                "{}\t{}{}",
                view.note.id.0,
                view.note.path,
                if view.note.trashed { " [trash]" } else { "" }
            );
        }
        app.flush_files()?;
        app.shutdown()?;
        return Ok(());
    }
    #[cfg(feature = "gui")]
    {
        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1080.0, 760.0]),
            ..Default::default()
        };
        eframe::run_native(
            "Tanglewood",
            options,
            Box::new(move |_| {
                let editor = tanglewood::ui::TanglewoodUi::new(app);
                #[cfg(all(target_os = "linux", feature = "mcp"))]
                let editor = match owner {
                    Some(socket) => editor.with_mcp(socket),
                    None => editor,
                };
                #[cfg(all(target_os = "linux", feature = "gui", feature = "codex"))]
                let editor = match codex_config {
                    Some(config) => editor.with_codex(config),
                    None => editor,
                };
                #[cfg(all(target_os = "linux", feature = "gui", feature = "kimi"))]
                let editor = match kimi_config {
                    Some(config) => editor.with_kimi(config),
                    None => editor,
                };
                Ok(Box::new(editor))
            }),
        )?;
    }
    Ok(())
}

#[derive(Default)]
struct WasmOptions {
    component: Option<PathBuf>,
    script: Option<PathBuf>,
    input: Option<String>,
    note: Option<tanglewood::notes::NoteId>,
}

#[derive(Default)]
struct PluginOptions {
    registry: Option<PathBuf>,
    ids: Vec<String>,
    invocation: Option<PluginInvocation>,
}

enum PluginInvocation {
    Run(String),
    Evaluate(String),
}
impl PluginInvocation {
    fn id(&self) -> &str {
        match self {
            Self::Run(id) | Self::Evaluate(id) => id,
        }
    }
}

#[cfg(feature = "wasm-extensions")]
fn read_wasm_script(path: PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Read;
    const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;
    let mut source = String::new();
    std::fs::File::open(path)?
        .take(MAX_SCRIPT_BYTES + 1)
        .read_to_string(&mut source)?;
    if source.len() as u64 > MAX_SCRIPT_BYTES {
        return Err("Wasm script exceeds 1 MiB".into());
    }
    Ok(source)
}

#[cfg(feature = "wasm-extensions")]
fn run_registered_plugin(
    app: &mut tanglewood::App,
    invocation: PluginInvocation,
    options: WasmOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    use tanglewood::commands::{Caller, Command, CommandResult};
    use tanglewood::plugin_jobs::{PluginCommand, PluginResult, RunPhase};

    let input = options.input.unwrap_or_default();
    let command = match invocation {
        PluginInvocation::Run(id) => PluginCommand::Run { id, input },
        PluginInvocation::Evaluate(id) => PluginCommand::Evaluate {
            id,
            source: read_wasm_script(options.script.ok_or("missing script")?)?,
            input,
        },
    };
    let CommandResult::Plugin(PluginResult::Started { run_id }) =
        app.execute(Command::Plugin(command), Caller::Local)?
    else {
        return Err("unexpected plugin start result".into());
    };
    eprintln!("Plugin run: {run_id}");
    loop {
        app.tick();
        let CommandResult::Plugin(PluginResult::Status(status)) = app.execute(
            Command::Plugin(PluginCommand::Status { run_id }),
            Caller::Local,
        )?
        else {
            return Err("unexpected plugin status result".into());
        };
        if matches!(status.phase, RunPhase::Running) {
            std::thread::sleep(std::time::Duration::from_millis(2));
            continue;
        }
        for diagnostic in &status.diagnostics {
            eprintln!("Plugin: {diagnostic}");
        }
        for receipt in &status.receipts {
            eprintln!("Plugin effect: {}", serde_json::to_string(receipt)?);
        }
        return match status.phase {
            RunPhase::Completed => {
                println!("{}", status.output.unwrap_or_default());
                Ok(())
            }
            RunPhase::Failed | RunPhase::Cancelled => Err(status
                .error
                .unwrap_or_else(|| "plugin did not complete".into())
                .into()),
            RunPhase::Running => unreachable!(),
        };
    }
}

#[cfg(feature = "wasm-extensions")]
fn run_wasm_extension(
    app: &mut tanglewood::App,
    options: WasmOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    use tanglewood::commands::{Caller, Command, CommandResult};
    use tanglewood::wasm_extensions::{Capabilities, Invocation, NoteSnapshot, Runtime};

    let note = options
        .note
        .map(|id| {
            let CommandResult::Note(view) =
                app.execute(Command::ReadNote { id }, Caller::AutomationReadOnly)?
            else {
                return Err("unexpected read-note command result".into());
            };
            Ok::<_, Box<dyn std::error::Error>>(NoteSnapshot {
                id: view.note.id.0.to_string(),
                path: view.note.path,
                revision: view.note.revision.0.to_string(),
                text: view.note.text,
            })
        })
        .transpose()?;
    let input = options.input.unwrap_or_default();
    let invocation = match options.script {
        Some(path) => {
            let source = read_wasm_script(path)?;
            Invocation::Interpreter { source, input }
        }
        None => Invocation::Compiled { input },
    };
    let runtime = Runtime::new();
    let job = runtime.start(
        options.component.ok_or("missing Wasm component")?,
        invocation,
        Capabilities { log: true, note },
    )?;
    let result = loop {
        app.tick();
        if let Some(result) = job.try_result() {
            match result {
                Ok(result) => break result,
                Err(error) => {
                    for diagnostic in job.diagnostics() {
                        eprintln!("Wasm {:?}: {}", diagnostic.level, diagnostic.message);
                    }
                    return Err(error.into());
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    for diagnostic in result.diagnostics {
        eprintln!("Wasm {:?}: {}", diagnostic.level, diagnostic.message);
    }
    println!("{}", result.output);
    Ok(())
}
