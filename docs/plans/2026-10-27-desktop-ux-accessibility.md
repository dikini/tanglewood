# Desktop UX and accessibility implementation plan

> **For the PM:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement
> this plan task by task, with the named PM, developer, and reviewer subagents below.

**Goal:** Address the seven priority desktop UX findings while preserving the quiet
shell, authored Markdown, retained drafts, and revision-checked commands.

**Architecture:** Keep direct egui rendering and view-local interaction state in
the existing Rust package. Reuse shared commands and the document sub-world model;
add a small layout module only for pane fitting and temporary pane state. Native
accessibility is a desktop feature integration plus explicit widget semantics.

**Tech stack:** Rust, egui/eframe 0.33.3, existing Lucide assets and bundled fonts,
eframe AccessKit integration, and the existing native layout fixtures.

## Scope and references

Status: priority implementation pass completed and independently reviewed,
2026-10-27; included in the local commit requested by the user. UX0–UX5 and UX9
are delivered with the native AT-SPI Click and Android limitations recorded in the
[evidence](../design/2026-10-27-desktop-ux-accessibility-evidence.md).
UX6–UX8 remain deferred; no publication is included.

- [Saved audit and finding IDs](../design/2026-10-27-desktop-ux-accessibility-audit.md).
- [Product and technical reference](../plan/tanglewood-mvp.md).
- [Approved quiet shell](2026-10-26-approved-egui-shell.md) and
  [visual reference](../design/tanglewood-quiet-desktop.html).
- [Proposed shared design system](../design-system/README.md).
- [Contextual editing](2026-10-26-contextual-block-editing.md) and
  [existing keyboard navigation](2026-10-26-block-keyboard-navigation.md).
- [Agent workflow](../../.agents/workflow.md).

Deliver the seven priority findings in bounded increments. Track the additional
per-screen recommendations in UX6–UX8 without making them prerequisites for the
priority fixes. The design-system proposal supplies starting values for this plan;
this work does not silently approve or implement its full mobile scope.

## Delivery approach

Use incremental changes to the existing shell. A copy-only pass would be faster
but leave pane and assistive-technology problems open. A complete design-system
rewrite would widen scope and obscure regression evidence. The incremental approach
allows visual and accessibility review after each coherent change.

Keep styling separate from Markdown formatting. Preserve exact original source
bindings, immutable draft generations, IME guards, selected workspace/thread, scroll,
undo identity, and accepted-versus-persisted status. Preparing an agent request must
continue to leave it unsent until the user explicitly sends it.

No new command framework, docking engine, editor engine, storage schema, AI provider,
network requirement, or document parsing rewrite is needed for the seven fixes.
Android delivery and optional capabilities retain their existing gates.

## Subagent team and ownership

Use subagents in this task, as requested, with the repository's exact role settings.

| Agent | Model and reasoning | Responsibility |
| --- | --- | --- |
| Root coordinator | Current conversation | Keep user scope and progress clear; delegate execution to one PM subagent and receive its delivery report. |
| `pm` subagent | `gpt-6.1-sol`, `medium` | Read the plan and PM role; own scheduling, contracts, the task board, shared-file integration, validation, and the shared Git index. |
| `developer` subagent, the requested dev role | `gpt-6.1-sol`, `low` | Implement one bounded assignment at a time; edit only assigned paths; return behavior and evidence. |
| `reviewer` subagent, the requested review role | `gpt-6.1-sol`, `xhigh` | Independently review frozen changes and evidence; report correctness, source-safety, focus, and scope findings without editing. |

Use `agent_type="pm"`, `agent_type="developer"`, and `agent_type="reviewer"`
when the host exposes these configured roles. If a fallback is required, use a fresh
context, the exact model/effort above, and the corresponding role prompt from
`.agents/roles/`. Report an unavailable setting rather than silently substituting.

The default active team is root + PM + one developer + reviewer, fitting all four
available slots. The PM may temporarily use two developers on disjoint files while
the reviewer slot is free; release that slot before requesting review. Developers
and reviewers never spawn agents. There is one PM and no nested PM delegation.

