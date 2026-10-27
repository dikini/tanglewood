# Reader comments and code presentation

Hide parsed HTML comments from native rendered Markdown, including comments inside fallback source blocks. Preserve exact authored bytes and source origins; do not hide comment-like text in inline/fenced code. Comment-only blocks must not create whitespace or keyboard stops. Keep unrelated HTML fallback behavior.

Render ordinary code fences in a full-width rounded container with padding, a muted language label, syntax colouring and horizontal scrolling for long lines. Keep artwork/callouts on their existing paths. Reuse egui_extras/syntect with its built-in languages and add Ash via an explicit adaptation of the local ash-vscode TextMate grammar, retaining captures and rule order. Unknown languages render readable plain text. Apply the same code widget to native/Pandoc CodeBlock output. No editor rearchitecture or source rewriting.

Validate comments/source binding and highlighter behaviour with focused regressions; run GUI suite and build, inspect Linux examples, then independent review. External ash-vscode remains read-only. No Android rebuild or automatic commit.

## Result and evidence

Implemented native comment hiding with exact original source bindings. HTML scanning is restricted to parser-designated HTML regions, respects quoted attributes, raw-text elements and CDATA, and handles short comments without swallowing following text. Comment lookup uses sorted range seeking rather than repeatedly scanning unrelated comments. Comment-only Pandoc raw blocks are also hidden.

Added cached syntax presentation using existing syntect5.3.0 (no new transitive packages). The Ash grammar adaptation preserves local VS Code match patterns/capture scopes/order; standard tokens resolve case-insensitively via the syntax set. Unknown languages fall back to plain text. Ordinary fences use rounded full-width containers, language labels, padding, selectable code and horizontal scroll with visible keyboard focus. Artwork/callout rendering remains separate.

Validation: GUI library suite28 passed/1 external Pandoc fixture ignored; Linux binary rebuilt. Independent review READY after correcting HTML literal/short-comment/custom-tag edge cases and comment-range lookup complexity. Native isolated smoke verified Bash/Ash/Rust colours, hidden comments, preserved literal code comments, stable long-line container width and Shift+Tab/Right horizontal scrolling. Screenshot: `docs/design/code-blocks-linux.png`. No Android rebuild.

Added `docs/fixtures/mvp-2/code-and-comments.md` and imported it through the shared CLI into `target/quiet-demo-vault` as `Code and comments.md` (note36351b34-bfa7-47f7-9351-73a5449270c6). Exported canonical text exactly matches the fixture. User lesson notes were preserved.
