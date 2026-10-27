# Research workspaces

A workspace is an ongoing investigation across conversations and sessions. It
retains its brief, evidence, checkpoint rounds, associated conversations, artifacts
and ordinary Markdown research/report notes. The domain is unrestricted: practical,
creative and academic research all use the same records.

[Design](plans/2026-10-27-research-workspaces-design.md) ·
[Flow/artifact plan](plans/2026-10-27-research-flow-and-artifacts.md)

## Research layout

Research uses the existing Codex conversation as its central working surface.
Sources and collected material stay on the left; linked notes and saved findings
appear under Outcomes on the right. The workspace picker changes the surrounding
research collection; it does not silently switch or merge conversations. Use the
Conversations control to choose a chat. Preparing context or opening an associated
chat keeps the research layout visible.

Add source opens reference collection, native multi-file import or provider discovery.
Sources open in the central reader, with Back returning to the conversation.
The reader supports exact extracted text, retained PDF pages, page navigation,
search with highlighted hits and explicit OCR. Assessments and provenance remain
contextual. Research details holds the brief and checkpoints.

Collected originals are Inputs; generated files are Outputs. Older records retain
an explicit Unclassified role. At narrow widths, Sources and Outcomes open on
request. The [desktop completion plan](plans/2026-10-27-desktop-research-completion.md)
supersedes the earlier UI-only gap list. Historical checkpoints below describe
what was available at each earlier stage.

## Desktop research completion — current

- Collect links from a specific assistant answer, preview candidates and Add selected/all;
  normalized duplicates are skipped. DOI is optional; arXiv versions remain distinct.
- Import files through the native picker or drop them in Sources. Text/Markdown/PDF
  originals are retained with readable text; a batch persists its intent atomically.
- Get full text resolves supported PDF landing-page metadata/redirects and arXiv links.
  Durable queued/running/ready/failed/cancelled state supports retry; interrupted active
  jobs become retryable. Storage errors pause scheduling and expose Retry queue.
- Read citations at their exact source/page. Search highlights exact text and scrolls
  to the selected hit. PDF rendering needs Poppler; explicit English OCR additionally
  needs Tesseract. Failed OCR preserves any previously readable generation.
- Discover papers through Crossref metadata, then explicitly collect candidates.
  General web investigation continues through the configured agent runtime.
- Relevant evidence is opt-in in the composer: bounded lexical snippets from selected
  local sources are previewed for the current question before Send. This is lexical
  matching, not semantic verification. Assessment history and evidence-gap suggestions
  preserve human and agent judgments separately.
- Create report notes, standalone HTML slides and SVG infographic/mind-map cards.
  Local card editing/rendering works without a model; synthesis prepares a request
  for explicit Send. Outputs retain cited source IDs and link to originals or URLs.
  These are deliberately simple local templates, not presentation-suite editing.

Runtime readiness explains missing curl, Poppler, Tesseract or native chooser tools.
Local host-file operations remain local-only; agent URL writes require vault access.
Sync/mobile and multi-agent research orchestration are deferred. Restart the rebuilt
Linux app and reconnect Codex to refresh its tools and instructions.

### Completion evidence

Independent Sol 6.1 xhigh final code/native UX review: READY for desktop MVP.
The final minor copy finding (Notes empty state contradicting saved outputs) is corrected.

Combined gui,codex library checkpoint: 93 passed, 2 ignored, 2 filtered; one additional
loopback PDF fixture initially failed because the sandbox prohibited binding a socket
and passed alone with local-network permission. Final targeted checks after corrections:
5 desktop tests in both gui,codex and headless mcp builds; exact citation conversion,
reader text rhythm and Unicode search/output-content guard each passed. Final bins and
research_demo build passed. Actual OCR regression and live Crossref query passed.
Native disposable-vault checks exercised candidate deduplication, native file selection,
retained Markdown import, exact citation→61-page PDF/text reading and local cited HTML
output. HTML was browser-rendered; both SVG formats were rasterized for visual review.
No new paid model turn, sync/mobile or multi-agent acceptance is claimed.

Screenshots: [desktop](design/research-desktop-complete-linux.png),
[PDF reader](design/research-desktop-pdf-linux.png),
[highlighted search](design/research-desktop-search-linux.png),
[narrow reader](design/research-desktop-narrow-reader-linux.png),
[slides](design/research-slides-output.png),
[infographic](design/research-infographic-output.png),
[mind map](design/research-mindmap-output.png).

## Quiet UI acceptance (2026-10-27)

