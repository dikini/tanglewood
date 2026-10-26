# Block keyboard navigation

Keep the reader quiet: a muted line spanning the focused block height in the left gutter indicates keyboard focus, with no block border or fill. Tab reaches each existing edit action. Up/Down move between current source-bound top-level blocks; Home/End select the first/last block. Enter opens the original source using the existing revision-checked workflow. Focused blocks scroll into view. Escape releases reader focus.

Navigation operates only while a reader block owns keyboard focus. Text fields, IME composition, links, menus and source editors retain their own key handling. Frontmatter and unbound/stale processed output are excluded; no guessed Pandoc source mapping. Keep existing link controls.

Implementation: stable view widget IDs from document NodeIds; process navigation before rendering native roots; draw a gutter marker on the focused block. Validate navigation ownership and bounds with a headless egui regression, run the GUI suite and rebuild Linux, then independently review the bounded delta.

## Result

Implemented native source-bound block traversal, focus restoration after Ready/Cancel, and marker-only gutter painting. The marker spans the full rendered block height and does not add a border, background or persistent pencil. Pandoc figure actions retain distinct focus IDs; arbitrary transformed content remains outside native traversal.

Validation: GUI library suite 24 passed, 1 ignored (external Pandoc fixture); Linux GUI binary rebuilt; Clippy gui,lan,codex completed with nonblocking warnings. Independent review ready after correcting duplicate figure IDs and queued egui spatial movement. Isolated Xvfb smoke verified Tab, Home, Down, End with scrolling, Enter opening the original paragraph inline, cursor keys remaining in the editor, and Cancel restoring block navigation. Current Android binary was not rebuilt or tested.

## Reader entry correction

F6 now moves focus from the sidebar/search to the first native document block. It is disabled while editing source or using settings, status, note dialogs or popup menus. Frameless shell icon buttons now paint a small focus outline so Tab/Shift+Tab traversal is visible. The document retains its quieter gutter marker.

Validation: 25 GUI library tests passed, 1 external fixture ignored; Linux demo rebuilt. Isolated native smoke confirmed visible Tab focus and Ctrl+P → F6 → Down transferring from search to the paragraph.

## Visible structural focus

Traced the invisible stop between note rows and document header to egui's keyboard-focusable notes-pane resize handle. It now paints a focus line inset into the pane (avoiding overpainting by the next panel), and Left/Right changes width by 16px within 180–360px. Note/document scrollbars paint a focused track and accept Up/Down scrolling. The status-details button now uses the normal hover/focus frame.

Validation: ten UI tests passed, including divider keyboard resizing; independent review ready. Linux binary rebuilt. Native Tab smoke verified focused notes divider, Right resizing, status action highlight and focused document scrollbar. Temporary focus-ID diagnostics were removed.
