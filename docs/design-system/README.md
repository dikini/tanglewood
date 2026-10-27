# Tanglewood design system

Status: approved direction, with the initial desktop/shared-shell implementation
in the codex/design-system worktree, 2026-10-27. See [adoption and evidence](adoption.md)
for implemented scope and remaining platform validation. Mobile delivery remains deferred.

## Direction and boundaries

Extend the approved [quiet desktop reference](../design/tanglewood-quiet-desktop.html)
and [egui shell contract](../plans/2026-10-26-approved-egui-shell.md). Tanglewood
should feel like the same product on desktop, tablet and phone: quiet navigation,
readable content, contextual actions, predictable focus and minimal decoration.

Use shared semantic tokens and component contracts with adaptive composition.
A single fixed desktop layout cannot serve narrow windows and touch; separate
platform visual systems would duplicate work and drift. Native integration may
vary without changing the visual language or meaning of an action.

**Styling** controls presentation. **Formatting** changes Markdown markup through
source-edit commands. Design tokens never alter authored text, parsed meaning,
filter configuration or persistence. Filtered output remains a derived view;
editing opens the original source. GUI modules continue to use the existing shared
commands; this system introduces no new application framework or document model.

## Foundations

Dimensions below use logical points, before display scaling. Values are starting
contracts, not physical pixels. User text scale and safe-area/keyboard insets apply
before layout fitting. Width classes do not imply a device or OS.

### Color

Components consume semantic names, not copied hex literals. Start from the approved
palette; verify contrast for each foreground/background pair before adoption.

| Token | Dark | Light | Use |
| --- | --- | --- | --- |
| surface.content | #191919 | #FAFAFA | Reading and conversation surface |
| surface.navigation | #161616 | #F2F2F2 | Rail/sidebar |
| surface.raised | #222222 | #FFFFFF | Menus, dialogs, fields |
| surface.hover | #2B2B2B | #E5E5E5 | Pointer hover |
| surface.selected | #303030 | #DEDEDE | Persistent selected item |
| text.primary | #DFDFDF | #292929 | Reading and controls |
| text.secondary | #AAAAAA | #666666 | Captions, secondary labels |
| border.subtle | #303030 | #DEDEDE | Quiet separation, not focus |
| accent.link | #8AB4F8 | #245DB3 | Links and restrained accent |
| focus.ring | #8AB4F8 | #245DB3 | Keyboard focus |
| status.success | #83C995 | #246A37 | Successful terminal state |
| status.warning | #E5BA73 | #795100 | Attention needed |
| status.error | #EF9999 | #A32222 | Failure/destructive consequence |

Focus, hover, selection and pressed states are distinct. Status needs a symbol and
accessible description; color alone is insufficient. Disabled controls are muted,
remain understandable, and explain unavailable prerequisites when useful. Do not
apply whole-parent opacity to readable content. Syntax highlighting has its own
semantic token set (keyword, string, number, comment, type, function, punctuation),
with plain-text fallback. No automatic theme-dependent changes to source bytes.

### Typography and reading

Use the existing bundled sans, serif and monospace fonts initially. Retain fallback
coverage; a future font replacement is one token change, not a widget rewrite.

| Role | Size / line height | Use |
| --- | --- | --- |
| UI body | 14 / 20 | Rows, menus, fields |
| UI secondary | 12 / 18 | Captions, auxiliary information |
| UI section | 14 / 20, semibold | Pane sections |
| Reading body | 16 / 26 | Notes and assistant prose |
| Reading small | 13 / 20 | Captions, footnotes |
| Heading 1 | 28 / 36 | Document title |
| Heading 2 | 23 / 30 | Main sections |
| Heading 3 | 19 / 26 | Subsections |
| Code | 13 / 21 | Code fences and source editors |

Start with regular/semibold weights. Support UI and reading text scale separately;
never shrink prose merely to retain a side pane. Compact H1 may use 26/34; body
remains 16/26. Preserve emphasis, links, selection and Unicode shaping inside runs.

Reading column: maximum outer width 730, including gutters; 48-point gutters on
expanded layouts, 24 on regular layouts, 16 on compact layouts. These produce a
preferred measure, not a rigid character count. Paragraph gap 16; section gap 24;
heading spacing 24 before/12 after. Tables, diagrams and code may overflow inside
their own scroll region; ordinary paragraphs must wrap.

Inline code uses a subtle pill with 4 horizontal/2 vertical padding and radius 4.
Account for its full box in line layout so adjacent lines do not collide. Never
simulate padding by inserting authored spaces. Fenced code uses a subdued rounded
container, small language label and contextual Copy action; inner padding 12–16.
Do not show internal source comments in rendered reading content. Keep them editable
in source mode. Images preserve aspect ratio and have no gratuitous outer frame.

### Spacing, shape and motion

Spacing scale: 2, 4, 8, 12, 16, 24, 32, 48. Default component gap 8, related-group
gap 16, section gap 24. Separators 1 point. Corners: inline 4, control 6, row 6,
menu/dialog 12, composer 20. Content is not wrapped in a generic card.

