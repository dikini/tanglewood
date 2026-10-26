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
    let private_root = activity
        .internal_data_path()
        .ok_or("Android did not provide app-private internal storage")?;
    let picker_activity = activity.clone();
    let directory = activity
        .internal_data_path()
        .ok_or("Android did not provide app-private internal storage")?
        .join("vault");
    // No fallback to a working directory or a second memory-only writer on failure.
    let app = App::open(&directory)?;
    #[cfg(feature = "lan")]
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    #[cfg(feature = "lan")]
    let app = {
        let mut app = app;
        app.set_lan_runtime(runtime.handle().clone());
        app
    };
    let options = eframe::NativeOptions {
        android_app: Some(activity),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Tanglewood",
        options,
        Box::new(move |creation| {
            let context = creation.egui_ctx.clone();
            // SAFETY: cloned AndroidApp keeps its VM and global Activity live;
            // the picker duplicates the reference rather than adopting it.
            let picker = unsafe {
                crate::android_picker::AndroidPicker::from_android_handles(
                    picker_activity.vm_as_ptr(),
                    picker_activity.activity_as_ptr(),
                    private_root.join("picker-transfer"),
                    move || context.request_repaint(),
                )
            }
            .map_err(std::io::Error::other)?;
            Ok(Box::new(AndroidEditor {
                inner: TanglewoodUi::new(app).with_android_picker(picker),
                #[cfg(feature = "lan")]
                runtime: Some(runtime),
                foreground: true,
            }))
        }),
    )?;
    Ok(())
}

struct AndroidEditor {
    inner: TanglewoodUi,
    foreground: bool,
    #[cfg(feature = "lan")]
    runtime: Option<tokio::runtime::Runtime>,
}

impl Drop for AndroidEditor {
    fn drop(&mut self) {
        #[cfg(feature = "lan")]
        {
            // Stop the foreground controller before terminating its executor.
            // Runtime::drop waits for blocking tasks; never do that on the UI thread.
            let _ = self.inner.app.execute(
                crate::commands::Command::PauseLan,
                crate::commands::Caller::Local,
            );
            self.inner.app.tick();
            if let Some(runtime) = self.runtime.take() {
                runtime.shutdown_background();
            }
        }
    }
}

impl eframe::App for AndroidEditor {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        // Submit changes accepted in this frame promptly, before a possible suspend.
        // Tick is bounded; only the existing worker touches SQLite.
        self.inner.app.tick();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.foreground && !input.focused {
            #[cfg(feature = "lan")]
            if let Err(error) = self.inner.app.execute(
                crate::commands::Command::PauseLan,
                crate::commands::Caller::Local,
            ) {
                log::warn!("Foreground LAN pause failed: {error}");
            }
            // Focus loss can precede suspension, but is not a lifecycle guarantee.
            self.inner.app.tick();
        }
        self.foreground = input.focused;
        self.inner.raw_input_hook(ctx, input);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        #[cfg(feature = "lan")]
        let _ = self.inner.app.execute(
            crate::commands::Command::PauseLan,
            crate::commands::Caller::Local,
        );
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