The PM retains `src/ui.rs`, `Cargo.toml`, `Cargo.lock`, the task/evidence records,
and any shared interfaces. It may temporarily transfer a shared file to one
developer, pausing other writes to that file until handback. Other UI modules receive
one named writer per assignment. Each brief states: "You are not alone in this
codebase. Preserve other agents' changes and coordinate overlaps with the PM."

Use the shared checkout unless ownership cannot provide isolation. Preserve existing
uncommitted work. Workers never stage, commit, reset, stash, clean, or switch branches.
Only the PM stages or commits when authorized, using explicit paths and unsigned
commits under `AGENTS.md`; no publication is included in this plan.

## Task board and dependency order

The table below preserves the planned delivery order; UX0–UX5 and UX9 are now
complete with qualified evidence, while UX6–UX8 remain deferred. The PM records ownership and status in
`docs/mvp-status.md` without changing the status of unrelated MVP work. Use the
workflow's ready/active/review/done/blocked labels and retain evidence links.

| Task | Priority | Findings | Dependency | Deliverable |
| --- | --- | --- | --- | --- |
| UX0 | First | All | None | Current baseline, reproducible scenarios, and scope assignment |
| UX1 | High | A1 | UX0 | Native accessibility bridge and meaningful control semantics |
| UX2 | High | L1 | UX0 | Pane fitting, reachable temporary panes, and correct transcript viewport |
| UX3 | High | L2, A2 | UX1 and UX2 shared contracts | Source identity, distinct state cues, and legible focus/selection |
| UX4 | High | L3, C1 | UX1; stable layout from UX2 | Clear active editing and honest apply/export wording |
| UX5 | High | C2 | UX1; UX3 handback of research module | Understandable recovery actions preserving conflict behavior |
| UX6 | Follow-up | Per-screen navigation/settings/files | Priority fix review | Search feedback, settings organization, and task-focused file dialogs |
| UX7 | Follow-up | Per-screen conversation/context | Priority fix review | Better empty states, capabilities, and compact menus |
| UX8 | Follow-up | Per-screen source reader | Priority fix review | PDF readability and separately scoped derived-text reading view |
| UX9 | Each checkpoint | Completed task set | Stable handoffs | Independent review, combined checks, and evidence record |

UX9 runs after each coherent checkpoint, not only after all follow-ups. Suggested
checkpoints are UX1, UX2–UX3, and UX4–UX5. UX6–UX8 can ship independently later.

## UX0 Capture the current baseline

**Owner:** PM, with a read-only developer exploration assignment if useful.

**Read:** `src/ui.rs`, `src/ui/theme.rs`, `src/ui/editor.rs`,
`src/ui/navigation.rs`, `src/ui/research.rs`, `src/ui/codex_context.rs`,
`src/ui/processing.rs`, `Cargo.toml`, and the references above.

**Create when executing:** `docs/design/2026-10-27-desktop-ux-accessibility-evidence.md`.

1. Record the base commit and relevant existing dirty paths; identify current
   ownership before assigning writes. Do not clean the checkout.
2. Open local note, transcript, and research fixtures in a disposable vault. Use
   existing `examples/transcript_demo.rs` and `examples/research_demo.rs` for layout;
   these do not require a live agent or provider turn.
3. Reproduce pane crowding, a long source name, short/long source drafts, a retained
   conflict, and keyboard pane transitions. Inspect the transcript at initial load,
   after scrolling, and after resizing. Classify intentional scroll clipping
   separately from an incorrectly allocated viewport.
4. Inspect the resolved desktop feature graph and native accessibility tree.
   Preserve existing working focus outlines, F6 entry, arrow traversal, and
   source-safe editing instead of treating old captures as the full current state.
5. Record the smallest changes required and assign paths. Confirm sample fixtures
   contain similar source titles, Unicode, a selected source without full text, an
   excluded source with text, and one failed acquisition.

**Acceptance:** Every priority finding has a current reproduction or a qualified
"already satisfied / requires runtime evidence" result. No product behavior changes
or claim of remediation occurs from this baseline task alone.

