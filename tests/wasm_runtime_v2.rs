#![cfg(feature = "wasm-extensions")]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tanglewood::{
    commands::Command,
    plugin_manifest::Limits,
    wasm_commands::host,
    wasm_extensions::{
        BridgeReceipt, CallContext, CommandBridge, ExtensionError, Invocation, Runtime,
    },
};

struct WaitingBridge {
    entered: mpsc::SyncSender<()>,
}
impl CommandBridge for WaitingBridge {
    fn execute(
        &self,
        command: Command,
        ctx: &CallContext,
    ) -> Result<BridgeReceipt, host::CommandError> {
        assert!(matches!(command, Command::CreateNote { .. }));
        let _ = self.entered.try_send(());
        while !ctx.cancelled.load(Ordering::Acquire) && Instant::now() < ctx.deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        if ctx.cancelled.load(Ordering::Acquire) {
            Err(host::CommandError::Cancelled)
        } else {
            Err(host::CommandError::Timeout)
        }
    }
    fn read_result(
        &self,
        _: String,
        _: Vec<host::PathElement>,
        _: u64,
        _: u32,
        _: &CallContext,
    ) -> Result<host::ResultNode, host::CommandError> {
        panic!("unexpected result read")
    }
    fn capabilities(&self, _: &CallContext) -> Result<Vec<host::Capability>, host::CommandError> {
        panic!("unexpected discovery")
    }
}

#[test]
fn owner_waits_observe_cancel_and_manifest_deadline() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/wasm-extensions/wasm32-wasip2/release/tanglewood_compiled_v2_guest.wasm");
    let bytes: Arc<[u8]> = std::fs::read(&path)
        .unwrap_or_else(|_| {
            panic!(
                "missing {}: run ./scripts/build-wasm-extensions.sh first",
                path.display()
            )
        })
        .into();
    let runtime = Runtime::new();
    let (entered, receiver) = mpsc::sync_channel(1);
    let bridge = Arc::new(WaitingBridge { entered });
    let invocation = || Invocation::Compiled {
        input: "Notes/test.md".into(),
    };

    let job = runtime
        .start_component(bytes.clone(), invocation(), bridge.clone(), Limits::HOST)
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(60))
        .expect("guest called owner bridge");
    job.cancel();
    assert!(matches!(job.wait(), Err(ExtensionError::Cancelled)));

    let short = Limits {
        timeout_ms: 50,
        ..Limits::HOST
    };
    let job = runtime
        .start_component(bytes, invocation(), bridge, short)
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(60))
        .expect("guest called owner bridge");
    assert!(matches!(
        job.wait(),
        Err(ExtensionError::ResourceExhausted(_))
    ));
}

struct ImmediateBridge {
    calls: AtomicUsize,
}
impl CommandBridge for ImmediateBridge {
    fn execute(
        &self,
        command: Command,
        _: &CallContext,
    ) -> Result<BridgeReceipt, host::CommandError> {
        use tanglewood::{
            commands::CommandResult,
            notes::{NoteId, NoteRecord, NoteView, Revision, SaveState},
        };
        let sequence = self.calls.fetch_add(1, Ordering::AcqRel);
        let id = NoteId(uuid::Uuid::from_u128(1));
        let revision = Revision(uuid::Uuid::from_u128(2));
        let text = match command {
            Command::CreateNote { path, text } if sequence == 0 && path == "Notes/test.md" => text,
            Command::EditNote {
                id: requested,
                expected,
                edits,
            } if sequence == 1 && requested == id && expected == revision && edits.len() == 1 => {
                format!("Compiled extension{}", edits[0].replacement)
            }
            Command::ReadNote { id: requested } if sequence == 2 && requested == id => {
                "Compiled extension edited".into()
            }
            _ => {
                return Err(host::CommandError::InvalidArgument(
                    "unexpected command".into(),
                ));
            }
        };
        Ok(BridgeReceipt {
            completion: host::Completion::Persisted,
            value: CommandResult::Note(NoteView {
                note: NoteRecord {
                    id,
                    revision,
                    path: "Notes/test.md".into(),
                    text,
                    trashed: false,
                },
                save: SaveState {
                    accepted: revision,
                    persisted: Some(revision),
                    error: None,
                },
            }),
        })
    }
    fn read_result(
        &self,
        _: String,
        _: Vec<host::PathElement>,
        _: u64,
        _: u32,
        _: &CallContext,
    ) -> Result<host::ResultNode, host::CommandError> {
        Err(host::CommandError::NotFound)
    }
    fn capabilities(&self, _: &CallContext) -> Result<Vec<host::Capability>, host::CommandError> {
        Ok(Vec::new())
    }
}

#[test]
fn compiled_guest_uses_typed_bridge_and_manifest_callback_limits() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/wasm-extensions/wasm32-wasip2/release/tanglewood_compiled_v2_guest.wasm");
    let bytes: Arc<[u8]> = std::fs::read(&path)
        .unwrap_or_else(|_| {
            panic!(
                "missing {}: run ./scripts/build-wasm-extensions.sh first",
                path.display()
            )
        })
        .into();
    let runtime = Runtime::new();
    let bridge = Arc::new(ImmediateBridge {
        calls: AtomicUsize::new(0),
    });
    let invocation = || Invocation::Compiled {
        input: "Notes/test.md".into(),
    };
    let result = runtime
        .start_component(bytes.clone(), invocation(), bridge.clone(), Limits::HOST)
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(
        result.output,
        "00000000-0000-0000-0000-000000000001:Compiled extension edited"
    );
    assert_eq!(bridge.calls.load(Ordering::Acquire), 3);

    bridge.calls.store(0, Ordering::Release);
    let one_call = Limits {
        max_calls: 1,
        ..Limits::HOST
    };
    let result = runtime
        .start_component(bytes.clone(), invocation(), bridge.clone(), one_call)
        .unwrap()
        .wait();
    assert!(matches!(result, Err(ExtensionError::ResourceExhausted(_))));
    assert_eq!(bridge.calls.load(Ordering::Acquire), 1);

    bridge.calls.store(0, Ordering::Release);
    let tiny_payload = Limits {
        max_host_bytes: 32,
        ..Limits::HOST
    };
    let result = runtime
        .start_component(bytes, invocation(), bridge.clone(), tiny_payload)
        .unwrap()
        .wait();
    assert!(matches!(result, Err(ExtensionError::ResourceExhausted(_))));
    assert_eq!(bridge.calls.load(Ordering::Acquire), 0);
}
