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
    let mut startup = Vec::new();
    let mut mcp_path = None;
    let mut mcp_write = false;
    let mut codex_enabled = false;
    let mut codex_cwd = None;
    let mut codex_resume = None;
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
                    "tanglewood [--vault DIRECTORY] [--headless] [--mirror|--no-mirror] [--rescan] [--enable-sync] [--import SOURCE NOTE_PATH] [--export NOTE_UUID DESTINATION] [--mcp-socket SOCKET] [--mcp-write] [--codex [--codex-cwd DIRECTORY] [--codex-resume THREAD]]\nHeadless mode opens the vault and lists note paths. Build with --features gui for the editor.\nLAN (--features lan): --prepare-join TICKET | --cancel-join | --lan-listen ADDRESS:PORT [--lan-advertise IP] | --lan-ticket | --confirm-peer TICKET | --connect-peer FINGERPRINT | --unpair-peer FINGERPRINT | --lan-status | --pause-lan | --resume-lan | --lan-serve. Startup actions run in argument order; use fixed ports for ticket exchange across restarts."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
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
                Ok(Box::new(editor))
            }),
        )?;
    }
    Ok(())
}
