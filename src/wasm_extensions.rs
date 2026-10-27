//! Bounded, isolated WebAssembly component execution.
//!
//! `Runtime::start` performs loading, compilation and execution on a worker.
//! Each invocation gets a new Store, instance, WASI context, and host state.
//! Only compiled component code is reused; no mutable guest state is cached.
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::Duration,
};
use wasmtime::component::{
    Component, HasSelf, Linker, Resource, ResourceTable, ResourceType, types::ComponentItem,
};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};
use wasmtime_wasi::{
    WasiCtx, WasiCtxView, WasiView, clocks::WasiClocksView, random::WasiRandomView,
};

mod compiled {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "compiled-extension",
        imports: { default: trappable },
    });
}
mod interpreter {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "interpreter-extension",
        imports: { default: trappable },
        with: { "tanglewood:extensions/host": super::compiled::tanglewood::extensions::host },
    });
}
use compiled::tanglewood::extensions::host;

const MAX_COMPONENT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_NOTE_TEXT_BYTES: usize = 256 * 1024;
const MAX_NOTE_PATH_BYTES: usize = 4096;
const MAX_TOKEN_BYTES: usize = 128;
const MAX_LOG_BYTES: usize = 4096;
const MAX_HOST_CALLS: usize = 128;
const MAX_HOST_BYTES: usize = 1024 * 1024;
const MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_TABLE_ELEMENTS: usize = 10_000;
const MAX_FUEL: u64 = 100_000_000;
const MAX_RUN_TIME: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct NoteSnapshot {
    pub id: String,
    pub path: String,
    pub revision: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    pub log: bool,
    /// A note is available only when selected and authorized by the caller.
    pub note: Option<NoteSnapshot>,
}

#[derive(Clone, Debug)]
pub enum Invocation {
    Compiled { input: String },
    Interpreter { source: String, input: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

#[derive(Debug)]
pub struct InvocationResult {
    pub output: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("component load failed: {0}")]
    Load(String),
    #[error("component interface mismatch: {0}")]
    Interface(String),
    #[error("extension error: {0}")]
    Application(String),
    #[error("guest trapped: {0}")]
    Trap(String),
    #[error("guest resource limit: {0}")]
    ResourceExhausted(String),
    #[error("extension cancelled")]
    Cancelled,
    #[error("extension runtime is busy")]
    Busy,
    #[error("invalid extension input: {0}")]
    InvalidInput(String),
    #[error("extension runtime failed: {0}")]
    Internal(String),
}

struct CachedComponent {
    bytes: Vec<u8>,
    component: Component,
}
struct Shared {
    running: AtomicBool,
    engine: OnceLock<Result<Engine, String>>,
    cache: Mutex<Option<CachedComponent>>,
}

/// One worker may run at a time. Clones share their engine and one-entry cache.
#[derive(Clone)]
pub struct Runtime {
    shared: Arc<Shared>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
impl Runtime {
    /// Cheap to construct: the engine and component are built on the worker.
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                running: AtomicBool::new(false),
                engine: OnceLock::new(),
                cache: Mutex::new(None),
            }),
        }
    }

    /// Start loading and executing a component without blocking the caller.
    pub fn start(
        &self,
        path: PathBuf,
        invocation: Invocation,
        caps: Capabilities,
    ) -> Result<Job, ExtensionError> {
        validate_input(&invocation, &caps)?;
        if self
            .shared
            .running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ExtensionError::Busy);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let (send, receive) = mpsc::sync_channel(1);
        let shared = self.shared.clone();
        let worker_cancelled = cancelled.clone();
        let worker_diagnostics = diagnostics.clone();
        let spawn = thread::Builder::new()
            .name("tanglewood-wasm".into())
            .stack_size(4 * 1024 * 1024)
            .spawn(move || {
                let permit = Running(shared.clone());
                let result = execute(
                    &shared,
                    path,
                    invocation,
                    caps,
                    worker_cancelled,
                    worker_diagnostics,
                );
                drop(permit);
                let _ = send.send(result);
            });
        if let Err(error) = spawn {
            self.shared.running.store(false, Ordering::Release);
            return Err(ExtensionError::Internal(format!(
                "could not start worker: {error}"
            )));
        }
        Ok(Job {
            receive,
            cancelled,
            diagnostics,
            shared: self.shared.clone(),
        })
    }
}