- Independent Sol 6.1 xhigh design and final code review: READY, no remaining blockers.
- `RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex ui::`: 26 passed. Includes request transfer preserving newer/unrelated drafts and the existing conversation preparation/access guards.
- `RUSTC_WRAPPER= cargo test --locked --offline --no-default-features --features gui --lib ui::research`: 5 passed.
- `RUSTC_WRAPPER= cargo build --locked --offline --bin tanglewood --example research_demo --features gui,codex`: passed; two existing unused theme-helper warnings.
- `cargo fmt --check` and `git diff --check`: passed.

Native Linux inspection used Xvfb and a disposable copy of the earlier research
fixture. The developer-only `research_demo` renders the real shell and conversation
with deterministic local messages, without connecting an agent. Inspected the
1360×920 populated workspace, a real empty-vault main window, source preview,
report dialog, one-action preparation into the conversation, refusal to replace
an existing conversation draft, and 500×850 source/chat/outcome navigation. The
narrow source drawer fills the available width and provides a return control.

Screenshots: [wide](design/research-quiet-linux.png),
[narrow conversation](design/research-quiet-narrow-linux.png).
No live provider turn, Android device, new source-discovery behavior or output
generator was exercised or added in this UI pass. Prior backend evidence below
remains historical, separate from this UI acceptance.

## Start and continue an investigation

Run the editor with `cargo run --features gui -- --vault /path/to/vault`.
The folder-shaped Research rail action opens workspaces. Create a title/question
without needing sources first. In the optional Codex interface, Start research
from a connected, project-listed conversation opens an editable bounded discussion
capture. Creation retains that context as an initial checkpoint and associates
the conversation; it never sends a model turn.

In Research details, edit the investigation brief: purpose, scope, assumptions and open questions.
Associate multiple conversations from the current verified vault-project list;
opening one uses the normal draft-preserving conversation switch. Linking threads
does not merge transcripts. Unavailable/unlisted threads cannot be resumed by an
arbitrary supplied ID. Each thread retains its own conversation and draft context.

Add immutable checkpoints for successive rounds. A checkpoint records intent,
findings, open questions, discussion context, conversation/turn provenance when
known and selected artifact references. Capture the current assistant answer or
write a checkpoint manually. Existing checkpoint drafts must be saved/discarded
before another answer replaces them. Discussion and recorded summaries are context,
not independently verified evidence. Older checkpoints are preserved.

Continue research builds an inspectable context from the current brief, recent
checkpoint excerpts, artifact metadata and selected source packets. Full discussions
and archived bodies remain saved; the context explicitly discloses excerpts and
omitted counts. Read-only `Research ReadIndex` lists all stable source/round/artifact
IDs without bodies, `ReadRound` retrieves a full checkpoint, and `ReadArtifact`
returns exact UTF-8 Markdown pages (offset/limit, maximum 32 KiB, next_offset).
The context is bounded to 64 KiB, so selected complete source packets may still
need reduction. This does not delete stored evidence.

Choose existing-evidence-only or explicitly enable external gathering. Prepare in
conversation requires a selected thread, an empty composer/context and a sandboxed
Access profile. It creates a readable question and named context attachment;
network permission follows the explicitly selected scope. Preparation never sends.
Use normal Send to run the agent with its configured capabilities. Actual external
search depends on that runtime; there is no built-in new search service here.
Copy context works without Codex. Configured skills remain available through the
existing catalog; service/dependency readiness automation is a separate follow-up.

## Collect evidence and artifacts

These are related but distinct collections:

- Sources: immutable note snapshots and page-aware extracted PDF/text/Markdown
  evidence, with include/exclude checkboxes and separate human/agent assessments.
- Artifacts: retained originals (PDFs, raster images/infographics, and other bounded
  local files), URL references, supplied Markdown, and optional fetched web copies.

Add sources captures a current note or extracts an explicitly selected local file.
Source edits never change an earlier snapshot. Linux PDF text extraction requires
Poppler's `pdftotext`; scanned documents need an external OCR step. Copy a citation
such as `[source:UUID page:N]` into Open citation to navigate to the retained page.
Automatic source-citation links from arbitrary notes/transcripts are not implemented.

Artifacts > Collect artifact copies local originals without changing their bytes,
records an HTTP(S) reference without fetching, or retains supplied Markdown. Enabling
Retain a Markdown web copy explicitly fetches and converts static HTML using local
curl/Pandoc. It retains source/final URL, capture time and Markdown. Conversion is
not a full website archive: it does not run JavaScript, authenticate to services,
archive linked media or silently treat fetch failure as success. Remote PDF URLs
can be references; retaining the PDF original uses the local-file collector.

