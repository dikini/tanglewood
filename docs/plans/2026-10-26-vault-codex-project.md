# Vault-associated Codex project

Approved workflow: Notes and Codex activity icons switch the same left sidebar. Codex shows only the current vault's project and its threads; selecting a thread shows its conversation in the main area. Restore note/draft/scroll state when returning. Project name is editable through its menu. Persist actual project identity with vault metadata. Scope: one project, paginated thread list/history, new thread, resume, rename. No all-project browser, bulk imports, model turns on selection, or Android integration.

Use experimental project API from installed codex-cli0.163.0 generated schema (`project/create`, `project/read`, `project/update`, `thread/list` projectId filter, `thread/start` assignment). Enable experimental capability only for project mode, retain legacy adapter path. Persist stable project creation idempotency key before network creation; retain it on restart and attach returned ID durably. Renames preserve identity; never silently replace missing project. Only explicitly selected project-listed threads may resume; returned association and existing sandbox/cwd/approval configuration are checked.

Tasks and ownership (shared checkout):
- CP1 developer codex_projects owns src/codex.rs: bounded protocol requests/events, paging/history, fixtures.
- CP2 developer vault_codex_metadata owns notes/commands/app/store: singleton binding metadata, local CAS command, durable acknowledgements, restart fixture.
- CP3 root owns ui.rs/main.rs/examples/mcp/docs/shared integration: sidebar/main switching, lifecycle and persistence wiring, validation.
- Independent reviewer reads frozen integration after handoffs. Cargo jobs serialized. No commit unless requested. Prior comment/highlight changes remain intact.

Validation: focused protocol and metadata regressions, combined gui,codex build/tests; native Linux visual/keyboard smoke; actual app-server project create/list/rename/resume without generation if authenticated runtime available. Report exact live limitations. No paid turn needed for project management.

Delivery: CP1/CP2/CP3 implemented. Header connect/disconnect and new-thread icons,
project-only sidebar, paginated history, durable rename/identity and bounded
per-thread draft preservation are integrated. Thread state uses compact symbols
with explanatory tooltips/accessibility labels; list snapshots plus pushed
`thread/status/changed` events avoid polling. Unknown/unloaded stay distinct.
See `docs/codex-live-evidence.md` for real project create/rename/restart, one
scoped model reply, history resume, note-draft preservation and final Linux
capture. Combined tests: 34 passed, 1 optional Pandoc fixture ignored; binaries
and all-target Clippy completed with warnings. No commit made.
Independent final delta review: READY; earlier prompt-ownership findings closed.
No correctness blockers remain. Intentional disconnect currently retains the
existing diagnostic message presentation; it does not prevent reconnect.