struct Running(Arc<Shared>);
impl Drop for Running {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::Release);
    }
}

/// A handle may be polled without blocking. Dropping it never joins the worker.
pub struct Job {
    receive: Receiver<Result<InvocationResult, ExtensionError>>,
    cancelled: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    shared: Arc<Shared>,
}
impl Job {
    pub fn wait(&self) -> Result<InvocationResult, ExtensionError> {
        self.receive.recv().unwrap_or_else(|_| {
            Err(ExtensionError::Internal(
                "worker stopped without a result".into(),
            ))
        })
    }
    pub fn try_result(&self) -> Option<Result<InvocationResult, ExtensionError>> {
        match self.receive.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(ExtensionError::Internal(
                "worker stopped without a result".into(),
            ))),
        }
    }
    /// Includes messages emitted before a trap or other unsuccessful result.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .lock()
            .map(|messages| messages.clone())
            .unwrap_or_default()
    }
    /// Interrupts guest execution; a compiler already running must finish first.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(Ok(engine)) = self.shared.engine.get() {
            engine.increment_epoch();
        }
    }
}

fn validate_input(invocation: &Invocation, caps: &Capabilities) -> Result<(), ExtensionError> {
    let input_ok = match invocation {
        Invocation::Compiled { input } => input.len() <= MAX_INPUT_BYTES,
        Invocation::Interpreter { source, input } => {
            source.len() <= MAX_INPUT_BYTES && input.len() <= MAX_INPUT_BYTES
        }
    };
    if !input_ok {
        return Err(ExtensionError::InvalidInput(
            "source or input exceeds 1 MiB".into(),
        ));
    }
    if let Some(note) = &caps.note
        && (note.text.len() > MAX_NOTE_TEXT_BYTES
            || note.path.len() > MAX_NOTE_PATH_BYTES
            || note.id.len() > MAX_TOKEN_BYTES
            || note.revision.len() > MAX_TOKEN_BYTES)
    {
        return Err(ExtensionError::InvalidInput(
            "selected note exceeds snapshot limits".into(),
        ));
    }
    Ok(())
}

fn execute(
    shared: &Shared,
    path: PathBuf,
    invocation: Invocation,
    caps: Capabilities,
    cancelled: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
) -> Result<InvocationResult, ExtensionError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ExtensionError::Cancelled);
    }
    let engine = shared
        .engine
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .consume_fuel(true)
                .epoch_interruption(true)
                .max_wasm_stack(1024 * 1024)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| ExtensionError::Internal(error.clone()))?;
    let mut bytes = Vec::new();
    File::open(&path)
        .map_err(|error| ExtensionError::Load(error.to_string()))?
        .take(MAX_COMPONENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ExtensionError::Load(error.to_string()))?;
    if bytes.len() as u64 > MAX_COMPONENT_BYTES {
        return Err(ExtensionError::ResourceExhausted(
            "component exceeds 16 MiB".into(),
        ));
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(ExtensionError::Cancelled);
    }
    let cached = shared
        .cache
        .lock()
        .map_err(|_| ExtensionError::Internal("component cache poisoned".into()))?
        .as_ref()
        .filter(|cached| cached.bytes == bytes)
        .map(|cached| cached.component.clone());
    let component = if let Some(component) = cached {
        component
    } else {
        let component = Component::new(engine, &bytes)
            .map_err(|error| ExtensionError::Load(error.to_string()))?;
        *shared
            .cache
            .lock()
            .map_err(|_| ExtensionError::Internal("component cache poisoned".into()))? =
            Some(CachedComponent {
                bytes,
                component: component.clone(),
            });
        component
    };
    if cancelled.load(Ordering::Acquire) {
        return Err(ExtensionError::Cancelled);
    }

    let expired = Arc::new(AtomicBool::new(false));
    let (stop, wait) = mpsc::sync_channel::<()>(1);
    let timer_engine = engine.clone();
    let timer_expired = expired.clone();
    let timer = thread::Builder::new()
        .name("tanglewood-wasm-deadline".into())
        .spawn(move || {
            if matches!(
                wait.recv_timeout(MAX_RUN_TIME),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                timer_expired.store(true, Ordering::Release);
                timer_engine.increment_epoch();
            }
        })
        .map_err(|error| {
            ExtensionError::Internal(format!("could not start deadline timer: {error}"))
        })?;

    let result = invoke(
        engine,
        &component,
        invocation,
        caps,
        &cancelled,
        &expired,
        &diagnostics,
    );
    let _ = stop.send(());
    let _ = timer.join();
    result
}

