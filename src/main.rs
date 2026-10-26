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
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vault" => {
                vault = Some(PathBuf::from(
                    args.next().ok_or("--vault needs a directory")?,
                ))
            }
            "--headless" => headless = true,
            "--help" | "-h" => {
                println!(
                    "tanglewood [--vault DIRECTORY] [--headless]\nHeadless mode opens the vault and lists note paths. Build with --features gui for the editor."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
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
    let mut app = tanglewood::App::open(&vault)?;
    if headless || !cfg!(feature = "gui") {
        for view in app.notes() {
            println!(
                "{}\t{}{}",
                view.note.id.0,
                view.note.path,
                if view.note.trashed { " [trash]" } else { "" }
            );
        }
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
            Box::new(move |_| Ok(Box::new(tanglewood::ui::TanglewoodUi::new(app)))),
        )?;
    }
    Ok(())
}
