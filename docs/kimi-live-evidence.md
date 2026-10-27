# Kimi ACP evidence

The implementation targets Kimi Code CLI 2.2.0 and ACP protocol 1. The
metadata-only harness creates its own temporary vault, working directory and
Kimi session, exercises the advertised model configuration without sending a
model prompt, restores the original model, and closes/deletes only its own session.

```sh
RUSTC_WRAPPER= cargo build --locked --bin tanglewood-mcp --example kimi_smoke --features kimi
RUSTC_WRAPPER= cargo run --locked --example kimi_smoke --features kimi
```

The optional `--live` argument sends two bounded model prompts using the existing
login and the temporary vault. It reports stream, tool, permission and cancellation
observations separately and retains diagnostic information on failure. Review the
output before claiming a live behavior was observed. It is not run by the metadata
phase.

An initial installed metadata run revealed that Kimi sends a full configuration
update before the matching set response. That first run did not restore the model
or clean up through its harness; its disposable session was separately verified
against the temporary working directory, deleted, and confirmed absent from the
session list. The implementation now keeps a configuration mutation pending until
its corresponding response. The corrected metadata-only run passed on Kimi 2.2.0: it selected the advertised
K2.7 Code Highspeed model, observed its Thinking On and Thinking Max choices,
restored the original model from the correlated response, and closed and deleted
its disposable session. No model prompt was sent.

No Android support, network isolation, live model response, live cancellation, or
native accessibility result follows from compilation or deterministic fixtures.


## Installed live acceptance

The integrator built the combined `gui,codex,kimi` binaries and example, then ran
`target/debug/examples/kimi_smoke --live` once after explicit user approval to send
two short prompts and synthetic `before`/`after` note text to Kimi. Kimi 2.2.0:

- Reported 144 streamed text chunks and 155 tool activity events.
- Read, revision-edited and reread the temporary note through Tanglewood MCP.
- Confirmed the `after` text persisted and survived closing/reopening the vault.
- Returned terminal `cancelled` after Stop on the second prompt.
- Closed/deleted the created session. Temporary diagnostics were removed on success.

No permission request occurred. No rejected mutation event was observed before
Stop. These are **not** live proofs of permission UI or owner-policy rejection.
The harness restarts its owner for persistence/revocation; it does not instantiate
the planned second simultaneous live MCP client. The separate concurrent-client
regression is the evidence for overlapping request IDs, independent replies,
competing revisions, live shared policy revocation and disconnect isolation.

The first attempt to launch this live check was blocked by automatic approval
review for external payload egress. No prompt ran on that attempt. The user then
explicitly authorized the synthetic payload and the single bounded run above.

## Native desktop acceptance

The actual combined binary ran in isolated Xvfb with a disposable vault/context.
At 1080×760, the Kimi pane connected, created its own session, displayed nine
server-advertised model choices, and switched to K2.7 Code Highspeed. The composer
showed the current server model/thinking/mode controls. A synthetic unsent draft
remained visible after intentional disconnect.

At 600×700 the composer controls wrapped, navigation was reachable through an
overlay, and the note source editor remained usable after Kimi teardown. No model
prompt was submitted through the native UI. The test's sole native session was
closed/deleted separately and confirmed absent from the unique context listing.

Screenshots from this run are in `/tmp/tanglewood-kimi-native-*.png`, including
`models`, `disconnect-draft`, `compact`, `overlay` and `editor`. The isolated test
window was destroyed by X automation at cleanup, causing winit's X11 bad-window
panic; that action does not test the app's ordinary close path. No user desktop
window or existing Kimi session was touched.

This initial evidence precedes the review-cycle-1 fixes. Those fixes require
focused regressions and independent rereview; they do not justify repeating paid
prompts merely to recreate incidental permission or rejection events.


## Review fixes and follow-up checks

Cycle 1 identified five recovery/correlation/tool-update/cancellation issues.
The same Terra developer fixed them; focused Kimi worker tests (2) and panel tests
(2) passed, including a real worker preflight correlation and a descendant holding
an inherited pipe. A combined `gui,codex,kimi` UI run then passed all 37 tests, and
the combined bins/example build passed.

A follow-up native run with both providers confirmed the header provider menu,
switch from Codex to Kimi, connection, and ordinary intentional disconnect. The
persistent privilege paragraph is now in connection help. No new native session
or model prompt was created in this run. Screenshots:
`/tmp/tanglewood-kimi-final-provider-menu.png`, `final-connected.png` and
`final-disconnected.png` (all with the `tanglewood-kimi-` prefix).

Cycle 2 confirmed the five fixes and identified two remaining issues: same-session
reopen with a retained fresh draft, and an unnecessarily low 32-call tool cache.
These require a focused fix and review cycle 3.

The initial combined library run had 102 passes, 6 failures and 2 ignored tests.
Four failures were socket/network permission restrictions; two restart tests
reported database locks during parallel execution. Each affected test passed
individually with socket access and `--test-threads=1`:
`owner_socket_routes_policy_revision_and_durable_mutations`,
`same_socket_grants_and_revokes_before_queued_dispatch`,
`simultaneous_clients_preserve_revisions_and_survive_other_disconnects`,
`desktop_batch_intents_are_durable_and_invalid_jobs_never_persist`,
`codex_project_cas_survives_restart_and_rejects_identity_changes`, and
`landing_download_retains_pdf_and_final_provenance`.
The original combined command itself is not claimed as passing.


## Final review checkpoint

The same Sol 6.1 xhigh reviewer marked the frozen implementation **READY in cycle
3 of at most 4**, after the same GPT-6.1 Terra max developer corrected all seven
concrete findings from cycles 1 and 2. Final focused panel tests passed (3), as did
worker tests (2), including 256 distinct tool calls, subsequent partial merge and
257th-call rejection. The final combined desktop binaries built successfully.

The installed native reconnect test then created another disposable session,
entered a synthetic unsent draft, disconnected, and reconnected. Automatic
session list/load returned **Ready**, preserving the exact draft with enabled
model/thinking/mode and Send controls. No model prompt was sent. Screenshot:
`/tmp/tanglewood-kimi-final-reconnect.png`. The session was closed/deleted and confirmed absent from the unique test context.
This closes the actual same-session recovery check; it does not imply native coverage of every error branch.

The user authorized local commit and merge of the reviewed `codex/kimi-acp` work.
No remote publication is included. Historical activity replay and the live
permission/rejection observations retain the limits described above.