Motion is optional: short 100–160 ms state transitions; respect reduced motion.
No animated layout while typing, continuous decorative motion, or idle spinners.
Progress belongs beside the affected item and stops at a terminal state.

## Input and interaction

Density and layout are independent. Desktop pointer controls default to 34×34;
touch controls to at least 48×48, including row height and separation. The icon
itself is usually 18 on desktop/20 on touch, with the same Lucide family and visual
weight. Hit areas must not overlap. Mixed-input devices must retain keyboard and
pointer behavior when touch is available; do not infer input solely from Android.

Every icon has a stable accessible name and a tooltip on hover/focus. Hover-only
actions also appear on keyboard focus; touch exposes the equivalent action on
selection or via a visible overflow control. Right-click always has a menu/button
equivalent. Visible control order is tab order: top-to-bottom, left-to-right.

Tab moves between controls/panes; arrows navigate the active list or document
blocks; Enter activates; Escape dismisses the top temporary surface and restores
its invoker. Text fields retain native editing keys. Do not add invisible tab stops.
A focused block has a subtle full-height margin marker; keyboard focus on a control
uses a visible ring. Selected rows remain selected when focus moves elsewhere.

Use a 2-point focus ring with at least 3:1 contrast against its adjacent background.
Target 4.5:1 contrast for ordinary text and 3:1 for meaningful control boundaries.
These are accessibility design targets, not a claim of native accessibility
conformance. Validate the actual egui/platform accessibility and IME paths.

