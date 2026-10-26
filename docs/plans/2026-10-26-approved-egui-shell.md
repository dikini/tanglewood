# Approved quiet egui shell implementation plan

**Goal:** Replicate the user-approved Codex-inspired interactive sketch as closely as practical in native egui, preserving current MVP actions and source safety.

**Architecture:** Existing App/commands/editor/processing remain. Shell layout/theme are UI-only. Rendered blocks offer source editing only through verified current revision/ranges; arbitrary derived output never acquires guessed bindings.

**Tech stack:** egui/eframe 0.33.3, existing Lucide SVG subset and DejaVu fonts. No new framework, dependency, storage or protocol.

## Durable visual baseline

User explicitly approved `docs/design/tanglewood-quiet-desktop.html` on 2026-10-26: “That is much, much better…replicate this as close as possible…using egui shell.” This replaces the earlier exposed debug/control layout as the visual reference.

- Narrow ~48-point activity rail; ~228-point resizable sidebar. Quiet selected note rows, workspace name and icon New action. Search/trash/create controls appear only when requested.
- Main document header ~66 points: sidebar toggle, plain filename, right-aligned source icon and ellipsis. Avoid duplicated global and document toolbars.
- Reading column ~730 points including generous inner padding, centered in remaining width. Content occupies the surface; no outer card, image frame or explanatory metadata/alt text paragraphs.
- Native headings ~28 points, prose ~15–16 with clear line rhythm, captions small/muted. Dark surfaces ~#191919 document / #161616 sidebar, light equivalents supported. Low-contrast separators, restrained selected fill, frameless Lucide controls.
- Icon-only actions have tooltip and accessible label, keyboard focus and mobile hit areas. Accept/cancel appear only while editing. Errors remain visible and actionable; routine status stays small at the bottom.
- Contextual pencil at original/rendered blocks, visible on hover/focus and touch. Opens exact source; Ready/Cancel keeps reviewed revision/IME guards. For unbound filter output use an explicit source-outline menu, never an always-visible numbered button list.
- Processing/filter/resource controls and connection/picker controls live in settings. Rename/trash/restore/retry/refresh are in the document menu. Existing optional Codex panel is opened from the rail.
- Backlinks are a requested side view, not a permanent document footer. No permanent raw paths, pipeline internals, saved/mirror text rows, “original span” labels or instructional empty-preview paragraphs on the writing surface.
- Mobile: one document/drawer; sidebar toggle must retain native input transition guards. Do not compress desktop columns onto phone.

## Work and ownership

1. PM preserves approved sketch and this contract; AGENTS.md points future UI work here.
2. Shell lane exclusively owns `src/ui.rs`, `src/ui/theme.rs`: implement chrome/layout/menus and preserve all editor/input transitions. Current uncommitted icon/menu cleanup is authorized prior work; build on it.
3. Presentation lane exclusively owns `src/ui/processing.rs`, `src/ui/document.rs`: quiet content typography/artwork; `Processing::show` returns `Option<Range<usize>>` only for verified current source binding; shell feeds it into existing `edit_range`. Expose status and explicit refresh as small methods; no World/commands in renderer. No old image under new generation, no lost errors.
4. PM joins handoffs, compiles once, independent review of behavior/accessibility/reachable actions; inspect actual Linux window against reference and exercise source edit/Ready/Cancel/settings. A focused existing controller regression covers source behavior; no test-per-widget. Android build and affected layout/input smoke required because shared shell changes.

## Acceptance / scope control

The actual egui window must look recognizably like the approved sketch: rail/sidebar/quiet header/centered content/contextual editing. A compilation success or icon swap alone is insufficient. Existing MVP commands remain reachable, exact-source safety remains, meaningful errors are not hidden. Do not add tabs, docking, publishing profiles, asset sync, editor engines or new visual frameworks. Keep runtime evidence and real limitations concise in `docs/quiet-shell-evidence.md`. No new goal, external publication or unsolicited integration workflow.