#[derive(Debug, thiserror::Error)]
#[error("guest memory or table limit exceeded")]
struct GrowthLimit;
#[derive(Debug, thiserror::Error)]
#[error("native host-call budget exceeded")]
struct HostCallLimit;
struct Limits;
impl ResourceLimiter for Limits {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > MAX_MEMORY_BYTES || maximum.is_some_and(|maximum| desired > maximum) {
            Err(GrowthLimit.into())
        } else {
            Ok(true)
        }
    }
    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > MAX_TABLE_ELEMENTS || maximum.is_some_and(|maximum| desired > maximum) {
            Err(GrowthLimit.into())
        } else {
            Ok(true)
        }
    }
    fn instances(&self) -> usize {
        32
    }
    fn memories(&self) -> usize {
        4
    }
    fn tables(&self) -> usize {
        8
    }
}

struct HostState {
    caps: Capabilities,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    calls: usize,
    host_bytes: usize,
    limits: Limits,
    wasi: WasiCtx,
    resources: ResourceTable,
}
impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.resources,
        }
    }
}
impl HostState {
    fn charge(&mut self, bytes: usize) -> wasmtime::Result<()> {
        self.calls = self.calls.saturating_add(1);
        self.host_bytes = self.host_bytes.saturating_add(bytes);
        if self.calls > MAX_HOST_CALLS || self.host_bytes > MAX_HOST_BYTES {
            Err(HostCallLimit.into())
        } else {
            Ok(())
        }
    }
}
impl host::Host for HostState {
    fn log(
        &mut self,
        level: host::LogLevel,
        message: String,
    ) -> wasmtime::Result<Result<(), host::HostError>> {
        self.charge(message.len())?;
        if !self.caps.log {
            return Ok(Err(host::HostError::Denied));
        }
        if message.len() > MAX_LOG_BYTES {
            return Ok(Err(host::HostError::LimitExceeded));
        }
        let level = match level {
            host::LogLevel::Debug => {
                log::debug!(target: "tanglewood::wasm", "{message}");
                DiagnosticLevel::Debug
            }
            host::LogLevel::Info => {
                log::info!(target: "tanglewood::wasm", "{message}");
                DiagnosticLevel::Info
            }
            host::LogLevel::Warn => {
                log::warn!(target: "tanglewood::wasm", "{message}");
                DiagnosticLevel::Warn
            }
            host::LogLevel::Error => {
                log::error!(target: "tanglewood::wasm", "{message}");
                DiagnosticLevel::Error
            }
        };
        match self.diagnostics.lock() {
            Ok(mut messages) => messages.push(Diagnostic { level, message }),
            Err(_) => return Ok(Err(host::HostError::Unavailable)),
        }
        Ok(Ok(()))
    }
    fn read_note(&mut self) -> wasmtime::Result<Result<host::Note, host::HostError>> {
        let bytes = self.caps.note.as_ref().map_or(0, |note| {
            note.id.len() + note.path.len() + note.revision.len() + note.text.len()
        });
        self.charge(bytes)?;
        let Some(note) = self.caps.note.as_ref() else {
            return Ok(Err(host::HostError::Denied));
        };
        Ok(Ok(host::Note {
            id: note.id.clone(),
            path: note.path.clone(),
            revision: note.revision.clone(),
            text: note.text.clone(),
        }))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("blocking WASI clock subscriptions are disabled")]
struct BlockingWasi;

// Clock reads and the small WASI seed are the only WASI functions needed by
// the sample guests. Charge those callbacks exactly like Tanglewood host calls.
// Subscriptions would create native timer resources and can make poll block
// indefinitely, so they are denied rather than delegated to the default host.
impl wasmtime_wasi::p2::bindings::clocks::monotonic_clock::Host for HostState {
    fn now(&mut self) -> wasmtime::Result<u64> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        wasmtime_wasi::p2::bindings::clocks::monotonic_clock::Host::now(&mut self.clocks())
    }
    fn resolution(&mut self) -> wasmtime::Result<u64> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        wasmtime_wasi::p2::bindings::clocks::monotonic_clock::Host::resolution(&mut self.clocks())
    }
    fn subscribe_instant(
        &mut self,
        _when: u64,
    ) -> wasmtime::Result<Resource<wasmtime_wasi::p2::DynPollable>> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        Err(BlockingWasi.into())
    }
    fn subscribe_duration(
        &mut self,
        _duration: u64,
    ) -> wasmtime::Result<Resource<wasmtime_wasi::p2::DynPollable>> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        Err(BlockingWasi.into())
    }
}
impl wasmtime_wasi::p2::bindings::clocks::wall_clock::Host for HostState {
    fn now(
        &mut self,
    ) -> wasmtime::Result<wasmtime_wasi::p2::bindings::clocks::wall_clock::Datetime> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        wasmtime_wasi::p2::bindings::clocks::wall_clock::Host::now(&mut self.clocks())
    }
    fn resolution(
        &mut self,
    ) -> wasmtime::Result<wasmtime_wasi::p2::bindings::clocks::wall_clock::Datetime> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        wasmtime_wasi::p2::bindings::clocks::wall_clock::Host::resolution(&mut self.clocks())
    }
}
impl wasmtime_wasi::p2::bindings::random::insecure_seed::Host for HostState {
    fn insecure_seed(&mut self) -> wasmtime::Result<(u64, u64)> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        wasmtime_wasi::p2::bindings::random::insecure_seed::Host::insecure_seed(self.random())
    }
}
impl wasmtime_wasi::p2::bindings::sync::io::poll::Host for HostState {
    fn poll(
        &mut self,
        _pollables: Vec<Resource<wasmtime_wasi::p2::DynPollable>>,
    ) -> wasmtime::Result<Vec<u32>> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        Err(BlockingWasi.into())
    }
}
impl wasmtime_wasi::p2::bindings::sync::io::poll::HostPollable for HostState {
    fn ready(
        &mut self,
        _pollable: Resource<wasmtime_wasi::p2::DynPollable>,
    ) -> wasmtime::Result<bool> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        Err(BlockingWasi.into())
    }
    fn block(
        &mut self,
        _pollable: Resource<wasmtime_wasi::p2::DynPollable>,
    ) -> wasmtime::Result<()> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        Err(BlockingWasi.into())
    }
    fn drop(&mut self, pollable: Resource<wasmtime_wasi::p2::DynPollable>) -> wasmtime::Result<()> {
        self.charge(0).map_err(|_| HostCallLimit)?;
        wasmtime_wasi::p2::bindings::sync::io::poll::HostPollable::drop(
            &mut self.resources,
            pollable,
        )
    }
}