Reference: [WCAG 2.2](https://www.w3.org/TR/WCAG22/) provides text contrast, focus,
reflow and target-size criteria. Our 48-point touch target is a product choice;
WCAG's minimum target criterion is not a prescription for our native layout.

## Responsive composition

Let W be usable window width after left/right safe insets, in logical points.
Initial width bands are below. Also fit pane minimums; large text, explicit pane
widths and localization can cause a layout to fall back earlier.

| Class | Width | Default composition |
| --- | --- | --- |
| Compact | W < 600 | One primary surface; every undocked pane remains available as a pullout overlay |
| Regular | 600 ≤ W < 1200 | Rail + optional resizable left pane + primary surface; undocked panes open as pullout overlays |
| Expanded | W ≥ 1200 | Rail + optional left pane + primary surface + optional right context, if the width budget fits |

Desktop rail width 48; touch rail width 64. Left pane default 228, minimum 200,
maximum 360; right context default 260, minimum 240, maximum 360. Main pane minimum
480 for simultaneous columns. A pane's splitter/separator is included in its
allocated width. Fit test: rail + visible left + visible right + main minimum ≤ W.
For example, expanded pointer defaults need 48+228+260+480=1016 points; the 1200
threshold leaves reading room rather than opening a third pane at its bare minimum.
Clamp restored pane sizes to the current budget. Undock right context first,
then left navigation. Never squeeze the main content below its minimum to preserve
optional panes. Compact main width is allowed below 480 and reflows normally.

Use 24-point hysteresis when promoting into a larger width class, then fall back
immediately on an actual fit failure. This avoids jitter at resize boundaries.
Retain user pane preferences separately from transient visibility. Resizing never
silently opens a previously user-hidden pane. Automatic undocking keeps an actively
focused pane open as an overlay; other panes remain closed but reachable. Crossing width bands must not lose
drafts, change the selected item, reset scroll, or move keyboard focus unnecessarily.

### Pullout overlays and compact navigation

**All panes remain accessible in every width class.** Hidden means undocked, not
unavailable. Give each pane a stable, visible icon toggle with an accessible name,
tooltip and open state. Navigation uses the header/rail toggle; context, Outcomes
and other available panes use their corresponding header or rail controls. If
several panes share an edge, provide a labeled pane selector inside the pullout.
Do not require a gesture, hover, shortcut or settings visit to discover them.

An undocked left pane pulls out from the left; right context pulls out from the
right. It overlays rather than resizes or replaces the primary surface, preserving
its document/conversation, draft, selection and scroll. Use the normal pane widths
where space permits; clamp to the usable viewport. Leave a visible strip of the
underlying content when practical. At very narrow widths or large text scale the
overlay may fill the usable width, with an explicit Close/Back control.

Keep at most one pullout overlay open at a time; choosing another swaps the overlay
without discarding either pane's state. The first egui implementation uses a modal pullout: the underlying panes are
temporarily inactive; the pullout selector switches directly between available panes.
A lightweight scrim marks the background as inactive and prevents
click-through; clicking outside dismisses without activating what is underneath.
Overlays have their own scroll region, and their header/close action stays reachable.
No click-through typing into the covered editor.

Opening moves focus into the pane; closing restores the invoker or previously
focused control. Keyboard traversal cannot enter covered content. Escape, Android
Back, the toggle and Close dismiss the overlay using the existing IME/draft guards;
they never save, discard or submit a draft implicitly. Selecting a note/source/thread
may return to its primary view, but editing within the overlay never auto-dismisses.
Swipe-to-dismiss can be an optional convenience, with the same guards.

When there is room, a dock control can pin the overlay alongside the main content;
when there is not, it stays an overlay. Explicitly closing or undocking a pane is
separate from responsive visibility. Restore dock preferences when space returns
without reopening a pane the user explicitly closed.

On compact layouts, replace the permanent activity rail with a header destination
selector and visible pane toggles. Keep the same destinations and icons; do not
invent a parallel mobile information architecture or add a permanent bottom bar
in this first system.

### Height, keyboard and dense controls

Header defaults to 56 high (existing 66-point document header can migrate during
shell adoption). Touch header is at least 56. Content scrolls independently of
chrome. Under 600 usable points of height, reduce decorative vertical spacing;
retain hit targets and text size. Dialog bodies scroll while their title/actions
remain reachable. Place menus within the visible viewport and flip upward if needed.

On-screen keyboard reduces the usable viewport; it is not an extra bottom margin.
Keep caret and active composer above the keyboard/safe area without double insets.
Only the active editor region scrolls as needed. Composer body grows up to 35% of
usable height, then scrolls internally. Its controls wrap into logical groups:
attachments/access; mode; model/effort; Send/Stop. Compact layouts keep model/mode
reachable and move secondary controls into a labeled overflow menu. No horizontal
scrolling of the whole toolbar or hidden Send button.

## Component contracts

| Component | Shared contract | Adaptive/platform detail |
| --- | --- | --- |
| Icon action | Frameless at rest, tooltip/name, hover/press/focus/disabled states | Larger touch hit target; secondary actions in overflow |
| Navigation row | Label, optional leading icon/status, selected fill, contextual actions | 36 pointer / 48 touch minimum height; ellipsis + full-name tooltip/details |
| Pane header | Title, one primary action, overflow | Hide secondary labels before reducing hit areas |
| Search | Explicit label/name, clear action, retained query | Native keyboard/input behavior |
| Tabs | Same selection/focus/close semantics if introduced | Overflow menu, no endlessly shrinking titles; not a new docking feature |
| Menu/popover | Ordered labeled actions, clear destructive action, focus return | Anchored on desktop; sheet/full-width list when space requires |
| Dialog | One task, explicit completion/cancel, inline validation | Centered, max 640 wide; full-width page/sheet on compact |
| Editor | Source authoritative; context remains visible; Ready/Cancel while drafting | Ordinary blocks edit in place; large fences use a spacious focused sheet |
| Reader | Same document styles and exact citation targets | Text wraps; PDF/table/code has local overflow/zoom |
| Source row | Separate inclusion, availability and assessment | Status tooltip/details; no repeated raw paths or debug rows |
| Conversation | Shared Markdown presentation, compact tool activity, citations | Reading measure preserved; undocked context uses a pullout overlay |
| Composer | Draft/context preview, permission/mode/model controls, explicit Send | Keyboard-aware height and grouped wrapping |
| Progress/error | Local to affected item, honest state, actionable retry | Persistent error details in context; no routine global status clutter |

No mandatory component gallery, new widget framework, docking engine, third-party
replacement editor, custom plugin UI or theme marketplace is part of this design.
Use a small set of egui functions and semantic tokens. Keep link/selection semantics
and source-safe interactions shared between notes, research reading and conversation;
PDF page images and externally styled output documents retain their own presentation.

## Platform boundaries

Shared: palette, typography roles, spacing scale, icon meanings, component states,
labels, action ordering, document styling, responsive rules and source-edit semantics.

Platform-specific: safe insets, keyboard/IME, native Back, file picking, clipboard,
window chrome, shortcuts, share/open intents and accessibility bridges. Native file
choosers remain native. Wide layouts can show simultaneous docked panes; narrow layouts retain the
same panes through pullout overlays. Desktop-only capabilities are absent or explained
contextually on mobile, not represented by a screen full of disabled controls.

## Adoption and acceptance

This is a proposal, not a request to implement mobile or redesign all screens at
once. After design approval, use three bounded steps:

1. Centralize semantic tokens and width/input layout policy around existing
   src/ui/theme.rs. Preserve current state, commands and user preferences.
2. Adopt shared icon/row/header/menu/field contracts in desktop notes, conversation
   and research. Remove copied constants as each affected surface is touched.
3. Apply compact composition to the shared shell; leave Android-specific integration
   and runtime validation for the separately scheduled mobile work.

One small reference gallery can be a development aid if comparison becomes hard;
it is not a release gate. Do not build a general design-tool infrastructure.

Desktop visual checks: 390×844, 599/600, 900×700, 1199/1200 and 1440×900; one
large-text/zoom check forces width fallback. Check dark and light palettes, a long
translated title, expanded code, error/retry, keyboard navigation, selected versus
focused rows and preserved edit draft across resize. Open every undocked pane via
its visible toggle; check edge placement, pane switching, dismissal/focus return,
no click-through and conversion of a focused docked pane into an overlay. Sample representative cases,
not every cross-product. Android keyboard, safe area and Back claims require a
later actual device/emulator check. Native accessibility also needs runtime evidence.

Review styling changes visually against the approved reference. Domain tests need
not be repeated for a token-only change; meaningful layout-policy and state-retention
behavior can use focused checks. No Cargo checks are needed for this document.
