# Quiet Chat Hierarchy Implementation Plan

**Goal:** Make messages, activity, errors and reported files distinguishable without adding a new chat engine or changing the approved notes styling.

**Architecture:** Render the existing `ConversationMessage`, `Activity`, `CommandExecution` and thread-scoped `ContextFile` records through ordinary egui widgets. Keep existing file loading, limits, generation checks and command dispatch. No new persistent model, dependency, font, polling loop, filesystem discovery or automatic preview reads.

**Tech Stack:** Existing Rust, egui/eframe, serde_json and the current bounded Codex adapter.

---

## Approved direction

User approved the quiet hierarchy on 2026-10-27: user bubbles, open assistant replies, compact activity rows labeled by type/status, and distinct artifact cards with name/type/Preview. The user also reported errors being parsed incorrectly and directed investigation to the morning Tanglewood session.

- User/assistant identity is visually and accessibly clear. Do not infer a final-answer phase that the current records do not expose.
- Activity labels distinguish Search, Tool, Files and Command. Show Running, Done, Failed, Stopped or explicit Outcome unknown (and preserve other unknown states honestly); terminal command failure is determined by nonzero exit code even if its lifecycle says completed. Details remain bounded, selectable and collapsed by default. Failure/uncertainty use text as well as color, without adding automatic retry.
- Show a small thread-level Files and artifacts section from reported metadata, with at most three cards and a reachable all-files action. Reuse cards in Context/Files. Filename/type are visible, full path discoverable, Preview explicitly uses the existing background reader and generation guards. Do not imply an arbitrary assistant link was saved, bind thread-wide files to an invented turn, or execute/render arbitrary HTML/SVG.
- Preserve body/code fonts, composer, scrolling, source/attachment safety and current icon assets. Distinction comes from role, spacing, a restrained event treatment and artifact card shape.

## Ownership and work

| Task | Owner | Owned paths | Acceptance |
| --- | --- | --- | --- |
| CH1 presentation and native fixture | developer | `src/ui.rs`, `src/ui/codex_context.rs` | Typed/status rows, clear roles, bounded reported artifact cards, working Preview; representative local fixture |
| CH2 error investigation/correction | root | `src/codex.rs` | Reproduce concrete structured/string error mistakes; correct bounded parsing at receipt; preserve raw details and scoping |
| CH3 integration/evidence | root | plan/status docs, native captures | One relevant combined check; actual native message/event/artifact/error and Preview evidence |
| CH4 independent review | reviewer | read-only frozen combined diff | Correct states, no false saved/success claims, bounds, generation safety and tight scope |
| CH5 topical icons | developer; root integrates assets/docs | `src/ui.rs`, `src/ui/theme.rs`, `src/ui/codex_context.rs`; pinned SVG subset | Small monochrome Search/magnifier, Tool/wrench, Command/terminal and Files/document icons to the left; preserve labels/disclosures and native narrow layout |

`Activity`/`CommandExecution` fields remain unchanged unless coordinated. CH2 can normalize the existing status and readable title from explicit error evidence; CH1 must not classify arbitrary prose by searching for the word error. Only root runs Cargo and touches the index. One writer per file; reviewed source is frozen.

### CH5 implementation steps

User requested font-sized topical icons after accepting CH1–CH4, and explicitly chose a terminal for Command. Extend the existing trusted Lucide subset with just wrench and terminal; reuse Search/document assets. Use the existing image loader and keep icons decorative beside accessible text, with matching semantic colors. Keep disclosure arrows, bounded titles, running indicators and file Preview behavior. No icon font, emoji, new dependency or icon registry. Developer owns the three UI files, root owns the two SVG additions and docs; other source remains frozen. Build the native demo once, inspect wide/narrow icons and expanded disclosure, then review the stable delta. Existing parser/UI test evidence remains applicable where behavior is unchanged; do not add tests for static icon choices.