fn allowed_import(name: &str) -> bool {
    if name == "tanglewood:extensions/host@0.1.0" {
        return true;
    }
    let Some((interface, version)) = name.rsplit_once('@') else {
        return false;
    };
    let Some(patch) = version.strip_prefix("0.2.") else {
        return false;
    };
    if patch.is_empty() || !patch.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    matches!(
        interface,
        "wasi:clocks/monotonic-clock"
            | "wasi:clocks/wall-clock"
            | "wasi:random/insecure-seed"
            | "wasi:io/poll"
            | "wasi:io/error"
            | "wasi:io/streams"
            | "wasi:cli/environment"
            | "wasi:cli/exit"
            | "wasi:cli/stdin"
            | "wasi:cli/stdout"
            | "wasi:cli/stderr"
            | "wasi:cli/terminal-input"
            | "wasi:cli/terminal-output"
            | "wasi:cli/terminal-stdin"
            | "wasi:cli/terminal-stdout"
            | "wasi:cli/terminal-stderr"
            | "wasi:filesystem/preopens"
            | "wasi:filesystem/types"
    )
}

fn add_wasi(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    use wasmtime_wasi::p2::bindings::{clocks, random};
    clocks::wall_clock::add_to_linker::<_, HasSelf<_>>(linker, |state| state)?;
    clocks::monotonic_clock::add_to_linker::<_, HasSelf<_>>(linker, |state| state)?;
    random::insecure_seed::add_to_linker::<_, HasSelf<_>>(linker, |state| state)?;
    wasmtime_wasi::p2::bindings::sync::io::poll::add_to_linker::<_, HasSelf<_>>(linker, |state| {
        state
    })?;
    Ok(())
}

