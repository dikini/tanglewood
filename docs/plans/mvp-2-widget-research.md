# MVP-2 R0: widget and rendering choices

Research frozen 2026-10-26 against the [revised proposal](2026-10-26-mvp-2-proposal.md). **Keep egui/eframe 0.33.3.** Reuse its controls and input state, adapt a small common theme, and build only application-specific session/layout and semantic-document behavior. This is source research, not a new GUI/platform acceptance result.

## Evidence and bounded shortlist

“Advertised” means upstream documentation; “inspected” means the published source/manifest/example was read; “compiled” and “observed” require executed checks. All new candidates below are **uncompiled and unobserved** in Tanglewood. Existing egui source editing and Android input have prior [emulator evidence](../android-emulator-evidence.md); that does not validate the new shell or highlighting.

| Candidate, exact version | Evidence, compatibility and cost | Decision |
| --- | --- | --- |
| egui/eframe **0.33.3** | Cached source inspected: Panels, ScrollArea, Grid, menus, Popup/Modal, CollapsingHeader, ComboBox, text selection, font hooks, TextEdit layouter/cursor/state. Existing application compiles and has Linux/Android observations. MIT OR Apache-2.0. | **Reuse/adapt** one coherent style. No framework upgrade. |
| [egui_extras **0.33.3**](https://docs.rs/crate/egui_extras/0.33.3/source/Cargo.toml) | Published manifest/source inspected. Exact egui 0.33.3, Rust 1.88 minimum (project 1.94 satisfies it); MIT OR Apache-2.0. Tables, syntax highlighting, PNG/image and SVG support are useful; loaders are not an asset security/resource boundary. | **Reuse/adapt** selected features only, below. |
| [egui-shadcn **0.5.0**](https://docs.rs/crate/egui-shadcn/0.5.0/source/) | Published README, manifest and component API inspected; egui 0.33, MIT. Advertises form controls, tabs, popovers/dialogs, typography and theme tokens. Upstream explicitly warns of unstable API. Manifest also pulls chrono, regex and lucide-icons; optional plot is not needed. README installation example still names 0.3.1. No Android input evidence. | **Do not add.** Useful quiet-surface/token reference; replacing TextEdit/form state adds integration risk without resolving our session/IME requirements. |
| [egui_tiles **0.14.1**](https://docs.rs/crate/egui_tiles/0.14.1/source/) | Published manifest, README and `examples/simple.rs` inspected; egui 0.33.0 range accepts 0.33.3, MIT OR Apache-2.0. Tree/Behavior API supplies tabs, horizontal/vertical/grid layouts and drag docking; default serde. Example stores independent pane content and does not solve shared drafts. | **Do not add.** Native panels plus a concrete two-pane state are smaller for the selected layout. Revisit only if arbitrary docking becomes a requirement. |

These are the two shell alternatives surveyed. No further docking/flex/framework survey is needed. The upstream [egui repository](https://github.com/emilk/egui/tree/0.33.3), [shadcn repository](https://github.com/FerrisMind/shadcn-rs) and [tiles repository](https://github.com/rerun-io/egui_tiles) provide examples/screenshots; their appearance is not proof of keyboard, touch or Android behavior.

## Reuse / adapt / build matrix

| Needed capability | Choice and remaining application work |
| --- | --- |
| Buttons, inputs, menus, dialogs, popovers, tooltips | **Reuse** egui; **adapt** centralized colors, spacing, radii and focus states. Native TextEdit stays inside the existing Android mirror helper. Do not introduce a kit-specific input state. |
| Rail, collapsible/resizable sidebars, status strip | **Reuse** SidePanel/TopBottomPanel/CentralPanel and bounded ScrollAreas; **adapt** compact contextual controls. Sidebar trees/command search use ordinary selectable rows and existing search, not a second search framework. |
| Tabs, reorder, keyboard switching, one optional split | **Build** a small local tab/pane state over shared note sessions; **reuse** egui drag responses and Resize/panels. Persist only device preferences. Mobile uses one editor and drawers, never a squeezed desktop docking tree. |
| Source/block editing, Ready/Cancel, recovery | **Reuse** TextEdit, cursor/undo and Android input bridge; **build** one document transaction controller and conservative source bindings. Only the focused view edits; other views reference the same draft. Ready must replace autosubmit, remain blocked during composition, and use the existing revision-checked command. Cancel/adopt must reset undo/widget generation deliberately. |
| Syntax highlighting | **Adapt** extras/Syntect spans into TextEdit's layouter. Cache by session/draft generation, language, font/theme; wrap-width affects galley layout separately. Do not parse Markdown or process filters on every frame. |
| Rich document text, headings, links, lists, quotes | **Reuse** egui text/layout primitives; **build** one native semantic flow renderer. Existing reader/Pandoc projection is authoritative. A CommonMark viewer that reparses a Markdown string cannot substitute for Pandoc/custom-filter nodes or verified source bindings. |
| Tables, figures/captions, footnotes, source edit affordances | **Adapt** Grid/TableBuilder where appropriate; **build** semantic layout, conservative bindings, bounded image sizing and diagnostic/source fallback. Complex tables/math/HTML remain explicit fallback per plan. |
| Notes / Academic / Tufte-inspired profiles | **Build** style parameters on the same flow renderer: width, type scale, line spacing, note placement. Wide-screen margin notes need non-overlap bounds; phones show inline notes. Not three renderers or pagination. |
| SVG/PNG assets | **Reuse** resvg/image codecs; **build** bounded owned-byte decode/publication/cache integration with P0. Do not load arbitrary file/HTTP URIs from documents. |
| Keyboard, touch, accessibility | **Reuse** focus, WidgetInfo, shortcuts and existing platform bridge; **adapt** explicit labels, focus visuals, touch geometry and navigation. Actual new-layout acceptance remains required. |

## Exact minimal dependency proposal

PM owns manifest/lock changes. Add only optional GUI dependencies/features, preserving headless capability separation:

```toml
egui_extras = { version = "=0.33.3", optional = true, default-features = false,
                features = ["syntect", "svg", "image"] }
image = { version = "=0.25.10", optional = true, default-features = false,
          features = ["png"] }
```

Enable these from `gui`; `image 0.25.10` is already resolved transitively. Do not enable `all_loaders`, `file`, `http`, GIF or WebP. Source-inspected extras dependencies select **Syntect 5.3.0** with `default-fancy` (Rust regex path, no Oniguruma requirement) and **resvg 0.45.1**, default features off. PM must confirm the resolved transitive versions/license inventory after its serialized fetch. Crates.io version metadata reports Syntect 5.3.0 MIT, resvg 0.45.1 Apache-2.0 OR MIT, and image 0.25.10 MIT OR Apache-2.0; retain their transitive notices too. No new async runtime, docking kit or icon crate is requested.

Extras' `highlight` uses a FrameCache keyed by font/theme/code/language/settings and performs highlighting on a cache miss while accessing egui memory. It still hashes code and is not the required long-lived per-generation editor cache or a bounded worker scheduler. Use the underlying Syntect path on a bounded latest-work lane if representative typing shows an issue; retain plain-text layout until a matching result arrives. Do not block typing waiting for colors. One active buffer is the first implementation, not many editors or a new editor engine.

Extras' SVG loader rasterizes synchronously on a miss, caches by URI/size, and its helper allocates a pixmap from the selected raster dimensions. The PNG helper calls `image::load_from_memory` then RGBA conversion. Neither helper alone enforces our job quotas, dimensions, total cache bytes or asset path policy. For document artwork, decode owned verified bytes off-frame with P0's limits, publish only current immutable asset keys, then upload/cache textures on the owner UI. Tiny bundled icons may use the loader directly. Never interpret source-provided network paths as permission to fetch.

**SVG text contract:** `svg` alone disables resvg text. If the required saved artwork contains SVG `<text>`, P0 must either prove outlined-text fixture compatibility or select resvg text with an explicitly bundled font database; do not silently drop text. Extras' `svg_text` additionally enables system-font discovery, which is unsuitable as a promise of identical Android rendering. Set deterministic font inputs rather than depend on host font installation.

## Icons

Select a vendored **Lucide 0.575.0 SVG subset**, with [tagged source](https://github.com/lucide-icons/lucide/tree/0.575.0/icons) and the [complete tagged license](https://github.com/lucide-icons/lucide/blob/0.575.0/LICENSE): ISC plus MIT attribution for Feather-derived portions. The license and `file-text.svg` were fetched/inspected. Do not import all icons or a runtime network URL.

Initial subset: `file-text`, `folder-open`, `search`, `link`, `panel-left`, `panel-right`, `columns-2`, `rows-2`, `plus`, `x`, `check`, `undo-2`, `redo-2`, `chevron-down`, `chevron-right`, `more-horizontal`, `settings`, `refresh-cw`, `circle-alert`, `sparkles`. Verify each tagged filename when vendoring; retain provenance and license alongside assets. Resolve stroke color from theme/state into a bounded cache key (icon, size, color), not a freshly unique SVG URI each frame.

Alternatives inspected: [egui-lucide 0.1.0](https://docs.rs/crate/egui-lucide/0.1.0/source/Cargo.toml) targets egui **0.36**, so is incompatible without a second egui version. [egui-phosphor 0.11.0](https://docs.rs/crate/egui-phosphor/0.11.0/source/) targets **0.33**, is compatible on paper, and adds a selected font as a proportional fallback. Its regular TTF is 488,636 bytes; additional weights add roughly 449–537 KB each. Adapter MIT OR Apache-2.0, icon assets MIT. Latest 0.14.0 targets egui 0.36. The small SVG subset reuses the already required SVG path and avoids private-use icon characters in labels/font fallback.

Use 18–20 point desktop icons and 24 point phone icons inside suitably sized hit targets. Every icon-only control needs a real accessible action label and tooltip; color alone must not convey state. Preserve visible text for Ready/Cancel and errors.

## Fonts and visual tokens

Select **DejaVu 2.37** unmodified regular assets: Sans for shell/Notes, Serif for Academic/Tufte prose, Sans Mono for code. They are already available for vendoring at the exact paths below; do not depend on these system paths at runtime. Upstream [release/download information](https://dejavu-fonts.github.io/Download.html) and [license](https://dejavu-fonts.github.io/License.html) identify the Bitstream Vera permission with DejaVu changes in public domain. Bundle the notices; do not describe these as OFL fonts. Font binaries may be bundled in the application, not sold alone; preserve names/notices and review rename conditions before subsetting/modifying.

| Local source asset | Bytes | SHA-256 |
| --- | ---: | --- |
| `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` | 759720 | `b4c632e3cdf9acc7f28758fb5a323c8524d7fc6660d46904d9b6cbe2809c419c` |
| `/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf` | 380660 | `a52c2a69dfc93d0b511b98c1019a57eb3d6a62d328bdf1387f00851b02e3a315` |
| `/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf` | 343140 | `a54dca07c76d6289e717e75e0a58c0128f6d7269ef3faf76417c9d7d3bba37ab` |

Total raw regular fonts **1,483,520 bytes**, before compression, atlas memory and existing fallback fonts. `fc-scan` reports version 155320 (16.16 ≈2.37); `fc-query` character maps contain every character in `Привет Ελληνικά ∑∫√∞≤≥≈ℝαβγ→↔` for all three. This is glyph-presence evidence, not shaping, visual quality, full scientific coverage or math typesetting. Keep existing emoji/default fallbacks; explicitly check representative fallback glyphs when registering custom FontDefinitions. Register once, avoid per-frame `set_fonts`/atlas rebuilds. First implementation can use the selected regular families with egui emphasis/italics; true bold/italic face selection and any additional needed glyphs must be explicit rather than assumed from this three-file selection.

Proposed shared tokens: spacing 4/8/12/16 points, compact 6-point corner radius, restrained separators and one accent; distinct focus outline/error warning; shell text 14–16, prose 17–18, code 14–16 with user scaling. Use the existing dark/light style as a base; document profile colors/type are separate from shell theme. Desktop rail 44, sidebar initial 240 (resizable), reading width roughly 720; two-pane mode only when each pane remains usable (initial minimum 320). Android retains one central writing view, closed drawers by default and 48-point touch targets. These are implementation starting tokens, to be checked on the existing emulator rather than asserted to be universally accessible.

## Interaction, contracts and next gates

Pinned TextEdit source exposes `layouter`, `code_editor`, `load_state`, cursor/undo state and selection WidgetInfo. A layouter must preserve source byte/character relationships and wrapping; highlight spans never mutate the buffer. Keep the Android complete-state mirror/UTF-16 mapping, epoch/sequence checks, old-widget composition recovery, protected Edit menu and system clipboard. Switching block/tab/pane cannot silently commit or discard a composing draft. No new custom selection or InputConnection engine is needed.

Use explicit keyboard shortcuts with `KeyboardShortcut`/input consumption and focus ownership, visible menu hints, Tab/Shift-Tab focus and Escape/Ready semantics that respect IME composition. Do not consume editor shortcuts globally while a text field owns focus. Menus/tooltips alone are not accessible labels; provide WidgetInfo/action text and visible focus. Eframe 0.33.3 has an `accesskit` feature, but current default-features=false GUI does not enable it. Request enabling `eframe/accesskit` in GUI after the PM's dependency review; host assistive-technology behavior and Android accessibility are separate unobserved gates, not established by source presence. Keyboard and screen-reader navigation through new custom tab/command rows needs intentional semantics.

Consumer contracts for S0/P0: (1) stable shared draft/session ID plus generation, base revision and composition/conflict state; only one input owner; (2) verified source ranges/opaque generated nodes from the authoritative reader; (3) immutable owned asset bytes/hash and bounded raster result, no arbitrary URI loader; (4) semantic document/style-independent content, explicit processing state and retained old result; (5) local pane/preferences state, never CRDT note content. No shared production interface is changed by this document.

Next checks are intentionally small: after serialized dependency integration, compile the actual GUI assembly once; inspect representative highlighted Markdown/Rust with Cyrillic/Greek/emoji and Ready/Cancel adjacent-byte preservation; run Linux resize/tab/split/focus and Android drawer/composition/clipboard on the existing bridge. Exercise one actual SVG/PNG illustration through P0's owned asset path, including SVG text policy. Then record one representative release assembly's startup, whole-process memory, idle CPU and resize/typing behavior. No benchmark harness, speculative matrix or R0 compilation is needed. APK/device and processing gates remain open until executed by their assigned lanes.

Research validation: published crate archives extracted only under `/tmp/tanglewood-r0`, manifest/source/examples read; cached egui source inspected; `fc-scan`, `fc-query` sample coverage and `sha256sum` executed on the three local fonts. No Cargo invocation, production file edit, dependency change, Android/ADB action or new runtime observation occurred in R0.

## Subsequent integration evidence

The table above records the R0 research checkpoint, not current implementation status. The selected egui_extras 0.33.3/Syntect 5.3.0/resvg 0.45.1/image 0.25.10 stack subsequently compiled in the Linux GUI and API35 x86_64 Android APK. Actual native SVG/PNG, bundled icons/fonts and the source-editor workflow were observed; see [Linux evidence](../mvp-2-linux-evidence.md) and [Android evidence](../mvp-2-android-evidence.md). Shadcn/docking alternatives were not added. Extra styles, tabs/splits and a resource benchmark remain outside the reduced slice. These observations do not establish universal glyph, accessibility, IME or device coverage.
