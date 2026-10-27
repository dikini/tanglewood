# Quiet research workspace: design review and implementation plan

**Goal:** Make the existing research capabilities usable around the existing Codex conversation. This pass changes presentation and navigation, without adding domain commands, persistence fields, retrieval, or generation capabilities.

**Architecture:** Reuse `ConversationPanel` in the research center. `ResearchUi` owns source/outcome panels and contextual windows; shared commands and persistence acknowledgments remain unchanged. Preserve the approved quiet shell and existing draft/permission guards.

**Tech stack:** Existing Rust/egui, existing research commands and Codex controller.

## Accepted interaction

The user approved a conversation-centered workspace after reviewing Google's screenshots: sources on the left, conversation in the center, outcomes on the right. Sources can be added at any time. The latest instruction is to keep the UI quiet. A further design approval is unnecessary for applying this direction.

- Left: compact workspace switcher, source list, one add action. PDFs/files, links and retained snapshots are input material. Source text and assessments open on demand.
- Center: the existing Codex conversation and composer. Switching an associated conversation or preparing research context keeps the research workspace visible. Do not substitute a second chat implementation.
- Right: outcomes already represented by linked vault notes, with report actions. Do not advertise unavailable slide/infographic generation. Retained input files are not automatically outcomes.
- Details: brief, checkpoints, provenance, manual links and recovery controls remain available contextually. No default stack of forms, raw citation-ID field, debug rows or persistent instructional cards.
- Narrow windows show one working surface with requested side panes; preserve access to sources and outcomes without squeezing three columns together.

## Review findings and deferred functionality

1. Existing central forms expose storage structure instead of the activity. Move them behind explicit detail actions.
2. `ResearchArtifact` currently mixes retained inputs and generated outputs, with no explicit role. For this UI pass show retained artifacts with sources and linked notes with outcomes. A future role model must avoid guessing from file extension or silently migrating data.
3. Automatic reference extraction from Codex answers, reference previews, collect selected/all, and exact click-through citations are not a complete integrated path. Document them; retain the existing manual source/citation tools.
4. Source collection, inclusion in a prompt and quality assessment are separate decisions. Existing human/agent assessments remain separate. Automated triage and decision history are future work.
5. Source context remains explicitly prepared, bounded and guarded; merely adding a source does not automatically make every chat turn aware of it. Automatic retrieval and contextual citation binding are deferred.
6. Existing local report creation assembles checkpoints into a new editable vault note. Agent-written synthesis uses the existing prepared report request and conversation. Do not label checkpoint assembly as a newly generated synthesis.
7. Skills, dependencies and generators for slides/infographics remain a separate capability increment. No new provider integration in this pass.
8. Existing import/retention dialogs take local paths. Native file picking and drag/drop are still UX gaps; no new picker integration is included here.
9. PDF/text extraction and retention of the original file currently use separate operations. A unified add-file operation that retains the original and registers extracted evidence is future work.

## Delivery tasks and ownership

1. Root: review and preserve current behavior; own this plan, `src/ui.rs`, status/evidence documentation. Record gaps rather than implementing them.
2. Medium Sol developer: own `src/ui/research.rs`; compact source navigation, outcomes list, contextual details/source/collector windows. Keep existing commands and error recovery reachable.
3. Root: embed the existing conversation in research, preserve selected workspace on thread/context actions, support narrow pane switching, simplify research chat chrome.
4. Xhigh Sol reviewer: read-only independent design and frozen implementation review; correctness, draft preservation, feature gates and quiet layout.
5. Root: focused GUI/Codex tests, native build and visual inspection with empty/populated/narrow workspace. No broad backend retesting or dependency changes. Record exact evidence and limitations.

No commit or publication is authorized by this UI task. Shared dirty checkout and disjoint ownership are retained.

## Result

Delivered the UI-only pass with medium Sol implementation and independent xhigh
Sol review (READY). Sources and collected material stay left, the existing chat
stays central, and Notes/Findings appear under Outcomes. Forms open contextually;
successful report preparation transfers a natural request and its context to the
chat without sending. Failed preparation preserves both drafts and shows the
error at the top of its dialog. Existing backend contracts are unchanged.

Validation and native wide/empty/narrow evidence are recorded in
`docs/research.md`. Screenshots are in `docs/design/research-quiet-linux.png` and
`docs/design/research-quiet-narrow-linux.png`. All test windows/processes were
stopped; no provider turn, commit or publication occurred.
