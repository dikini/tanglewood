//! Android NativeActivity entry. Winit owns Android event polling and lifecycle.
use eframe::egui;
use std::sync::{Arc, Mutex};
#[derive(Clone, Copy)]
struct Insets {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    width: i32,
    height: i32,
}
struct AndroidBridge {
    owner: i64,
    geometry: Option<Insets>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}
static BRIDGE: Mutex<Option<AndroidBridge>> = Mutex::new(None);
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeBegin(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    owner: i64,
) {
    if let Ok(mut bridge) = BRIDGE.lock() {
        *bridge = Some(AndroidBridge {
            owner,
            geometry: None,
            wake: None,
        });
    }
}
fn unregister_bridge(owner: i64) {
    if let Ok(mut bridge) = BRIDGE.lock() {
        if bridge.as_ref().is_some_and(|bridge| bridge.owner == owner) {
            *bridge = None;
        }
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeEnd(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    owner: i64,
) {
    unregister_bridge(owner);
    crate::android_input::end_owner(owner);
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeSystemInsets(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    owner: i64,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    width: i32,
    height: i32,
) {
    let wake = if let Ok(mut bridge) = BRIDGE.lock() {
        if let Some(bridge) = bridge.as_mut().filter(|bridge| bridge.owner == owner) {
            bridge.geometry = (width > 0 && height > 0).then_some(Insets {
                left,
                top,
                right,
                bottom,
                width,
                height,
            });
            bridge.wake.clone()
        } else {
            None
        }
    } else {
        None
    };
    if let Some(wake) = wake {
        wake();
    }
}
fn register_bridge(activity: &AndroidApp, context: egui::Context) -> Result<i64, String> {
    // SAFETY: AndroidApp owns these live VM/global Activity handles.
    let vm = unsafe { jni::JavaVM::from_raw(activity.vm_as_ptr().cast()) };
    let owner = vm
        .attach_current_thread(|env| -> jni::errors::Result<i64> {
            let raw = env.get_raw();
            // Duplicate the global reference into this JNI frame; never adopt AndroidApp's ref.
            let local =
                unsafe { ((**raw).v1_2.NewLocalRef)(raw, activity.activity_as_ptr().cast()) };
            if local.is_null() {
                return Err(jni::errors::Error::NullPtr("Activity reference"));
            }
            let local = unsafe { jni::objects::JObject::from_raw(env, local) };
            env.get_field(&local, jni::jni_str!("nativeOwner"), jni::jni_sig!("J"))?
                .j()
        })
        .map_err(|error| error.to_string())?;
    let mut bridge = BRIDGE.lock().map_err(|_| "Android bridge poisoned")?;
    let bridge = bridge
        .as_mut()
        .filter(|bridge| bridge.owner == owner)
        .ok_or("Android Activity bridge unavailable")?;
    bridge.wake = Some(Arc::new(move || context.request_repaint()));
    Ok(owner)
}
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
            let bridge_owner = register_bridge(&picker_activity, context.clone())
                .map_err(std::io::Error::other)?;
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
            let input_context = creation.egui_ctx.clone();
            // SAFETY: cloned AndroidApp holds these handles live; adapter duplicates the reference.
            let input = unsafe {
                crate::android_input::AndroidInput::new(
                    picker_activity.vm_as_ptr(),
                    picker_activity.activity_as_ptr(),
                    bridge_owner,
                    move || input_context.request_repaint(),
                )
            }
            .map_err(std::io::Error::other)?;
            Ok(Box::new(AndroidEditor {
                inner: TanglewoodUi::new(app).with_android_picker(picker),
                #[cfg(feature = "lan")]
                runtime: Some(runtime),
                foreground: true,
                activity: picker_activity,
                bridge_owner,
                input,
            }))
        }),
    )?;
    Ok(())
}

struct AndroidEditor {
    inner: TanglewoodUi,
    foreground: bool,
    activity: AndroidApp,
    bridge_owner: i64,
    input: crate::android_input::AndroidInput,

    #[cfg(feature = "lan")]
    runtime: Option<tokio::runtime::Runtime>,
}

impl Drop for AndroidEditor {
    fn drop(&mut self) {
        unregister_bridge(self.bridge_owner);
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
        self.input.flush(ctx);
        // Submit changes accepted in this frame promptly, before a possible suspend.
        // Tick is bounded; only the existing worker touches SQLite.
        self.inner.app.tick();
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        // Native visible geometry and the coherent Java insets snapshot are physical
        // pixels. Intersect with the actual raw viewport so adjustResize is not counted twice.
        if let Some(screen) = input.screen_rect {
            let geometry = BRIDGE.lock().ok().and_then(|bridge| {
                bridge
                    .as_ref()
                    .filter(|bridge| bridge.owner == self.bridge_owner)
                    .and_then(|bridge| bridge.geometry)
            });
            if let Some(geometry) = geometry {
                let scale = ctx.pixels_per_point();
                let rect = self.activity.content_rect();
                let mut visible = egui::Rect::from_min_max(
                    egui::pos2(
                        geometry.left.max(0) as f32 / scale,
                        geometry.top.max(0) as f32 / scale,
                    ),
                    egui::pos2(
                        (geometry.width - geometry.right.max(0)).max(0) as f32 / scale,
                        (geometry.height - geometry.bottom.max(0)).max(0) as f32 / scale,
                    ),
                );
                if rect.right > rect.left && rect.bottom > rect.top {
                    visible = visible.intersect(egui::Rect::from_min_max(
                        egui::pos2(rect.left as f32 / scale, rect.top as f32 / scale),
                        egui::pos2(rect.right as f32 / scale, rect.bottom as f32 / scale),
                    ));
                }
                visible = visible.intersect(screen);
                // Reject transient empty/invalid geometry rather than zeroing the UI.
                if visible.is_positive() {
                    input.safe_area_insets = Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
                        left: (visible.left() - screen.left()).clamp(0.0, screen.width()),
                        top: (visible.top() - screen.top()).clamp(0.0, screen.height()),
                        right: (screen.right() - visible.right()).clamp(0.0, screen.width()),
                        bottom: (screen.bottom() - visible.bottom()).clamp(0.0, screen.height()),
                    }));
                }
            }
        }
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
