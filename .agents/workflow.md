# Concurrent implementation workflow

## Ownership and scheduling

The PM is the only scheduler and integrator. Keep a small `docs/mvp-status.md` task board: task ID, scope/dependency, owner, owned paths, status, evidence, and next action. Use `ready`, `active`, `review`, `done`, or `blocked` as task-board labels; these are not instructions to change a host goal's status.

Default to the root PM plus up to two developers and one reviewer. Use fewer workers for sequential work. Keep the reviewer busy with a stable completed increment while developers work on unrelated modules. Do not spawn separate agents for every file or trivial fix.

Before assigning concurrent writes, grant each task a disjoint set of files. The PM initially owns `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/app.rs`, `src/commands.rs`, and the status/capability records. It can transfer a shared file to one worker for a task, suspending other writes until handback. Requests for shared changes should state the exact API/dependency change and why; do not work around a missing contract with duplicate implementations.

## Task brief

Give each developer a compact assignment:

```text
Task ID and intended behavior:
Repository/worktree and starting state:
Relevant plan sections and agreed input/output contracts:
Owned files; shared files requiring PM coordination:
Dependencies and explicit non-goals:
Acceptance behavior:
Smallest useful validation and any slow checks to defer:
Handoff expected; no shared-index writes or commits:
```

Workers read current files before editing. Shared-workspace edits are immediately visible; another worker's partial changes are not an invitation to fix them. If an overlap or incompatible interface appears, pause that file and notify the PM while continuing independent work. Never reset, stash, clean, or revert someone else's changes.

Use worktrees only when disjoint ownership cannot provide practical isolation. The PM explicitly assigns a managed worktree, starting ref, optional `codex/` branch, and integration order. In an isolated worktree a developer may make unsigned commits only when the assignment grants it. Never share a worktree/index between independent Git operations.

## Handoff and review

A developer reports behavior delivered, files changed, contract changes, exact checks/results, unverified cases, and risks. In a shared checkout leave changes for the PM; do not stage or commit. State if the task is partial.

The PM identifies a stable review scope: base commit plus owned diff or explicit files if there is no base. Freeze writes to those files for the review. Other workers may continue elsewhere. Give the reviewer the task acceptance criteria and developer evidence; distinguish relevant changes from unrelated worktree edits.

The reviewer performs one combined correctness, scope, and integration review. Findings need file/line, concrete failure condition, impact, and a minimal correction. Prioritize data loss, stale writes, broken contracts, authorization boundaries, and real missing evidence. Do not demand extra abstractions, test counts, or aesthetic rewrites. An empty findings list is valid. Reviewers do not edit, format, stage, commit, or spawn agents.

The PM resolves blocking findings through a scoped fix. Re-review the fix's affected risks, not the entire increment by default. If reviewed files change, approval applies only to the unchanged portions until the delta is checked.

## Validation budget

Follow the plan's testing section. Reuse focused behavioral scenarios; add tests for meaningful uncovered regressions. Documentation changes need no Cargo runs. Do not run `cargo check`, build, tests, and Clippy in succession when one targeted check answers the current question.

Coordinate Cargo jobs sharing a target directory. One worker runs the relevant targeted check; others reuse its recorded evidence where applicable. The PM runs the necessary combined checks at integration, with only affected features. Never repeat a passing check without a relevant change, failure, or new risk. Device and live-service checks belong at their delivery gates or after changes to those boundaries.

## Integration and unsigned commits

The PM waits for the relevant handoffs, resolves shared-contract changes, verifies the combined result, and updates the status record. Acquire the integration/commit window before broad formatting or Git mutations. Other tasks may continue only in paths excluded from this integration; stop all writers if the change/check has project-wide effects.

In a shared checkout, only the PM touches the index. Check the current staged set, stage explicit task-owned paths, inspect `git diff --cached`, and run `git diff --cached --check`. Never sweep in unknown files or remove pre-existing staged changes without coordination.

Commit with `git -c commit.gpgsign=false commit --no-gpg-sign -m "..."`. Keep hooks enabled and verify `%G?` is `N` using `git log -1 --format='%h %G? %s'`. Use the same unsigned discipline for explicitly authorized merges/cherry-picks. Do not change global Git config or publish remotely as part of local integration.

On interruption, leave a short resume point: integrated commit, active ownership, outstanding findings, validation evidence, and next dependency. Required work that is unimplemented or unverified remains open; a successful local smoke is not proof of Android, sync, or live AI readiness.
