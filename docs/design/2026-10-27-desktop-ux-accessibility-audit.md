# Tanglewood desktop UX and accessibility audit

Date: 2026-10-27. Saved from the desktop review requested in this conversation.

## Summary

Tanglewood's quiet desktop shell is a strong foundation. The biggest opportunities
are clearer navigation, more resilient pane layouts, and more explicit feedback
when editing or recovering work. Preserve the approved rail, sidebar, quiet header,
centered document, and contextual source editing.

This review covers the approved reference, repository screenshots, and current UI
implementation. Screens without captures were reviewed in code. No live desktop or
screen-reader session was run for this audit; accessibility findings are risks,
not a compliance certification. Screenshot findings require reproduction against
the current application before implementation.

The [implementation plan](../plans/2026-10-27-desktop-ux-accessibility.md) maps the
seven priority findings and the remaining screen improvements to bounded tasks.

## Top three layout flaws

- [ ] **L1 — Side panes compete with the primary task.** Research keeps Sources
  and Outcomes beside the reader; Codex adds another substantial context pane.
  This reduces reading space and truncates important names. Automatically undock
  secondary panes when the main surface becomes cramped, preserving visible toggles
  and focus. The narrow research capture also shows a partially clipped first
  message: check the transcript's scroll viewport beneath its header. A capture of
  an intentionally scrolled conversation alone does not establish a layout defect.

- [ ] **L2 — Source navigation hides the information needed to choose correctly.**
  Long titles truncate, while inclusion checkboxes and availability symbols sit
  together without clear visual grouping. Let the selected source show its full
  title, provide full names on hover and keyboard focus, and separate "included in
  context" from "text available." This improves comparing similar papers and
  returning to previously used evidence.

- [ ] **L3 — The editing state is too visually subtle.** Inline drafts resemble
  ordinary document content, with isolated check/cross actions below; the source
  dialog places those actions far from short edits. Add a restrained active-block
  indication, keep actions adjacent to the draft, and retain surrounding reading
  context. This makes the read → edit → apply journey clearer without adding
  permanent toolbars. Use the approved gutter focus marker and a boundary on the
  active editor rather than introducing permanent borders around reader blocks.

## Top two accessibility risks

- [ ] **A1 — Native screen-reader support appears disabled in the desktop
  configuration.** `Cargo.toml` disables eframe's default features without explicitly
  enabling `accesskit`. Several fields also lack explicit label associations;
  research inclusion checkboxes rely on hover explanations. Enable the native
  accessibility bridge, give fields and checkboxes contextual names, and verify
  reading order, state announcements, and dialog focus with a Linux screen reader.
  This follows the [name, role, and value principle](https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html).
  Confirm the resolved Cargo feature graph before treating this as a runtime finding.

- [ ] **A2 — Important states depend on faint visual cues.** Selected rows use
  subtle gray fills; source availability uses small muted symbols, and supporting
  labels are often 11 points. Add a distinct selection marker, a clear focus outline,
  and readable status text in source details. Measure active text against the
  [4.5:1 text benchmark](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)
  and essential state indicators against the
  [3:1 non-text benchmark](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html).
  Decorative separators can remain quiet. Existing focus outlines and keyboard
  navigation should be retained and checked before adding replacement controls.

## Top two microcopy improvements

- [ ] **C1 — Make edit completion explicit.** Replace "Ready — accept changes"
  with **"Apply changes"** and "Export accepted source" with **"Export saved
  Markdown…"** only when durability is confirmed; otherwise use **"Export current
  Markdown…"** with "Unapplied edits aren't included." Keep applying a draft and
  saving to disk distinct.

- [ ] **C2 — Translate recovery language into a decision.** Replace "Use current
  revision, retain my assessment" with **"Keep my assessment and retry"**, followed
  by: "This source changed while you were editing. Review the latest version before
  applying your assessment." Preserve the existing conflict safeguards; make their
  consequence understandable. The final button wording must match whether its
  current action retries a save or only prepares a retained draft for review.

## Per screen analysis