## UX1 Enable native accessibility and name controls

**Owner:** PM owns manifest/lockfile edits and shell integration; developer owns
the specifically assigned labeling changes in UI modules. Review the bridge first.

**Modify:** `Cargo.toml`, `Cargo.lock` only if resolution changes, `src/ui.rs`
(`field_single`, dialogs, search, composer), `src/ui/theme.rs` (icon semantics),
`src/ui/research.rs` (source rows and fields), and `src/ui/codex_context.rs`.
Serialize edits or split by file, never by overlapping line ranges.

1. Inspect `cargo tree --locked --offline --features gui,codex -e features -i eframe`.
   Enable eframe's `accesskit` feature on desktop if the resolved graph lacks it;
   retain disabled defaults and existing graphics/platform features.
2. Associate visible labels with their fields. Keep stable widget IDs separate from
   human-readable accessible names; an internal name such as `quick-switch` is not
   an adequate spoken field label. Name search, Markdown fields, and the composer.
3. Name each inclusion checkbox with its source title and purpose, for example
   "Include Workshop constraints in research context." Expose checked, selected,
   expanded, enabled, and busy states using the supported native semantics.
4. Ensure icon actions, Send/Stop, page navigation, and unlabeled removal controls
   have useful names. Avoid duplicate announcements from decorative status symbols.
5. In an actual Linux desktop session, use Orca or the available native screen
   reader to traverse a note, a research source row, one dialog, and the composer.
   Confirm labels, state changes, reading order, and focus return.

**Acceptance:** The accessibility tree is exposed and the sampled workflow can be
understood and operated without hover. A passing build or feature graph alone is
not evidence of a working screen-reader workflow. If desktop tooling is unavailable,
record the bridge implementation separately from the still-unverified runtime gate.

## UX2 Fit panes and allocate the transcript viewport

**Owner:** Developer owns new layout helpers; PM owns `src/ui.rs` integration.

**Create if needed:** `src/ui/layout.rs`, containing only concrete pane-fitting
policy and view-local temporary-pane state, with focused tests in the same module.

**Modify:** `src/ui.rs` (`TanglewoodUi::update`, `ConversationPanel::show_surface`),
`src/ui/navigation.rs` for shared focus handling if necessary, and
`src/ui/theme.rs` only through its assigned owner.

1. Centralize the pane budget currently spread across notes, research, and Codex.
   Use the proposal's starting bands: compact below 600 logical points, regular
   below 1200, expanded otherwise. Default pointer sizes are rail 48, left pane
   228, right pane 260, and main minimum 480 for simultaneous columns. Count
   gutters/separators and actual scaled text; a fit failure overrides the band.
2. Undock right panes before left navigation. Keep user docking/visibility intent
   separate from automatic visibility. Resize must preserve drafts, selection,
   thread/workspace, scroll, and actively focused pane. Keep every undocked pane
   reachable through a named visible toggle.
3. Replace floating temporary-pane behavior where needed with an edge overlay,
   scrim, reachable Close action, and independent scrolling. Permit one temporary
   pane at a time. Opening moves focus inside; closing restores the invoker.
   Covered content cannot receive pointer or keyboard input. Escape/outside click
   dismisses through existing draft/IME guards without applying or sending work.
4. If UX0 reproduces the transcript defect, reserve header, local errors/approvals,
   and composer height before allocating the remaining scroll viewport. Keep
   controls reachable at short window heights and allow the composer to scroll
   internally once it reaches its bounded height. Preserve intentional user scroll.
5. Add a regression only for uncovered pane state/fitting or reproduced viewport
   behavior. Include focused-pane undocking and a user-closed pane staying closed.

**Acceptance:** At sampled narrow, regular, and expanded desktop sizes, the reader
wraps, Send/Stop remains reachable, all panes can be opened, dismissal restores
focus, and resizing retains the draft. A scrolled message may be clipped at the
viewport edge normally; content must not be permanently hidden under chrome.