Artifact details distinguish reference-only from retained copies. Read retained
Markdown in the workspace; copy retained file paths for other applications.
Open retained image is limited to supported raster extensions. Generic files,
HTML and SVG are never automatically executed/rendered as active web content.
Generated artifacts can be collected by the same route; this increment does not
generate infographics/decks itself.

Originals live under vault-owned `research-assets/` with generated immutable names.
Collection runs off the UI thread with cancellation, size limits and subprocess
deadlines. It becomes a workspace artifact only after SQLite acknowledgement.
If registration fails after retention, the file remains and the current session
offers an explicit registration retry. Failed-registration receipts are memory-only;
a restart preserves copied bytes but does not reconstruct an unregistered receipt.
Successful artifact records and files survive restart. Do not move managed files
outside the app or assume LAN sync includes them.

Automation can record URL-only references and supplied Markdown through the same
Research commands and read-write policy. Arbitrary local-file copying, network
archival and registration retry are local-only. No adapter can forge a managed
copy path or bypass the single App/SQLite owner.

## Reports and research notes

Create report note now assembles the saved brief, checkpoint findings, open
questions and source/artifact references into a NEW ordinary Markdown note. It
needs no model call. It identifies recorded findings and reference-only material
honestly. It saves the note before linking it and rejects an occupied path rather
than overwriting an earlier report. After another research round, create another
report at a new path; earlier reports and user edits remain unchanged.

Ask agent for report synthesis is a separate optional conversation draft. Normal
Send requests a new cited note through the existing MCP tools; `--mcp-write` is
needed to save one. Unavailable tools must not be reported as a saved report.
See [Codex setup](codex.md) for `gui,codex`, socket and connection configuration.
Any ordinary research note can still be created/linked independently and edited
through the usual source-preserving commands.

## Limits and scope

Workspace/source/flow data remains local-only; no new CRDT/asset sync is claimed.
Existing JSON records load with defaulted brief/thread/round/artifact fields.
Workspace changes are CAS/revision checked and publish only after durable save.
On errors, prior committed state and UI drafts remain available.

Local originals are limited to 32 MiB; Markdown captures to 8 MiB; asset jobs and
completions to eight outstanding requests; archive subprocesses to 20 seconds.
PDF extraction retains its 8 MiB/500-page limit. Source text and flow/archive text
have separate 16 MiB workspace budgets. Reports are capped at 2 MiB. The schema
bounds associated threads (128), rounds (512), artifacts (512) and sources (128).
Source/assessment/source-only prompt limits from the initial increment still apply.
Read/List return full records and can exceed transport limits for large workspaces;
use ReadIndex and paginated artifact reads for bounded agent retrieval.

Discovery-provider integration, OCR, automatic trust scoring, multi-agent runtime,
automatic artifact ingestion from every agent output, and new infographic/deck/map
renderers remain follow-ups. No external service or skill collection was installed.
Configured workflow skills may produce outputs using their own available tools.
Android research UI/IME/device acceptance, LAN attachment sync and live research
model-generation success were not established by these host checks.

## Initial workspace increment evidence

Integration evidence is recorded below after focused review and checks. Developer
agents used Sol 6.1 medium; independent review used Sol 6.1 xhigh. Existing unrelated
working-tree changes were preserved; this follow-up does not publish anything.

- `RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --offline`:
  17 passed. Real SQLite scenario covers rejected read-only/stale mutations,
  save-before-link, immutable note snapshots after source edit, restart and a
  failing SQLite trigger retaining committed workspace state.
- `RUSTC_WRAPPER= cargo test --locked --lib --features gui,codex --offline`:
  coherent integration checkpoint 61 passed, 1 optional external-Pandoc test ignored.
  After review fixes, the `research` filter passed 8 focused tests. The final
  `research_preparation` filter passed 1 updated scenario for exact evidence
  attachments, no-thread rejection, and switch-away/back draft ownership.
  These targeted reruns replace redundant full-suite repetition.
- `RUSTC_WRAPPER= cargo test --locked --lib --features mcp --offline
  owner_socket_routes_policy_revision_and_durable_mutations`: 1 passed, including
  an actual Research::Create round trip that waits for SQLite acknowledgement.
- `RUSTC_WRAPPER= cargo clippy --locked --offline --all-targets --features
  gui,codex`: exit 0, with nonblocking warnings (77 lib-test warnings including
  duplicates); no broad lint auto-fix. `cargo fmt --check` and `git diff --check`
  passed. Formatting included two existing cfg attributes in transcript_demo.rs.
