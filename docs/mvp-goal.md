# Goal prompt: complete the Tanglewood MVP

Act as the **Tanglewood PM agent** and orchestrate completion of the MVP described in `docs/plan/tanglewood-mvp.md`. Read `AGENTS.md`, `.agents/workflow.md`, and `.agents/roles/pm.md` first. Treat the plan as the architectural reference and this prompt as the execution scope. Deliver working software in small increments; do not stop after planning, scaffolding, or a local-editor-only demonstration.

This prompt starts implementation when explicitly invoked. Preparing or reviewing this file alone does not start the MVP goal.

## Team and operating mode

Use `gpt-6.1-sol` for all three roles:

- **PM:** `medium` reasoning. Own the dependency graph, assignments, contracts, integration, acceptance evidence, and final delivery.
- **Developer:** `low` reasoning, corresponding to the requested “small” setting. Delegate narrowly scoped implementation/fix tasks to one or more developers.
- **Reviewer:** `xhigh` reasoning. Independently review coherent changes for correctness, feasibility, scope, and important missing evidence.

Use the custom definitions in `.codex/agents/` where supported. Otherwise explicitly set model/effort and supply the corresponding `.agents/roles/*.md` prompt. Do not assume a full-history fork changes model settings. Verify the host's supported delegation controls; report an unavailable requested setting rather than silently substituting a model.

The root PM should orchestrate directly, not create a second PM to duplicate its work. Default to the PM plus two developers and one reviewer, subject to the host's actual limit. Use fewer agents when work is sequential. Developers/reviewers do not delegate further. Routine implementation decisions, focused validation, and local unsigned commits are authorized; external publication is not.

## Required outcome

Complete the plan's required increments 0–4, with these deliverables:

1. **Shared Rust core:** one small Cargo package, persistent note IDs, explicit revisions/errors, a single ECS owner, shared typed commands, and a headless execution path. Implemented commands need not all have GUI controls.
2. **Usable Linux and Android notes:** source editing, save/error status, create/read/rename/trash/restore commands, durable restart recovery, Markdown import/export, navigation, preview, links/backlinks, search, and the planned Linux directory mirror. Complete the Android input/lifecycle/storage feasibility check early.
3. **LAN sync:** CRDT migration and persistence, offline concurrent edits, authenticated pairing, reconnect, and recoverable path/delete conflicts. Follow the plan's foreground-only Android scope.
4. **Linux MCP and Codex:** the single-writer MCP bridge and Codex app-server adapter/panel, including streamed turns, revision-checked note tools, cancellation, approvals, and failure handling.
5. **Extension accommodation:** reader/filter/render contracts, fence attributes, source-origin rules, and the small Pandoc-compatible example. Keep canonical Markdown intact and preserve a source fallback.
6. **Delivery material:** reproducible Linux/Android build and launch instructions, configuration/permission notes, an accurate capability table, and concise acceptance evidence.

The plan's optional D-Bus adapter and external-filter runner remain independent optional tracks. At kickoff record whether each is included or deferred, with a reason; they must not block required increments. If included, implement them behind optional features, reuse shared contracts, and provide their targeted evidence. Do not claim they are supported merely because their interfaces are documented. Block editing, general plugin management, arbitrary plugin UI, and the plan's other deferred features remain outside this goal.

## Orchestration loop

1. Inspect the repository, Git state, available toolchain/device access, and existing implementation. Preserve prior work. Create or update the PM-owned `docs/mvp-status.md` with required/optional scope, task dependencies, ownership, evidence, risks, and next actions.
2. Establish the small shared contract checkpoint. In parallel, delegate the Android feasibility spike when tooling permits. Pin working versions based on evidence; surface platform blockers before expanding dependent work.
3. Assign disjoint module work from the plan's concurrency table. Give every task an ID, input contracts, owned files, acceptance behavior, a validation budget, and a handoff requirement. The PM owns shared files such as `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/app.rs`, and `src/commands.rs` unless it explicitly transfers ownership.
4. Keep useful work moving: storage and UI can develop against agreed core contracts; MCP can precede sync; command capabilities can precede GUI exposure. Avoid parallel tasks that both need to redesign the same contract.
5. Obtain a developer handoff, freeze the review scope, and ask the reviewer for one combined correctness/scope review of the coherent increment. Resolve concrete blocking findings through a bounded fix task; re-review only changed risk areas.
6. Integrate, run the relevant combined checks once, update evidence/capabilities, and make an unsigned checkpoint commit. Proceed to the next unblocked increment without asking for routine confirmation.
7. Repeat until required acceptance gates are satisfied. Preserve a compact resume point in `docs/mvp-status.md` across interruptions; record incomplete work honestly rather than redefining completion.

## Concurrency and testing discipline

Follow `.agents/workflow.md`: one writer per file, one owner of the shared index, coordinated dependency changes, and no destructive cleanup of other workers' edits. Explicitly authorize any worktree isolation and its integration path. Do not allocate agents just to keep all slots busy.

Follow the plan's lean testing policy. Prefer existing focused checks and a small number of meaningful domain/durability/merge scenarios. Do not repeat the same domain test through GUI, MCP, D-Bus, AI, and plugins. Reviewers should consume recorded evidence before rerunning checks. Share the exact command, result, relevant revision/files, and skipped validations. Passing relevant checks is the stopping point, not a reason to run larger suites automatically.

## Commits and completion

Use **unsigned commits only**: `git -c commit.gpgsign=false commit --no-gpg-sign -m "..."`. The PM stages explicit integration paths and owns commits in the shared checkout. Keep normal hooks enabled. Do not push, publish, or create release artifacts on external services without authorization.

The goal is complete only when required behavior is implemented, integrated, reviewed, and backed by the relevant acceptance evidence. Confirm acknowledged saves survive restart, shared commands behave consistently, sync preserves recoverable content, and adapter failures leave editing usable. Include the Android and live integration evidence actually obtained.

If a required device, credential, service, model setting, or external dependency is unavailable, explain the exact blocker, finish independent work, and record the remaining acceptance step. Do not claim a complete MVP with required gates still unverified. Any goal-tracking tool must follow its own status rules; this prompt does not override them.

Finish with a concise delivery report: implemented capabilities, launch instructions, validation performed, optional/deferred items, genuine limitations, and unsigned commit IDs.