## UX3 Identify sources and distinguish states

**Owner:** Developer owns `src/ui/research.rs` after UX1 handback. A separate
developer may own `src/ui/theme.rs` while PM coordinates the shell row changes.

**Modify:** `src/ui/research.rs` (`sidebar`, selected reader heading, `outcomes`),
`src/ui/theme.rs` (`palette`, selected/focused controls), and PM-owned note/thread
rows in `src/ui.rs` where the same issue is reproduced.

1. Give the selected source its full wrapping title in the reader/details area.
   Truncated rows expose the complete title on pointer hover and keyboard focus.
   Keep the checkbox and status area stable so titles do not push controls away.
2. Keep inclusion, text/original availability, and assessment distinct. Inclusion
   changes only through its checkbox. Name availability beside the selected source
   or in its details; retain compact symbols in the list with accessible meaning.
3. Add or preserve separate selected and keyboard-focused treatments. Reuse the
   existing gutter marker for reader focus. Introduce a small selected-row marker
   if the fill alone is insufficient; do not brighten every separator.
4. Measure actual foreground/background pairs in both palettes. Target at least
   4.5:1 for ordinary text and 3:1 for meaningful state/focus indicators. Use existing
   semantic color helpers; raise meaningful small labels toward the proposal's
   12-point secondary role without enlarging every decorative label.
5. Shorten the Outcomes action to "Create output…"; keep all existing formats in
   the next dialog. Do not add repeated status explanations to the document body.

**Acceptance:** A user can distinguish two similarly named sources, included versus
excluded sources, and reference-only versus readable material using pointer,
keyboard, and the native accessibility tree. Focus remains distinct from selection
after moving into the reader. Record measured contrast pairs rather than claiming
the current palette necessarily fails.

## UX4 Clarify active editing and completion copy

**Owner:** Developer owns `src/ui/editor.rs` and any assigned presentation change in
`src/ui/processing.rs`; PM owns dialog/export integration in `src/ui.rs`.

**Modify:** `src/ui/editor.rs` (`source_editor`), `src/ui/processing.rs` (inline
editor placement only), `src/ui.rs` (source dialog and Files actions), and
`src/ui/theme.rs` only through its owner.

1. Give only the active draft a restrained editing indication. Preserve reader
   gutter focus and ordinary block styling. Keep the contextual actions directly
   below the inline draft; bound large source dialogs to the usable viewport with
   a reachable action footer. Do not add permanent block cards or an edit list.
2. Rename the check action's tooltip and accessible name to "Apply changes";
   retain "Cancel changes." Keep actions discoverable on focus while editing.
3. Use "Export current Markdown…" consistently, with "Unapplied edits aren't
   included." Reserve "Saved" for the matching persistence acknowledgement; applying
   a draft must not claim durable storage. Keep export and revision semantics intact.
4. Exercise a short heading, a growing paragraph, and a large custom fence.
   Reuse the existing `ready_block_preserves_adjacent_bytes_and_cancel_conflict`
   and source-editor scenarios. Add a layout regression only if newly uncovered.

**Acceptance:** Reading versus editing is visually clear, Apply/Cancel stays
reachable, Cancel leaves canonical bytes unchanged, Apply changes only the verified
range, and a conflict or active composition still prevents unsafe application.

## UX5 Make recovery decisions understandable

**Owner:** Developer owns `src/ui/research.rs` after UX3 handback; PM owns any shared
shell/file recovery wording in `src/ui.rs`.

**Modify:** `src/ui/research.rs` (assessment/workspace/checkpoint conflicts and
retained import recovery); `src/ui.rs` only for the corresponding file/save wording.

1. Inventory the current button effects before renaming them. Distinguish retaining
   a draft, adopting a latest revision, retrying an operation, and discarding input.
2. Use task-specific labels: "Keep my assessment and retry" only for an actual
   retry, "Review latest version" for adopting/reviewing changed context, and
   "Discard my assessment edits" for the deliberate destructive choice.