- Final `RUSTC_WRAPPER= cargo build --locked --offline --bins --features
  gui,codex`: exit 0, two existing unused theme helper warnings.
- Native Linux/Xvfb `:83`, isolated vault
  `/tmp/tanglewood-research-smoke-20261027`: created Small boat workshop; captured
  an existing Markdown note; imported an actual installed Poppler-readable PDF
  (`letter-example-18-en.pdf`) through the background UI worker; opened physical
  Page 1; saved human relevance/quality/limitations; excluded the unrelated PDF
  from the selected evidence; built a grounded question. No model turn was sent.
- Restarted with the final binary: workspace, both captured sources, page text,
  selection and assessment survived. Created `Research/Small boat/Summary.md`
  through the UI; observed durable note creation then workspace linking and
  confirmed both records through read-only SQLite. Pasted the PDF citation into
  Open citation and observed the correct retained page. Source fixture is a sample
  letter, deliberately assessed as irrelevant to the boat question.
- Actual final native capture: [research-workspace-linux.png](design/research-workspace-linux.png).
  It shows the quiet rail/sidebar, selected sources, linked Summary note and
  contextual continuation composer. The earlier native manual handoff exposed
  source JSON in the composer; final reviewed code uses a named evidence attachment
  instead, verified by the focused conversation ownership fixture. No live AI
  generation, Android/device check, network discovery, OCR or output export is claimed.
- Independent Sol 6.1 xhigh review: READY after corrections for asynchronous MCP
  acknowledgement, saved-note link ordering, source-selection/follow-up prompt
  invalidation, cancelled-import correlation, and first-use conversation ownership.

Changes are left in the existing shared working tree alongside earlier work;
no commit, push, publication, or model-generation request was made.

## Research flow and artifact increment evidence

Plan: `plans/2026-10-27-research-flow-and-artifacts.md`. Three Sol 6.1 medium
developers worked concurrently in disjoint domain/worker/UI paths; independent
Sol 6.1 xhigh review concluded READY after scoped corrections. No shared-index or
remote publication operations were performed.

- Combined `RUSTC_WRAPPER= cargo test --locked --offline --lib --features gui,codex`:
  71 passed, 1 optional external-Pandoc test ignored at the coherent checkpoint.
  Review-driven focused reruns: GUI/Codex `research` filter 20 passed; final
  `research::flow_tests` filter 5 passed; final no-default-features `research`
  filter 15 passed. Relevant tests cover old JSON defaults, two thread identities,
  immutable rounds, original file/Markdown retention, failed SQLite registration
  and receipt retry, dirty draft protection, idle creation progress, bounded context
  and exact UTF-8 artifact pagination. A 10 MiB workspace's metadata index remains
  below 4 KiB and exposes IDs omitted from context.
- Final `RUSTC_WRAPPER= cargo build --locked --offline --bins --features gui,codex`:
  exit 0, two existing unused theme helper warnings. All-target gui,codex Clippy
  completed with nonblocking warnings; no broad auto-fix. `cargo fmt --check` and
  `git diff --check` passed. Dependencies add direct `url` and `time`, already
  present in the lock graph, for strict URL parsing and readable UTC timestamps.
- Native Xvfb `:84`, vault `/tmp/tanglewood-flow-smoke-20261027`: workspace with
  editable brief, two explicitly marked fixture thread references, two successive
  checkpoints and five artifacts. Thread/round fixture records were seeded through
  actual MCP; actual thread switching/capture attribution was verified by focused
  GUI controller tests, not a new live provider conversation.
- Through the actual native artifact form, retained the installed sample PDF and
  a raster visual artifact; read-only verification confirmed exact original bytes
  in vault-owned files. Through the native web-copy checkbox, fetched an actual
  local HTTP fixture at `127.0.0.1:8768/index.html` with curl and converted it using
  installed Pandoc; URL/final URL and exact retained Markdown matched its file.
  URL-only and supplied-Markdown records remained distinguishable.
- Created `Reports/Small boat checkpoint report.md` through the native report action.
  Added a second round, then created `Reports/Small boat updated report.md` at a new
  path through the same action. Both were linked after durable note saves. SQLite
  verification confirmed the first report's exact bytes were unchanged and the
  second included the later round, all artifacts and correct retention labels.
- Restarted the final binary and compared the complete workspace JSON: two thread
  references, two rounds, five artifacts and two report links were identical.
  Actual MCP ReadIndex returned all IDs without bodies and ReadArtifact returned
  the exact archived Markdown. Copied originals and generated reports remained.
