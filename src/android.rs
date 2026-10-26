//! Android NativeActivity entry. Winit owns Android event polling and lifecycle.
use eframe::egui;
use winit::platform::android::activity::AndroidApp;

use crate::{App, ui::TanglewoodUi};

// android-activity resolves this Rust-ABI symbol from the application library.
#[unsafe(no_mangle)]
fn android_main(activity: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("Tanglewood")
            .with_max_level(log::LevelFilter::Info),
    );
    if let Err(error) = run(activity) {
        log::error!("Android startup failed: {error}");
    }
}

fn run(activity: AndroidApp) -> Result<(), Box<dyn std::error::Error>> {
    let directory = activity
        .internal_data_path()
        .ok_or("Android did not provide app-private internal storage")?
        .join("vault");
    // No fallback to a working directory or a second memory-only writer on failure.
    let app = App::open(&directory)?;
    let options = eframe::NativeOptions {
        android_app: Some(activity),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Tanglewood",
        options,
        Box::new(move |_| {
            Ok(Box::new(AndroidEditor {
                inner: TanglewoodUi::new(app),
            }))
        }),
    )?;
    Ok(())
}

struct AndroidEditor {
    inner: TanglewoodUi,
}

impl eframe::App for AndroidEditor {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        // Submit changes accepted in this frame promptly, before a possible suspend.
        // Tick is bounded; only the existing worker touches SQLite.
        self.inner.app.tick();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if !input.focused {
            // Focus loss can precede suspension, but is not a lifecycle guarantee.
            self.inner.app.tick();
        }
        self.inner.raw_input_hook(ctx, input);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.inner.app.tick();
        if self
            .inner
            .app
            .notes()
            .iter()
            .any(|view| view.save.persisted != Some(view.save.accepted))
        {
            log::warn!("Android exit with unacknowledged changes; saved status was not confirmed");
        }
        // No blocking flush or independent event loop in an Android lifecycle callback.
        // Normal editing submits continuously; a process kill may lose unsaved changes.
    }
}