/// Supply inert traps only for allowlisted Rust-standard-library interfaces
/// whose operations this runtime does not grant. Do not shadow the versioned
/// real interfaces, so Wasmtime can match compatible WASI 0.2.x versions.
fn add_inert_wasi(
    linker: &mut Linker<HostState>,
    engine: &Engine,
    component: &Component,
) -> wasmtime::Result<()> {
    for (name, import) in component.component_type().imports(engine) {
        let base = name.split('@').next().unwrap_or(name);
        if matches!(
            base,
            "tanglewood:extensions/host"
                | "wasi:clocks/monotonic-clock"
                | "wasi:clocks/wall-clock"
                | "wasi:random/insecure-seed"
                | "wasi:io/poll"
        ) {
            continue;
        }
        let ComponentItem::ComponentInstance(interface) = import.ty else {
            return Err(wasmtime::Error::msg(format!(
                "unsupported WASI import kind: {name}"
            )));
        };
        let mut target = linker.instance(name)?;
        for (member, item) in interface.exports(engine) {
            match item.ty {
                ComponentItem::ComponentFunc(_) => {
                    target.func_new(member, |_, _, _, _| Err(BlockingWasi.into()))?;
                }
                ComponentItem::Resource(_) => {
                    let resource = if member == "pollable" {
                        ResourceType::host::<wasmtime_wasi::p2::DynPollable>()
                    } else {
                        ResourceType::host::<()>()
                    };
                    target.resource(member, resource, |_, _| Ok(()))?;
                }
                ComponentItem::Type(_) => {}
                _ => {
                    return Err(wasmtime::Error::msg(format!(
                        "unsupported WASI member: {name}#{member}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn invoke(
    engine: &Engine,
    component: &Component,
    invocation: Invocation,
    caps: Capabilities,
    cancelled: &Arc<AtomicBool>,
    expired: &Arc<AtomicBool>,
    diagnostics: &Arc<Mutex<Vec<Diagnostic>>>,
) -> Result<InvocationResult, ExtensionError> {
    if let Some((name, _)) = component
        .component_type()
        .imports(engine)
        .find(|(name, _)| !allowed_import(name))
    {
        return Err(ExtensionError::Interface(format!(
            "import is not registered: {name}"
        )));
    }
    let mut linker = Linker::new(engine);
    add_inert_wasi(&mut linker, engine, component)
        .map_err(|error| ExtensionError::Interface(error.to_string()))?;
    add_wasi(&mut linker).map_err(|error| ExtensionError::Internal(error.to_string()))?;
    let mut store = Store::new(
        engine,
        HostState {
            caps,
            diagnostics: diagnostics.clone(),
            calls: 0,
            host_bytes: 0,
            limits: Limits,
            wasi: WasiCtx::builder().build(),
            resources: ResourceTable::new(),
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(MAX_FUEL)
        .map_err(|error| ExtensionError::Internal(error.to_string()))?;
    store.set_epoch_deadline(1);
    let job_cancelled = cancelled.clone();
    let job_expired = expired.clone();
    store.epoch_deadline_callback(move |_| {
        if job_cancelled.load(Ordering::Acquire) || job_expired.load(Ordering::Acquire) {
            Ok(wasmtime::UpdateDeadline::Interrupt)
        } else {
            Ok(wasmtime::UpdateDeadline::Continue(1))
        }
    });
    if cancelled.load(Ordering::Acquire) {
        return Err(ExtensionError::Cancelled);
    }
    if expired.load(Ordering::Acquire) {
        return Err(ExtensionError::ResourceExhausted(
            "execution deadline exceeded".into(),
        ));
    }
    let outcome = match invocation {
        Invocation::Compiled { input } => {
            compiled::CompiledExtension::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
                .map_err(|error| ExtensionError::Internal(error.to_string()))?;
            let guest = compiled::CompiledExtension::instantiate(&mut store, component, &linker)
                .map_err(|error| classify(error, true, cancelled, expired))?;
            guest
                .tanglewood_extensions_compiled()
                .call_run(&mut store, &input)
        }
        Invocation::Interpreter { source, input } => {
            interpreter::InterpreterExtension::add_to_linker::<_, HasSelf<_>>(
                &mut linker,
                |state| state,
            )
            .map_err(|error| ExtensionError::Internal(error.to_string()))?;
            let guest =
                interpreter::InterpreterExtension::instantiate(&mut store, component, &linker)
                    .map_err(|error| classify(error, true, cancelled, expired))?;
            guest
                .tanglewood_extensions_interpreter()
                .call_evaluate(&mut store, &source, &input)
        }
    }
    .map_err(|error| classify(error, false, cancelled, expired))?;
    let output = match outcome {
        Ok(output) if output.len() <= MAX_OUTPUT_BYTES => output,
        Err(error) if error.len() <= MAX_OUTPUT_BYTES => {
            return Err(ExtensionError::Application(error));
        }
        _ => {
            return Err(ExtensionError::ResourceExhausted(
                "guest result exceeds 1 MiB".into(),
            ));
        }
    };
    Ok(InvocationResult {
        output,
        diagnostics: diagnostics
            .lock()
            .map(|messages| messages.clone())
            .unwrap_or_default(),
    })
}

fn classify(
    error: wasmtime::Error,
    instantiating: bool,
    cancelled: &AtomicBool,
    expired: &AtomicBool,
) -> ExtensionError {
    if cancelled.load(Ordering::Acquire) {
        return ExtensionError::Cancelled;
    }
    if expired.load(Ordering::Acquire) {
        return ExtensionError::ResourceExhausted("execution deadline exceeded".into());
    }
    // Wasmtime 47 reports Store instance/memory/table count limits as an
    // untyped error. Match its exact pinned message in the error chain before
    // the interface-mismatch fallback used during instantiation.
    let count_limit = error.chain().any(|cause| {
        let message = cause.to_string();
        ["instance", "memory", "table"].iter().any(|kind| {
            message
                .strip_prefix(&format!(
                    "resource limit exceeded: {kind} count too high at "
                ))
                .is_some_and(|count| {
                    !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
    });
    if count_limit
        || error.downcast_ref::<GrowthLimit>().is_some()
        || error.downcast_ref::<HostCallLimit>().is_some()
        || matches!(
            error.downcast_ref::<Trap>(),
            Some(Trap::OutOfFuel | Trap::StackOverflow)
        )
    {
        return ExtensionError::ResourceExhausted(format!("{error:#}"));
    }
    if error.downcast_ref::<Trap>().is_some() {
        return ExtensionError::Trap(error.to_string());
    }
    if instantiating {
        ExtensionError::Interface(format!("{error:#}"))
    } else {
        ExtensionError::Trap(error.to_string())
    }
}