| Screen or state | Journey and assessment | Most useful improvement |
| --- | --- | --- |
| Notes and rendered document | Reading is calm, with strong headings and a centered column. First-time users must discover editing through contextual controls. | Reveal the pencil on keyboard focus as well as hover; clearly mark the active draft. |
| Inline block editor | Surrounding content preserves orientation, but the boundary between reading and editing is weak. | Add a subtle editor boundary and nearby Apply/Cancel actions. |
| Large-block and source editor | The spacious sheet supports long Markdown and code. Short edits leave substantial empty space between content and actions. | Use a bounded editor height with a consistently reachable action footer. |
| Search and backlinks, code review | Separate destinations preserve the clean reader. Search uses a placeholder as its visible identification. | Give search an explicit accessible name and make query/no-results feedback clear. |
| Create, rename, and trash, primarily code review | Contextual dialogs keep administration off the document surface. "Markdown path" assumes filesystem knowledge. | Show a concrete example and distinguish the note name from an optional folder. |
| Codex conversation and new thread | Transcript typography is readable. Empty conversations have little guidance, while composer controls form a dense secondary row. | Offer one useful starter action; keep Send and essential controls reachable as the row wraps. |
| Codex context panel | Goal, Plan, Files, and Plugins provide useful organization. The Plugins capture exposes authentication terminology and tool counts prominently. | Lead with usable capabilities; put connection diagnostics behind details. |
| Model and approval menus | Settings are available close to the composer. Nested model menus occupy considerable space; approval labels require interpretation. | Keep the current choice visible and explain each permission option's consequence locally. |
| Thread actions | Rename, Archive, and Remove are easy to find; permanent removal has an explicit confirmation. | Preserve that distinction and ensure focus returns to the invoking thread after dismissal. |
| Research workspace and Sources | Sources → conversation → Outcomes supports the research journey well. Long names and compact state symbols weaken source identification. | Prioritize full selected-source identification and distinguish inclusion from availability. |
| Research source reader, text | Search and page controls support evidence inspection. Extracted PDF text retains awkward line breaks that interrupt scanning. | Offer a derived reading view while keeping exact captured text available for verification. |
| Research source reader, PDF | Original-page viewing supports citation checking. The implementation fits the page to available width, making dense papers small. | Add visible zoom controls or a clearly labeled action to open the original externally. |
| Research Outcomes and output creation | Saved artifacts remain close to the discussion. "Slides, infographic or map…" truncates in the pane. | Use a short "Create output…" action, then present the formats inside its dialog. |
| Research details, assessments, and recovery, code review | Detailed forms preserve substantial research context, but revision and retention terminology raises cognitive load. | Use task-based recovery wording and keep errors beside the affected draft. |
| Settings, Processing, code review | Advanced processing stays outside the reader. Appearance settings are nested under Processing. | Move Appearance to a clearly named section; collapse filter syntax into Advanced. |
| Settings, Connections | Agent permission is explicit. An unavailable LAN feature leaves a large, mostly empty dialog in the capture. | Size the dialog to its contents and explain availability in ordinary language. |
| Settings, Files and status, code review | Import/export capabilities are present, but manual paths and several operation variants demand attention. | Group each operation with its relevant fields and show completion or recovery beside it. |

## Scenarios and user journeys

| Scenario | Journey to preserve | Main friction |
| --- | --- | --- |
| First-time note author | Find/create a note → read → discover source editing → apply → confirm save | Editing affordances and the distinction between applied and saved changes |
| Returning keyboard user | Search → select a note → enter reader with F6 → edit → return focus | Explicit field names, visible selection/focus, and dependable focus restoration |
| Researcher comparing evidence | Include sources → read a paper → follow a citation → return to conversation | Truncated source identity, availability ambiguity, and competing panes |
| Researcher preparing an output | Discuss evidence → prepare report/output → inspect the request → send explicitly | Long output labels and unclear preparation versus submission |
| User recovering interrupted work | Encounter import/save/revision failure → review retained work → retry or discard deliberately | Technical recovery labels and errors far from the affected draft |
| User with low vision or a narrow desktop window | Enlarge text/resize → open needed panes → read and send without losing context | Faint state cues, crowded controls, and insufficient space for the primary task |
| Screen-reader user | Identify controls → traverse pane/dialog → change inclusion → hear result | Accessibility bridge availability, contextual names, and state announcements |

## Evidence and design references

- [Approved desktop reference](tanglewood-quiet-desktop.html) and
  [approved shell contract](../plans/2026-10-26-approved-egui-shell.md).
- [Design-system proposal](../design-system/README.md), which remains a proposal.
- Notes and editing: [reader](quiet-egui-linux.png),
  [inline editor](contextual-inline-linux.png),
  [source dialog](contextual-modal-linux.png), and [code blocks](code-blocks-linux.png).
- Conversation: [transcript](codex-rich-transcript-linux.png),
  [context](codex-context-panel-linux.png), [model picker](codex-model-picker-linux.png),
  [approvals](codex-approval-modes-linux.png), and [thread menu](codex-thread-context-linux.png).
- Research: [wide workspace](research-quiet-linux.png),
  [narrow workspace](research-quiet-narrow-linux.png),
  [current populated workspace](research-desktop-complete-linux.png),
  [text search](research-desktop-search-linux.png),
  [PDF reader](research-desktop-pdf-linux.png), and
  [narrow reader](research-desktop-narrow-reader-linux.png).
- Settings: [agent permission and connections](vault-agent-permission-linux.png).
- Implementation: `src/ui.rs`, `src/ui/theme.rs`, `src/ui/navigation.rs`,
  `src/ui/editor.rs`, `src/ui/research.rs`, `src/ui/processing.rs`,
  `src/ui/codex_context.rs`, and `Cargo.toml`.

The highest-value first pass is pane resizing and clipping, full source
identification, and native accessibility support. These changes improve everyday
reading, research, and keyboard use while preserving the approved visual direction.
