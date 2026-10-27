# Design system adoption and evidence

2026-10-27 — desktop/shared-shell increment. Independent Sol 6.1 xhigh review:
**READY**, no blocking correctness or UX findings.

## Delivered

- Semantic dark/light palette, shared type/spacing/dimension roles and distinct
  hover, selection and keyboard focus. Shared note/conversation code rendering,
  exact-source Copy, padded inline code and 16/26 prose; research text reading uses
  the same prose roles. Existing commands, Markdown source and persistence are unchanged.
- Platform-independent 34-point pointer / 48-point touch controls, automatic touch
  event detection and a Touch-friendly controls setting under Appearance. Font
  size is independent of control density; no additional dependencies.
- Width classes below 600 / 600–1199 / 1200+, with 24-point promotion hysteresis and
  minimum-width fit budgets. Rail/sidebar/context use shared dimensions; compact
  windows replace the rail with a destination selector and visible pane controls.
- A single content renderer for navigation, outcomes and conversation context in
  their dock or edge-anchored pullout. Hidden panes remain reachable from visible
  controls, and a pullout selector switches between available panes. Close/Escape,
  outside dismissal, focus return and guarded docking are supported. Pane state,
  drafts and scroll ownership remain view-local; globally stable pane IDs cannot
  be rendered in two hosts in the same frame.
- Pullouts use egui's modal backdrop: the entire background is inactive until
  dismissal, including unobscured docked panes. This conservative implementation
  prevents click-through and covered-editor keyboard input. It is documented in
  the design contract; it does not support simultaneous background interaction.
- Composer body is capped at 35% of usable height with internal scrolling. Its
  field has a stable per-thread ID; controls wrap and remain visible on compact
  windows. Header/picker widths account for actual control density.

## Worktree and review boundary

Managed worktree: `/home/dikini/.codex/worktrees/design-system/tanglewood`.
Branch: `codex/design-system`. Committed unsigned at the user’s request: the inherited
research/Codex foundation is a separate prerequisite commit, followed by this
design-system increment. No merge or publication performed.
The worktree was seeded with the current checkout's tracked/untracked project
files because HEAD predates the existing research/Codex work. Caches were excluded.
That inherited uncommitted baseline is not part of this increment's authored diff.
Original source/index were not modified by this task.

Task baseline: `/tmp/tanglewood-design-system-baseline`. A patch relative to that
baseline is available at `target/design-system-changes.patch` in this worktree.
Authored code paths: `src/ui.rs`, `src/ui/layout.rs`, `src/ui/theme.rs`,
`src/ui/document.rs`, `src/ui/code.rs`, `src/ui/research.rs`,
`src/ui/navigation.rs`. Shared domain/worker files are unchanged from the snapshot.

## Verification

Cargo jobs were serialized with `RUSTC_WRAPPER=` and the warm shared target directory
`CARGO_TARGET_DIR=/home/dikini/Projects/tanglewood/target`.

| Check | Result |
| --- | --- |
| Copied baseline: `cargo test --locked --offline --lib --features gui,codex ui::` |31 passed |
| Integrated same command |34 passed |
| `cargo test --locked --offline --no-default-features --features gui --lib responsive` |2 passed |
| Final research picker delta: `cargo test --locked --offline --lib --features gui,codex ui::research` |9 passed |
| `cargo build --locked --offline --features gui,codex --bins --example research_demo` |passed |
| `cargo fmt --check`, `git diff --check` |passed |

New coverage exercises width budgets, hysteresis, overlay swapping/focus return and
actual egui 1440→390→1440 frames. An unsubmitted Unicode source draft and widget
generation remain unchanged, covered text/F6 cannot escape overlay focus, and the
preferred dock is restored without a duplicate host. Existing GUI safety tests
were reused. Build warnings remain; no warning-free claim is made.

Native Linux inspection used a private Xvfb display and disposable research vault,
without starting an agent or changing the user's running application. Entered an
unsent composer draft, resized, opened navigation, swapped to Outcomes, dismissed
with Escape, opened context, and enabled Light/Touch-friendly controls in Settings.
Resizing the focused navigation pullout to 900×700 restored its single dock.

- [Expanded dark](evidence/expanded-dark.png)
- [Compact retained draft](evidence/compact-draft.png)
- [Navigation pullout](evidence/compact-navigation-overlay.png)
- [Outcomes pullout](evidence/compact-outcomes-overlay.png)
- [Context pullout](evidence/compact-context-overlay.png)
- [Compact light/touch](evidence/compact-light-touch.png)
- [Light/touch navigation and workspace picker](evidence/compact-light-touch-navigation.png)
- [Regular light/touch, initial resize frame; dock restored](evidence/regular-light-touch.png)
- [Regular dark/pointer, settled reflow](evidence/regular-dark-settled.png)

The initial light/touch resize capture shows a transient old transcript scroll
offset. A separate dark/pointer resize check across several repaint frames shows
correct reflow and retained content. These are different input/theme configurations,
not a before/after pair. Static review found no Markdown cache loss.

Android keyboard/IME, safe areas, native Back and accessibility bridge behavior are
not proven by desktop rendering. Mobile/sync delivery remains deferred. This is an
initial adoption of shared contracts, not a claim that every legacy settings form
or external PDF/output document is a fully tokenized component. No new tabs/docking
framework, plugin UI, presentation editor or theme marketplace is introduced.
