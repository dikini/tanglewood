# Design system implementation plan

**Goal:** Adopt the approved shared visual system and responsive pullout pane behavior in the existing egui application, in an isolated worktree.
**Architecture:** Semantic theme tokens and one view-local layout policy; source/domain commands and draft ownership unchanged. All panes use the same content functions docked or in overlays. Compact navigation uses visible header controls, not squeezed desktop columns. No new dependencies, mobile/sync functionality or external publication.
**Tech stack:** Existing Rust package, egui/eframe 0.33.3, Lucide and bundled fonts.

## Baseline and ownership

Worktree: /home/dikini/.codex/worktrees/design-system/tanglewood, branch codex/design-system.
Includes a file-copy snapshot of the current uncommitted project. Original checkout/index remain unchanged. Task baseline for code diffs is /tmp/tanglewood-design-system-baseline, not HEAD (which predates the existing research/Codex work).
Root owns src/ui.rs, new src/ui/layout.rs and docs/examples; theme developer owns src/ui/theme.rs, src/ui/document.rs, src/ui/code.rs, src/ui/codex_markdown.rs. Independent reviewer reads stable changes only. One Cargo owner: root. No staging/commits.

## Tasks

1. Baseline: run existing focused GUI tests with gui,codex in the copied current state. Record failures without conflating sandbox IPC restrictions with layout changes.
2. Theme: central semantic palette/type/spacing/control sizes, shared hover/selected/focus and input-aware targets. Apply document/code and conversation presentation via the shared roles. Keep exact source and Markdown semantics. Verify actual dark/light appearance.
3. Policy: test compact/regular/expanded width classes with hysteresis and pane width budgets; input density independent of OS; one active overlay, closed versus docking preferences and focused-pane transition. Keep user data outside this policy.
4. Shell: share sidebar/outcomes/context content between docked/overlay hosts. Visible toggles for all available panes, edge anchored overlays, scrim preventing click-through, explicit close, Escape/focus return and safe transitions. Compact destination selector replaces activity rail; no draft rebuild on resize. Clamp scrolling content and dialog geometry to viewport.
5. Validation: focused layout and existing GUI tests once, relevant combined build, formatting/diff checks. Native wide/narrow, keyboard, overlay swap/dismissal, actual draft preservation and light/dark visual checks. Review correctness/spec then UX against docs/design-system/README.md. No Android runtime claim from desktop checks.
6. Delivery: update design-system adoption/evidence record with precise implemented behavior and limits. Leave worktree attached and uncommitted for user review.

## Result

All desktop/shared-shell tasks delivered. Independent Sol 6.1 xhigh review READY.
34 GUI checks, 9 final research checks, 2 no-Codex responsive checks and final native build
passed; formatting/diff clean. Native dark/light/touch and pullout evidence is in
docs/design-system/adoption.md. Modal backdrop is an explicit implementation
choice; mobile runtime validation remains deferred. Worktree attached. User-authorized unsigned commits separate the prerequisite
foundation from the design-system increment; no merge or publication.

## Finalization

User-authorized fast-forward merge to main completed through `4e05aa5`. Source
matches the reviewed implementation; post-merge formatting/diff checks passed.
Worktree archival requested through the app; no push/publication. Unrelated local
UX audit work and caches preserved. See docs/design-system/adoption.md.