3. Put a short explanation beside the affected draft: "This source changed while
   you were editing. Review the latest version before applying your assessment."
   Show the relevant latest context without overwriting the user's retained input.
4. Preserve revision checks and explicit retry semantics. A label change must not
   automatically retry a timed-out mutation or silently overwrite intervening work.
5. Exercise one stale-revision assessment and one retained-import failure/retry.
   Reuse existing draft preservation coverage; add a regression only if interaction
   changes create an uncovered data-preservation risk.

**Acceptance:** The user understands what will be kept, reviewed, retried, or
discarded. Failed retries retain inputs and show the error beside the task. Exact
button text accurately describes the command it invokes.

## UX6 Follow up on navigation settings and file tasks

**Files:** PM-owned `src/ui.rs` (search/backlinks sidebar, `SettingsPage`, note
dialogs, Files and status),
`src/ui/processing.rs` (`controls`), and the evidence record.

1. Keep the search query visible and distinguish "No notes match this search" from
   an empty vault. Backlinks should identify their current note and explain an
   empty result locally, without adding a permanent document footer.
2. Give Appearance a clearly named settings section while preserving existing
   theme preferences. Collapse advanced filter configuration without hiding errors.
3. Size Connections to its content and group agent permissions separately from LAN
   availability. Keep process access separate from permission to update vault data.
4. Explain note name and optional folder with a concrete path example. Group import,
   replacement, export, and mirror actions with their own fields and results.
5. If adding file-selection convenience, reuse an existing supported native picker
   and shared commands. Do not introduce a new I/O path or merge import/replace.

**Acceptance:** A first-time user can understand an empty search, create/rename a
note, locate Appearance, and complete an export without decoding implementation
terminology. Existing actions remain reachable. Validate changed sidebars and
dialogs visually; copy-only edits need no new Cargo tests.

## UX7 Follow up on conversation and context

**Files:** PM-owned `src/ui.rs` (`show_surface`, `model_picker`, `turn_pickers`,
thread dialogs), and `src/ui/codex_context.rs` (`panel` and dialogs).

1. Add one useful empty-state action that prepares a draft without sending it.
2. Group composer controls logically; keep the current model/mode visible and Send
   reachable. Clamp menus to the viewport without silently changing selections.
3. Lead the Plugins tab with usable capabilities and clear availability; put raw
   authentication/tool diagnostics behind details. Keep actionable errors visible.
4. Explain approval options using their actual adapter semantics. Preserve explicit
   permanent-removal confirmation and restore focus after thread menu/dialog closure.

**Acceptance:** A user can start a conversation, inspect context and permissions,
and change a model in a narrow window without losing the draft or submitting it.
Local fixtures prove layout only; they do not establish live provider behavior.

## UX8 Follow up on source-reader readability

**Files:** `src/ui/research.rs` (`source_reader`, `reader_body`, reader layout helpers).
Read `src/research_reader.rs` and `src/research_desktop.rs` for existing boundaries;
changes there require a separate bounded ownership assignment if actually needed.

1. Add explicit local PDF zoom/fit controls, or surface the existing original-opening
   action clearly. Preserve source ID, physical page, page navigation, and citations.
2. Treat extracted-text reflow as a separate bounded design task. A derived reading
   projection must preserve exact captured text and provide reliable mappings for
   search highlights, copied excerpts, and citations. Do not normalize stored text
   or guess offsets to make a page look cleaner.
3. Keep that reading-view task queued if safe mapping needs a wider parser/data
   change. Record its dependency explicitly; it is not a priority-fix release gate.

**Acceptance:** Dense PDFs can be enlarged without losing their page/citation
target. Any later reflow implementation demonstrates exact evidence preservation
before claiming the text-reading recommendation complete.

## UX9 Review and verification at each checkpoint

**Owner:** PM integrates; reviewer independently inspects frozen files.

**Update:** `docs/design/2026-10-27-desktop-ux-accessibility-evidence.md`, the task
board, and the audit checklist only after the corresponding evidence exists.

1. Receive the developer's handoff: behavior, owned paths, exact checks/results,
   unresolved risks, and contract changes. Freeze that file set before review.