- Actual final screenshot: [research-flow-linux.png](design/research-flow-linux.png).
  Shows earlier and later reports, chronological checkpoints and the mixed artifact
  collection. Fixture material demonstrates workflow, not real boat-design findings.
- Corrections from review/integration: parent asset directory fsync; no checkpoint
  draft replacement on capture; repaint for skipped source-free creation steps;
  raster-only external opening; disclosed bounded context with exact reads; compact
  body-free index for workspaces exceeding MCP response limits. Originals and prior
  notes/checkpoints are preserved throughout.

No paid model turn, new search provider, skill installation, Android/device run,
LAN artifact sync or live multi-agent research execution was performed. Failed
artifact registration receipts remain session-local as described above. Changes
remain uncommitted alongside earlier working-tree changes.

## Save and read papers through an agent

Restart the rebuilt application and reconnect Codex to refresh the MCP schema and
child instructions. The vault must authorize agent writes. An ordinary URL or
file provenance is sufficient; DOI is optional. Existing reference-only sources
can be upgraded in place:

1. Use `Research List`, `ReadIndex`/`Read` to obtain the workspace, current revision
   and source ID. If needed, first register a reference using `AddImported` with
   kind Text and an explicitly labeled reference-only page; never invent an abstract.
2. Resolve the reference/landing page to a legally accessible **direct PDF URL**.
   Call `Research SavePaper {id, expected, source_id, input: {Url: {url, archive: true}}}`.
   The mutation runs through the shared owner and waits for durable confirmation.
3. Re-read workspace metadata. `source.paper.artifact_id` identifies the retained
   original in workspace artifacts; `managed_path` is a vault-relative immutable
   file under `research-assets/`. Source identity and original metadata are kept.
4. Check `source.paper.extraction_error`. Null means page text was extracted;
   otherwise the original is saved but OCR/password/unsupported-runtime or other
   extraction work is needed. Reference text is never substituted for PDF text.
5. Read `Research ReadSource {id, source_id, page: 0, offset: 0, limit: 32768}`.
   Follow `next_offset` on the same page, then `next_page` at offset zero. The
   response preserves physical page numbers and UTF-8 byte boundaries.

SavePaper accepts authorized HTTP(S) downloads through the owner's bounded
worker, independent of shell access. LocalFile input remains local-only; agents
cannot use it to read arbitrary host files. Generic URL-archive/local-file tool
restrictions remain intact. Supported landing-page metadata and redirects resolve to a PDF; unsupported HTML
returns a diagnostic without treating HTML as a PDF. Originals are capped at 32 MiB; extraction at 500 pages / 8 MiB;
network and extraction subprocesses have existing time/cancellation bounds. Linux
requires curl and Poppler's pdftotext. Scanned PDFs are retained with an extraction
error; explicit OCR is available in the desktop reader. Files are not synchronized to other devices.

A second SavePaper on an already-linked source is rejected to avoid duplicate
papers. After uncertain transport outcome, read state before retrying. If metadata
persistence fails, the current owner retains the original and extracted payload
in a receipt; local RetryArtifact restores both source linkage and content under
a fresh workspace revision. Low-level SavePaper receipts are in-memory; the desktop acquisition queue separately
persists batch intent and retry state; retained originals are not deleted by an interrupted metadata registration.

### Paper validation — 2026-10-27

Combined gui,codex library run: 82 passed, 1 ignored, 1 filtered; the sole failed
socket test was blocked by sandbox Unix-socket creation and subsequently passed
alone outside the sandbox. New paper fixtures exercise UTF-8/page continuations,
legacy source records, authorization, actual PDF extraction/exact original bytes,
HTML rejection and retained malformed-PDF errors. Review found two recovery gaps:
publication now follows extraction/cancellation checks, and retry receipts retain
source linkage plus extracted pages/error. The augmented SQLite-failure regression
first reproduced the lost-link bug; after correction the headless mcp `paper`
filter passed all 5 tests. No Android or paid model turn is claimed.

Real MCP end-to-end smoke:
`python3 examples/paper_smoke.py --url https://arxiv.org/pdf/2602.23927`.
A disposable vault downloaded 985663 PDF bytes, extracted 226784 UTF-8 bytes from
61 pages, and read every page through the actual MCP read tool. After stopping and
restarting its owner, source identity, complete workspace, original bytes and
extracted text all matched. Text SHA-256:
`92d6ffbf5e7e23bf58d26176513bb15937e2e68d2b7423891f85e01d9969d749`.
The test did not change the user's running vault or its six existing references.
