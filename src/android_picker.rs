//! Android SAF boundary: provider I/O owns descriptors, a single decoder owns
//! temporary imports, and only the App owner receives source-edit commands.
use crate::{
    commands::Command,
    notes::{NoteId, NoteView, Revision, TextEdit},
};
use jni::{
    EnvUnowned, JValue, JavaVM, jni_sig, jni_str,
    objects::{JByteArray, JClass, JObject, JString},
    refs::Global,
    sys::{jboolean, jint, jlong},
};
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::Duration,
};

const CHUNK_BYTES: usize = 64 * 1024;
static NEXT_ID: AtomicI64 = AtomicI64::new(1);
static ROUTE: OnceLock<Mutex<Option<Route>>> = OnceLock::new();
fn route() -> &'static Mutex<Option<Route>> {
    ROUTE.get_or_init(|| Mutex::new(None))
}
fn next_id() -> Result<i64, String> {
    NEXT_ID
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
        .map_err(|_| "document IDs exhausted".into())
}
struct Route {
    session: i64,
    active: i64,
    delivered: bool,
    cancelled: bool,
    snapshot: Option<Arc<String>>,
    send: SyncSender<RawResult>,
    wake: Arc<dyn Fn() + Send + Sync>,
    fault: Arc<Mutex<Option<String>>>,
}
struct RawResult {
    id: i64,
    result: Result<Option<PathBuf>, String>,
    cancelled: bool,
}
struct DecodeJob {
    id: i64,
    path: PathBuf,
    cancel: Arc<AtomicBool>,
}
struct Decoded {
    id: i64,
    text: Result<String, String>,
}
enum ImportTarget {
    Create {
        path: String,
    },
    Replace {
        id: NoteId,
        expected: Revision,
        bytes: usize,
    },
}
enum Operation {
    Import(ImportTarget),
    Export { note: NoteId, revision: Revision },
}
struct Pending {
    id: i64,
    operation: Operation,
    cancel: Arc<AtomicBool>,
}
#[derive(Debug)]
pub enum PickerCompletion {
    ImportReady {
        request_id: i64,
        command: Command,
    },
    Exported {
        request_id: i64,
        note: NoteId,
        revision: Revision,
    },
    Cancelled {
        request_id: i64,
    },
    Failed {
        request_id: i64,
        error: String,
    },
}
pub struct AndroidPicker {
    vm: JavaVM,
    activity: Global<JObject<'static>>,
    session: i64,
    receive: Receiver<RawResult>,
    pending: Option<Pending>,
    decode: SyncSender<DecodeJob>,
    decoded: Receiver<Decoded>,
    stop: Arc<AtomicBool>,
    fault: Arc<Mutex<Option<String>>>,
}
impl AndroidPicker {
    /// # Safety
    /// vm/activity must be live VM/global Activity handles from the current
    /// AndroidApp. private_root must be its internal_data_path()/picker-transfer.
    /// This owner creates its own global reference and never adopts the App's ref.
    pub unsafe fn from_android_handles(
        vm: *mut std::ffi::c_void,
        activity: *mut std::ffi::c_void,
        private_root: PathBuf,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        if vm.is_null() || activity.is_null() || !private_root.is_absolute() {
            return Err("Android activity/storage handles unavailable".into());
        }
        // SAFETY: caller supplies the VM owned by the live AndroidApp.
        let vm = unsafe { JavaVM::from_raw(vm.cast()) };
        let object = vm
            .attach_current_thread(|env| -> jni::errors::Result<_> {
                let raw = env.get_raw();
                // SAFETY: AndroidApp's global ref is live; duplicate it into this JNI frame.
                let local = unsafe { ((**raw).v1_2.NewLocalRef)(raw, activity.cast()) };
                if local.is_null() {
                    return Err(jni::errors::Error::NullPtr("Activity reference"));
                }
                // SAFETY: this newly created local ref is unique and belongs to this frame.
                env.new_global_ref(unsafe { JObject::from_raw(env, local) })
            })
            .map_err(|error| error.to_string())?;
        let (send, receive) = mpsc::sync_channel(1);
        let (decode, jobs) = mpsc::sync_channel::<DecodeJob>(1);
        let (done, decoded) = mpsc::sync_channel(1);
        let fault = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let worker_stop = stop.clone();
        let worker_fault = fault.clone();
        let worker_wake = wake.clone();
        thread::Builder::new()
            .name("tanglewood-document-decode".into())
            .spawn(move || {
                loop {
                    let job = match jobs.recv_timeout(Duration::from_millis(50)) {
                        Ok(job) => job,
                        Err(mpsc::RecvTimeoutError::Timeout)
                            if worker_stop.load(Ordering::Acquire) =>
                        {
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    };
                    let text = decode_spool(&private_root, &job, &worker_stop);
                    if done.try_send(Decoded { id: job.id, text }).is_err() {
                        if let Ok(mut fault) = worker_fault.lock() {
                            *fault = Some("document decode completion unavailable".into());
                        }
                    }
                    (worker_wake)();
                }
            })
            .map_err(|error| error.to_string())?;
        let session = next_id()?;
        let mut slot = route().lock().map_err(|_| "picker route poisoned")?;
        if slot.is_some() {
            return Err("an Android picker owner already exists".into());
        }
        *slot = Some(Route {
            session,
            active: 0,
            delivered: false,
            cancelled: false,
            snapshot: None,
            send,
            wake,
            fault: fault.clone(),
        });
        Ok(Self {
            vm,
            activity: object,
            session,
            receive,
            pending: None,
            decode,
            decoded,
            stop,
            fault,
        })
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn import_create(&mut self, path: String) -> Result<i64, String> {
        self.launch(
            Operation::Import(ImportTarget::Create { path }),
            "Import Markdown",
            None,
        )
    }
    pub fn import_replace(&mut self, note: &NoteView) -> Result<i64, String> {
        self.launch(
            Operation::Import(ImportTarget::Replace {
                id: note.note.id,
                expected: note.note.revision,
                bytes: note.note.text.len(),
            }),
            "Import Markdown",
            None,
        )
    }
    pub fn export(&mut self, note: &NoteView) -> Result<i64, String> {
        self.launch(
            Operation::Export {
                note: note.note.id,
                revision: note.note.revision,
            },
            note.note.path.rsplit('/').next().unwrap_or("note.md"),
            Some(Arc::new(note.note.text.clone())),
        )
    }
    fn launch(
        &mut self,
        operation: Operation,
        title: &str,
        snapshot: Option<Arc<String>>,
    ) -> Result<i64, String> {
        if self.pending.is_some() {
            return Err("a document request is already pending".into());
        }
        let exporting = snapshot.is_some();
        let id = next_id()?;
        {
            let mut slot = route().lock().map_err(|_| "picker route poisoned")?;
            let owner = slot
                .as_mut()
                .filter(|route| route.session == self.session)
                .ok_or("picker owner unavailable")?;
            owner.active = id;
            owner.delivered = false;
            owner.cancelled = false;
            owner.snapshot = snapshot;
        }
        self.pending = Some(Pending {
            id,
            operation,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        let result = self
            .vm
            .attach_current_thread(|env| -> jni::errors::Result<()> {
                let title = env.new_string(title)?;
                env.call_method(
                    &self.activity,
                    jni_str!("requestDocument"),
                    jni_sig!("(JZLjava/lang/String;)V"),
                    &[
                        JValue::Long(id),
                        JValue::Bool(exporting),
                        JValue::Object(title.as_ref()),
                    ],
                )?;
                Ok(())
            });
        if let Err(error) = result {
            self.pending = None;
            self.clear_active();
            return Err(error.to_string());
        }
        Ok(id)
    }
    pub fn cancel(&mut self) -> Result<(), String> {
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        pending.cancel.store(true, Ordering::Release);
        let id = pending.id;
        if let Ok(mut slot) = route().lock() {
            if let Some(owner) = slot.as_mut().filter(|route| route.session == self.session) {
                owner.cancelled = true;
            }
        }
        self.vm
            .attach_current_thread(|env| -> jni::errors::Result<()> {
                env.call_method(
                    &self.activity,
                    jni_str!("cancelDocument"),
                    jni_sig!("(J)V"),
                    &[JValue::Long(id)],
                )?;
                Ok(())
            })
            .map_err(|error| error.to_string())
    }
    fn clear_active(&self) {
        if let Ok(mut slot) = route().lock() {
            if let Some(owner) = slot.as_mut().filter(|route| route.session == self.session) {
                owner.active = 0;
                owner.snapshot = None;
            }
        }
    }
    fn finish(
        &mut self,
        text: Result<Option<String>, String>,
        cancelled: bool,
    ) -> Option<PickerCompletion> {
        let pending = self.pending.take()?;
        self.clear_active();
        if cancelled || pending.cancel.load(Ordering::Acquire) {
            return Some(PickerCompletion::Cancelled {
                request_id: pending.id,
            });
        }
        Some(match text {
            Err(error) => PickerCompletion::Failed {
                request_id: pending.id,
                error,
            },
            Ok(text) => match pending.operation {
                Operation::Export { note, revision } => PickerCompletion::Exported {
                    request_id: pending.id,
                    note,
                    revision,
                },
                Operation::Import(target) => match text {
                    Some(text) => {
                        let command = match target {
                            ImportTarget::Create { path } => Command::CreateNote { path, text },
                            ImportTarget::Replace {
                                id,
                                expected,
                                bytes,
                            } => Command::EditNote {
                                id,
                                expected,
                                edits: vec![TextEdit {
                                    start: 0,
                                    end: bytes,
                                    replacement: text,
                                }],
                            },
                        };
                        PickerCompletion::ImportReady {
                            request_id: pending.id,
                            command,
                        }
                    }
                    None => PickerCompletion::Failed {
                        request_id: pending.id,
                        error: "provider returned no imported text".into(),
                    },
                },
            },
        })
    }
    /// Owner-only polling: returns commands but never opens a file or mutates App.
    pub fn poll(&mut self) -> Option<PickerCompletion> {
        let fault = self.fault.lock().ok().and_then(|mut fault| fault.take());
        if let Some(error) = fault {
            return self.finish(Err(error), false);
        }
        if let Ok(decoded) = self.decoded.try_recv() {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.id == decoded.id)
            {
                return self.finish(decoded.text.map(Some), false);
            }
        }
        let raw = self.receive.try_recv().ok()?;
        let pending = self
            .pending
            .as_mut()
            .filter(|pending| pending.id == raw.id)?;
        match raw.result {
            Err(error) => self.finish(Err(error), raw.cancelled),
            Ok(Some(path)) => {
                let job = DecodeJob {
                    id: pending.id,
                    path,
                    cancel: pending.cancel.clone(),
                };
                if self.decode.try_send(job).is_err() {
                    // Only an owned private spool is returned. Queue capacity is one operation.
                    return self.finish(
                        Err(
                            "document decoder unavailable; private spool retained for recovery"
                                .into(),
                        ),
                        false,
                    );
                }
                None
            }
            Ok(None) => self.finish(Ok(None), raw.cancelled),
        }
    }
}
impl Drop for AndroidPicker {
    fn drop(&mut self) {
        let _ = self.cancel();
        if let Ok(mut slot) = route().lock() {
            if slot
                .as_ref()
                .is_some_and(|route| route.session == self.session)
            {
                *slot = None;
            }
        }
        // Hand queued private spools to the existing worker for cancellation cleanup.
        while let Ok(raw) = self.receive.try_recv() {
            if let Ok(Some(path)) = raw.result {
                let _ = self.decode.try_send(DecodeJob {
                    id: raw.id,
                    path,
                    cancel: Arc::new(AtomicBool::new(true)),
                });
            }
        }
        self.stop.store(true, Ordering::Release);
    }
}
fn decode_spool(
    root: &std::path::Path,
    job: &DecodeJob,
    stop: &AtomicBool,
) -> Result<String, String> {
    if job.path.parent() != Some(root)
        || !job
            .path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("import-"))
    {
        return Err("invalid private import spool".into());
    }
    let result = (|| {
        let metadata = fs::symlink_metadata(&job.path).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("import spool is not a regular file".into());
        }
        let mut file = fs::File::open(&job.path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        let mut chunk = [0; CHUNK_BYTES];
        loop {
            if stop.load(Ordering::Acquire) || job.cancel.load(Ordering::Acquire) {
                return Err("import cancelled".into());
            }
            let count = file.read(&mut chunk).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            bytes.try_reserve(count).map_err(|_| {
                "not enough memory to load this source; no import applied".to_owned()
            })?;
            bytes.extend_from_slice(&chunk[..count]);
        }
        String::from_utf8(bytes)
            .map_err(|_| "document is not valid UTF-8; no import applied".into())
    })();
    let _ = fs::remove_file(&job.path);
    result
}

/// Import callback transfers only an app-private spool name, never whole JNI arrays.
/// Returning false leaves Java responsible for deleting the spool.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeDocumentResult<'local>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    id: jlong,
    status: jint,
    path: JString<'local>,
    error: JString<'local>,
) -> jboolean {
    unowned
        .with_env(|env| -> jni::errors::Result<bool> {
            let result = match status {
                0 => path
                    .try_to_string(env)
                    .map(|path| {
                        if path.is_empty() {
                            None
                        } else {
                            Some(PathBuf::from(path))
                        }
                    })
                    .map_err(|error| error.to_string()),
                1 => Ok(None),
                _ => Err(error.try_to_string(env).unwrap_or_else(|_| {
                    "Android document provider error (message unavailable)".into()
                })),
            };
            // Serialize ownership transfer with Drop's unregister-before-drain.
            // No cloned sender may enqueue a spool after the owner has drained.
            let (accepted, wake) = {
                let Ok(mut slot) = route().lock() else {
                    return Ok(false);
                };
                let Some(owner) = slot
                    .as_mut()
                    .filter(|route| route.active == id && !route.delivered)
                else {
                    return Ok(false);
                };
                owner.delivered = true;
                let accepted = owner
                    .send
                    .try_send(RawResult {
                        id,
                        result,
                        cancelled: status == 1,
                    })
                    .is_ok();
                if !accepted {
                    if let Ok(mut fault) = owner.fault.lock() {
                        *fault = Some("document completion unavailable; no import applied".into());
                    }
                }
                (accepted, owner.wake.clone())
            };
            wake();
            Ok(accepted)
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
/// Java's worker requests at most 64 KiB of the immutable selected snapshot per call.
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_tanglewood_TanglewoodActivity_nativeExportChunk<'local>(
    mut unowned: EnvUnowned<'local>,
    _class: JClass<'local>,
    id: jlong,
    offset: jlong,
) -> JByteArray<'local> {
    unowned
        .with_env(|env| -> jni::errors::Result<JByteArray<'_>> {
            let snapshot = {
                let Ok(slot) = route().lock() else {
                    return Ok(JByteArray::null());
                };
                let Some(owner) = slot
                    .as_ref()
                    .filter(|route| route.active == id && !route.cancelled)
                else {
                    return Ok(JByteArray::null());
                };
                owner.snapshot.clone()
            };
            let Some(snapshot) = snapshot else {
                return Ok(JByteArray::null());
            };
            let Ok(start) = usize::try_from(offset) else {
                return Ok(JByteArray::null());
            };
            if start > snapshot.len() {
                return Ok(JByteArray::null());
            }
            let end = start.saturating_add(CHUNK_BYTES).min(snapshot.len());
            env.byte_array_from_slice(&snapshot.as_bytes()[start..end])
        })
        .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
