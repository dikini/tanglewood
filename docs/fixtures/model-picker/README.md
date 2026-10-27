# Local model-picker protocol fixture

`agent.py` is a deterministic local JSONL fixture for the Tanglewood Codex and
Kimi adapters. It never reads credentials or makes network requests. Invoke it
through `codex` and `kimi` executable wrappers; the Kimi adapter passes `acp`,
while the Codex adapter first calls `--version` and then `app-server`.

Set `TW_FAKE_AUDIT` to a writable JSONL path to record only provider, method and
session ID. Set `TW_FAKE_STATE` to a private writable JSON file to share fixture
sessions between processes. `TW_FAKE_BEHAVIOR` accepts `normal`, `slow_new`,
`malformed_config`, `cleanup_failure`, `long_stream`, `permission`,
`fresh_empty_pages`, `missing_new_id`, `disconnect_new`, `open_failed_new` or
`no_new_reply` or `model_effort_change`. The stream and approval modes simulate delayed updates, cancellation and a local approval request; they never contact an agent service.