2. Give the reviewer the baseline/owned diff, task acceptance criteria, screenshots,
   runtime accessibility evidence, and exact validation results. Unrelated files may
   continue changing; a new edit invalidates review only for its affected scope.
3. Review source/draft preservation, command compatibility, revision/durability
   semantics, focus containment/return, pane state retention, and optional feature
   gates. Require concrete failure conditions for blockers; style preferences and
   speculative tests are not blockers.
4. Resolve material findings through one scoped developer fix and focused re-review.
5. Run the needed combined checks once, serializing Cargo jobs. Reuse unchanged
   passing developer evidence rather than duplicating it. Stop after the relevant
   checks pass; rerun only for changes, failures, or unresolved concerns.

### Commands for implementation checkpoints

These commands describe the validation plan; actual commands and results from the
completed priority pass are recorded in the linked evidence.
Use the affected feature set; `lan` is included below only if shared LAN settings or
gated shell code changed. Otherwise omit it. If a required dependency is unavailable
offline, report that limitation rather than weakening the feature being checked.

```sh
cargo fmt --check
RUSTC_WRAPPER= cargo test --locked --lib --features gui,codex,lan --offline ui::
RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,codex,lan --offline
```

Expected: no formatting errors, relevant UI tests pass, and no new blocking Clippy
diagnostics. Run the baseline fast library checks from the MVP testing policy once
at the combined integration gate when required; do not repeat them per UI task.

Launch local fixtures sequentially for visual evidence; each opens a window:

```sh
RUSTC_WRAPPER= cargo run --locked --offline --features gui,codex --example transcript_demo
RUSTC_WRAPPER= cargo run --locked --offline --features gui,codex --example research_demo -- /tmp/tanglewood-desktop-ux-review
```

Use a fresh disposable directory for the research fixture. Exercise real note
editing separately with a disposable vault. Local fixtures need no provider turn,
and a visual capture does not replace the native screen-reader workflow.

### Representative desktop checks

Sample 390×844, 900×700, and 1440×900 logical points, plus a resize sweep around
599/600 and 1199/1200 for pane policy. Include one short-height case and one enlarged
text case. Use dark mode for the broad journey and light mode for affected contrast
pairs; avoid a full cross-product of every screen, width, theme, and feature.

- Note author: create/open → read → keyboard edit → Apply/Cancel → save feedback.
- Researcher: select/include source → read/search/zoom → follow citation → return
  to conversation → prepare output → review the unsent request.
- Recovery: retained assessment/import → latest context → explicit retry/discard.
- Accessibility: native names/states, selected versus focused rows, F6/Tab traversal,
  overlay containment, Escape dismissal, and focus return.
- Resize: preserve draft, selection, thread/workspace, and manual scroll while
  undocking and reopening each available pane.

Capture a small number of before/after images under `docs/design/desktop-ux/` and
link them from the evidence record. Check them against the approved quiet reference.

Shared-shell/editor changes also require the affected Android build and input/layout
smoke when they could invalidate existing evidence. Assign that verification to the
developer in a later slot; do not infer it from Linux. A desktop-only AccessKit
feature change does not create a new Android runtime gate. Keep missing evidence
explicit; do not declare the affected platform ready without it.

## Completion and resume point

The priority pass is complete when L1–L3, A1–A2, and C1–C2 have acceptance evidence,
the reviewer finds no unresolved material issue, and the PM records any platform
limitations accurately. An unverified native screen-reader path leaves A1 open even
if compilation succeeds. A finding disproved by current reproduction is recorded
as already satisfied with evidence, not as a fabricated implementation change.

UX6–UX8 remain individually tracked follow-ups until implemented or explicitly
rescheduled; completion of the seven fixes does not erase them.

**Next action:** Root delegates this plan and the repository PM role to a `pm`
subagent when implementation is requested. The PM starts UX0, assigns one bounded
UX1 developer task, and requests independent reviewer assessment at its handoff.
No code, goal execution, commit, or publication is authorized by the act of saving
this plan.
