# Native reader and extension boundary

`markdown::read(NoteId, Revision, &str)` builds owned renderer-independent blocks
and inlines. The source is canonical; the reader does not serialize its model
back to Markdown. Origins contain note ID, opaque revision and UTF-8 byte range.
They are valid only against that exact source revision, never persistent block IDs.

Native reader behavior:

- CommonMark paragraphs/headings retain text, code, emphasis, strong emphasis,
  line breaks and links. Fenced blocks retain body, exact fenced source and raw
  opening-line information after the delimiter. Unsupported containers, images,
  and HTML have visible exact-source fallback instead of fabricated rendering.
- Wikilinks use pulldown-cmark 0.13's opt-in syntax: `[[Target]]` has label Target;
  `[[Target|Label]]` has target Target and label Label. Targets are not normalized,
  resolved to notes, or given implicit `.md` suffixes here. Ordinary Markdown
  links are extracted too, including links in source-fallback containers. Code
  spans and code blocks never contribute links. Embeds/images are not backlinks.
- Fence attributes accept a plain language as a class, or a full
  `{#id .class key="value with spaces" key=value}` group. Multiple classes and
  key/value pairs retain order. Nested braces, escapes, duplicate IDs, empty
  keys/values and unclosed quotes reject attribute parsing; raw info and source
  remain available. This is a documented subset, not Pandoc Markdown.
- Metadata has an owned reserved field; the native reader does not interpret
  YAML frontmatter. `Reader::PandocMarkdown` reserves an adapter choice, not an
  implemented alternate parser.

`filter_document` takes an explicit ordered filter slice and `FilterContext`
containing the real writer target (`json` for native preview, `html` for export)
and configuration. The concrete native `Callout` example recognizes `.callout`
fences and returns a Div with the original attributes around a normal Quote,
whose body uses the native reader. This mirrors the structural behavior of
[the real Pandoc Lua example](plan/examples/callout.lua); it does not execute Lua
or claim full Pandoc body-syntax compatibility. The target-neutral example
ignores configuration. No external process runner or plugin installation exists.

After any whole-document filter, all origins, including descendants, are cleared
and the document is read-only. Filters have no App, ECS, command or persistence
handle. An eventual apply-to-note action must build a reviewed revision-checked
edit through shared commands. Filtered source is never automatically persisted.

`FenceRequest` owns body/raw info/attributes/exact fallback, origin and context.
`FenceOutput` permits normal blocks, a static image path with alternative text,
or source plus diagnostic. Image decoding, job limits, stale-completion checks,
asset lifetime management and worker scheduling belong to the future runner;
the types do not claim these runtimes exist. `unavailable_fence` returns the exact
original fence and diagnostic, so unavailable plugins do not block source editing.

The focused `extensions::tests::reader_callout_and_unavailable_renderer_preserve_source`
fixture covers wiki target/label behavior, code exclusion, attributes and origins,
callout transform, read-only origin clearing and byte-exact unavailable fallback.
Use the combined integration run's recorded evidence; avoid duplicate Cargo jobs.
