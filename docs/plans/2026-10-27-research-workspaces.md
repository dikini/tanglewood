# Research Workspaces Implementation Plan

**Goal:** Implement the approved explicitly added-source research workflow while reserving discovery and richer output adapters.

**Architecture:** Single App owner, revision-checked Research commands and existing SQLite worker; immutable evidence snapshots and a bounded import worker. Quiet egui research destination prepares context for the existing optional agent interface.

**Tech Stack:** Existing Rust, egui, SQLite, Serde; local Poppler pdftotext for Linux PDF extraction. No new framework or network service.

Follow the approved design in `2026-10-27-research-workspaces-design.md`, project workflow and minimal testing policy. Subagents execute in the current shared checkout; only integrator handles shared dependency/index changes. Preserve pre-existing uncommitted changes.

## R1: Domain and storage

Ownership: developer research_core; `src/research.rs`, `src/app.rs`, `src/commands.rs`, `src/store.rs`, `src/lib.rs`. Root owns Cargo dependency adjustment.

- Define source/page/provenance, workspace, separate human/agent assessment and command/result records.
- Validate bounded title/question/source payloads and CAS workspace changes; enforce trusted caller policy.
- Capture notes from App state with exact ID/revision; retain immutable snapshots across later edits.
- Integrate workspace persistence with the existing worker, acknowledgements/errors and restart.
- Add context construction with stable citations, selected sources only, no network, untrusted-document guard and explicit budget errors. Discovery returns Unavailable.
- Focused real App/SQLite scenario proves restart, source immutability, CAS and policy. Coordinate Cargo execution with root.

## R2: Import worker

Ownership: developer research_import; `src/research_import.rs`. Root registers module.

- Read explicitly selected regular text/PDF files off thread with bounded jobs and results.
- Spawn pdftotext directly on Linux, retain form-feed page positions, bound bytes/pages/process time and cancellation.
- Return owned DTOs, actionable missing tool/empty scan/OCR/invalid text errors; no UI/domain mutation in worker.
- Focused extraction/page parsing and real temporary-file checks; exercise an actual PDF during integration.

## R3: Native workflow

Ownership: developer research_ui; `src/ui.rs`, `src/ui/research.rs`.

- Add Research rail/workspace sidebar and contextual creation, source addition, selection and evidence/assessment view.
- Associate pending imports with explicit workspace/revision. Preserve drafts on conflicts or navigation.
- Open/create/link ordinary research notes using existing commands; keep source edits independent.
- Offer editable questions/continuations/overview/report requests; prepare optional agent context without sending or enabling network and without overwriting drafts. Provide copy fallback.
- Preserve quiet shell and one-surface narrow layout; discovery/richer outputs remain honestly deferred.

## R4: Integration and review

Ownership: root plus independent Sol 6.1 xhigh reviewer(s); frozen changed scopes.

- Review scope compliance, source/data preservation, CAS/durability, import lifecycle/bounds, caller policy and agent draft ownership.
- Run relevant headless and gui,codex tests, formatting and all-target Clippy once; repair focused failures and recheck affected risks.
- Build/run isolated Linux vault; create research workspace, add note and real PDF, inspect page/source and agent draft, restart and confirm saved data. Capture actual interface.
- Record exact commands/results/limitations in `docs/research.md` and task board; do not claim Android, discovery, OCR or live generation without actual evidence.

## Task board

| Task | Owner | State | Next dependency |
| --- | --- | --- | --- |
| R1 | research_core | done | Focused SQLite/CAS/snapshot/restart evidence |
| R2 | research_import | done | Real native PDF import and page/bounds fixtures |
| R3 | research_ui | done | Native workspace/notes/citation flow and draft regressions |
| R4 | root/reviewers | done | Independent READY; tests/build/native evidence in docs/research.md |