Asset provenance: [wrench](https://raw.githubusercontent.com/lucide-icons/lucide/0.575.0/icons/wrench.svg) (417 bytes) and [terminal](https://raw.githubusercontent.com/lucide-icons/lucide/0.575.0/icons/terminal.svg) (261 bytes), official Lucide tag `0.575.0`, covered by the existing subset's ISC/MIT license notice. Both were inspected as path-only SVG with no external references or event attributes.

CH5 implementation uses 14-point decorative images and real measured leading space in the existing header layout. The default disclosure arrow and header interaction remain; matching active work retains its separate spinner. No new icon widget/focus targets. The same document icon sits beside artifact filenames.

CH5 verification: the native binary/demo build, formatting and diff checks passed. Actual [wide icons](../design/chat-hierarchy/topical-icons-wide.png), [narrow icons with expanded details](../design/chat-hierarchy/topical-icons-narrow.png) and [narrow artifact cards](../design/chat-hierarchy/topical-icons-artifacts.png) show alignment, readable labels/statuses, bounded truncation and functional full-row mouse disclosure. Default keyboard handling was reviewed as unchanged; the synthetic-key attempt did not establish a new native keyboard guarantee. Reused CH1–CH4 behavioral evidence; no tests added for static icon choices. Independent source and native delta review **READY**, no material findings.

## Validation budget

1. For parser defects, add one focused scenario before the fix and verify the relevant assertion fails. Use installed schema/source evidence for payload shapes; handle explicit error indicators rather than guessing from text.
2. Run the affected parser scenario, then existing `ui::` tests at integration. Add a new UI regression only for a meaningful uncovered behavior, not style constants.
3. Build the existing `transcript_demo` once for native inspection. Check wide/narrow wrapping, visible failure text, expanded details, artifact selection and an actual existing-file preview. Reuse existing attachment/generation/source preservation scenarios.
4. Run formatting/diff checks and one scoped Clippy check if the integrated Rust changes warrant it. No repeated domain suite, release-size rebuild, feature matrix or live-provider requests.
5. Record commands, results and limits here. Notes remain at committed `f850060`; commit this reviewed follow-up on main when requested.

## Evidence

- Inspected the requested morning Tanglewood session (`01a17be3-2720-7180-87a1-9692c5a01354`, 2026-10-27) read-only. Captured tool failures included invalid/missing command arguments, a serialized `Outcome::Error(InvalidResearch)` and `Outcome::UnknownOutcome` after accepted-but-storage-failed PDF acquisition. Kept private identifiers out of the regression fixture. No historical commands were replayed.
- Verified installed `codex-cli 0.163.0` and generated its schema under `/tmp/tanglewood-chat-schema`. Known error/message and item lifecycle fields were cross-checked with [official app-server documentation](https://learn.chatgpt.com/docs/app-server#errors). Normalization is at receipt, from explicit fields; ordinary assistant/tool prose is not scanned for error words.
- The regression first failed against old parsing (`tanglewood.execute` lacked its readable cause). `RUSTC_WRAPPER= cargo test --locked --offline --lib --features codex codex::tests`: **16 passed** after correction, including exact raw result retention, own-domain error unwrapping, uncertainty, JSON/string notifications, terminal error fallback, UTF-8 bounds and thread scoping.
- `RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex ui::`: **35 passed**. Reused the existing lifecycle, context generation, attachment and source-preservation scenarios; extended status assertions within the existing UI scenario.
- `RUSTC_WRAPPER= cargo build --locked --offline --features gui,codex --bin tanglewood --example transcript_demo`: passed. The debug-only fixture does not connect an agent or execute its displayed commands.
- Native Linux inspection on a private Xvfb display passed at 1100 and 640 pixels: [wide hierarchy](../design/chat-hierarchy/wide-hierarchy.png), [narrow expanded uncertainty](../design/chat-hierarchy/narrow-error-details.png), and [narrow artifact cards](../design/chat-hierarchy/narrow-artifacts.png). Type/status survive truncated headers; full causes, command output and raw details remain available in bounded disclosures. Failed commands retain their nonzero exit code.
- Explicit Preview successfully opened an existing [Markdown file](../design/chat-hierarchy/markdown-preview.png) and [PNG image](../design/chat-hierarchy/image-preview.png) in the existing Files panel. View all 4 files reopened Files and exposed the fourth card. No preview read occurs merely from rendering cards. The test app/display were terminated after inspection.
- A settled PNG preview consumed **0 CPU ticks over 3 seconds** at 100 ticks/second on this software-rendered test display. This is a local idle observation, not a performance benchmark. No dependencies, bundled fonts or production polling loops were added; saved evidence images are documentation, not bundled application assets.
- `RUSTC_WRAPPER= cargo clippy --locked --offline --all-targets --features gui,codex`, `cargo fmt --check` and `git diff --check`: passed. Existing compiler/Clippy warning debt remains.
- Independent combined source and native-evidence review: **READY**, no material findings; reviewed source remained frozen through acceptance. No claim of Android, live provider error delivery, or all possible error shapes. Notes remain committed at `f850060`; this chat follow-up is included in the Git commit containing this record, as authorized by the user.
