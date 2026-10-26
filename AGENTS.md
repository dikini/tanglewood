# Tanglewood project instructions

## Context and scope

- Read `docs/plan/tanglewood-mvp.md` before architecture or implementation work; it is the product and technical reference.
- `docs/mvp-goal.md` is the executable MVP goal prompt. Execute it only when the user asks to start/resume that goal; its presence does not authorize starting implementation during documentation work.
- Preserve the plan's incremental delivery, optional capabilities, and testing stopping rules. Do not silently drop required deliverables or promote deferred features into release gates.
- Prefer a single Rust package, standalone ECS, egui direct-style rendering, explicit shared commands, and small concrete modules. Command implementation and GUI exposure are independent.
- Parsed Markdown uses the approved document sub-world model in `docs/plans/2026-10-26-document-subworlds.md`: one current cached sub-world per note, inline runs within block components, view-local layout/focus, and immutable older generations for active drafts.
- Filters transform derived views; source changes use revision-checked commands. MCP, optional D-Bus, AI, and future plugin actions share domain validation and dispatch.

## Approved GUI direction

- Terminology: **styling** means presentation (fonts, colors, spacing, layout); **formatting** means changing Markdown markup through source-edit commands. Styling does not modify authored source.
- The user-approved UI reference is `docs/design/tanglewood-quiet-desktop.html`; implementation rules are in `docs/plans/2026-10-26-approved-egui-shell.md`. Read them before GUI work.
- Preserve its Codex-inspired quiet rail/sidebar/header, centered document and contextual source editing. Prefer frameless icons with tooltips/accessibility labels; secondary actions live in menus/settings.
- Do not reintroduce always-visible numbered block-edit lists, processing/debug configuration, rename/trash forms, metadata explanations or routine status rows into the document surface. Validate visible results against the approved reference, not compilation alone.

## Codex roles

| Role | Model | Reasoning | Instructions |
| --- | --- | --- | --- |
| `pm` | `gpt-6.1-sol` | `medium` | `.agents/roles/pm.md` |
| `developer` | `gpt-6.1-sol` | `low` | `.agents/roles/developer.md` |
| `reviewer` | `gpt-6.1-sol` | `xhigh` | `.agents/roles/reviewer.md` |

The user's developer setting “small” maps to Codex reasoning effort `low`; it is not a separate model. Loadable custom agents live in `.codex/agents/`. `.agents/README.md` explains setup and the fallback for hosts with different delegation tools.

For an authorized MVP run, the root agent is the PM and explicitly delegates bounded implementation tasks and independent reviews. Use the requested settings; do not silently substitute models. Use subagents within the current task, not new user-visible chats. Developers and reviewers do not recursively spawn agents. Ordinary small follow-ups do not require a team.

## Concurrent work

- Follow `.agents/workflow.md`. The PM owns the task board, shared interfaces, integration, and the shared Git index.
- Assign disjoint file ownership before concurrent writes. One writer per file; shared-file changes go through the PM. Reading another agent's files is allowed.
- Shared workspaces are the default: edits are immediately visible. Do not reset, stash, clean, revert, or overwrite another agent's work. Pause the affected task and coordinate if changes overlap.
- Freeze the files under review. Other agents may continue in unrelated files. A review of an older version does not approve newer changes.
- Serialize dependency/lockfile updates, broad formatting, integration checks, and commits. Avoid duplicate Cargo jobs contending for the same build directory.
- Worktrees are optional when ownership cannot provide isolation. The PM assigns them, starting refs, and integration order; workers never invent independent branch/merge workflows.

## Validation and review

- Use the plan's testing policy: the smallest meaningful check, a regression test only for an uncovered behavior, and no repeated domain tests through each adapter.
- No Cargo checks for prose-only changes. No test-per-function mandate, coverage quota, repeated full suites, or broad feature/device matrix.
- At a coherent integration checkpoint, the PM runs the relevant combined checks once. Re-run only when changes or evidence invalidate the result.
- Review correctness, data preservation, concurrency boundaries, contract compatibility, and scope. Style preferences and speculative tests are not blockers.
- Report commands, results, and limitations accurately. Android, networking, and live AI checks are not proven by host compilation or mocks.

## Git: unsigned commits only

- When committing is authorized, every agent must use `git -c commit.gpgsign=false commit --no-gpg-sign ...`. Never use `-S` or request a signing key.
- In a shared checkout, only the PM/integrator stages or commits. Workers return their changes and handoff report without touching the index.
- Stage explicit owned paths; inspect `git diff --cached` before committing. Never use broad staging to sweep in unrelated work.
- Keep normal hooks enabled. Do not change global Git configuration, amend others' commits, force-push, or publish without user authorization.
- When explicitly using cherry-pick or merge, disable signing for those operations too: `git -c commit.gpgsign=false cherry-pick --no-gpg-sign ...` or `git -c commit.gpgsign=false merge --no-gpg-sign ...`.
- Verify unsigned commits with `git log -1 --format='%h %G? %s'`; `%G?` must be `N`.

## Communication

Explain completed behavior, next dependency, and real blockers briefly. Ask only for missing decisions that materially affect scope or irreversible actions; continue independent authorized work. Keep handoffs and progress records concise enough to resume without rereading the entire chat.
