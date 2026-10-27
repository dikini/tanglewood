# Desktop research completion

**Goal:** Complete the previously discussed research workflow on desktop, with UX taking precedence over exposed storage forms. User explicitly defers sync/mobile and multi-agent research orchestration; other research capabilities remain in scope. Broader editor/WASM/app features are not this task.
**Architecture:** Keep the quiet sources-left / existing conversation-center / outcomes-right shell. One App owns revision-checked workspace/source/job/output records. Background workers handle network, file selection, extraction/OCR and PDF-page rasterization. Source content is untrusted evidence. No new persistent document engine, independent chat, global permission change, or automatic external publication.
**UX review:** Sol 6.1 xhigh requested by user; initial recommendation preserves shell and separates candidate selection, inclusion, availability, assessment and provenance. Detailed review will refine acceptance before GUI integration.

## Required slices and ownership
1. Pure workflow helpers (developer): src/research_desktop.rs new. Reference extraction/dedup from Markdown answers, explicit reference candidates, lexical passage retrieval with citations and evidence-gap/assessment suggestions, generated output data/render helpers (slides HTML, infographic/mind-map SVG). No network/UI/database owner.
2. Acquisition/reader worker (developer): src/research_assets.rs, src/research_import.rs, src/research_reader.rs new. Resolve direct PDFs/landing-page metadata/arXiv/DOI redirects; bounded native discovery adapter; explicit OCR via Poppler/Tesseract; async native picker and PDF rasterizer. Preserve immutable originals, bounded processes/cancellation, existing save/read contracts. No UI/app/domain writes.
3. Root PM owns src/research.rs, src/app.rs, src/commands.rs, src/lib.rs, src/codex.rs, src/mcp.rs and docs/examples. Persist acquisition queue/status/retry, candidate collection, roles and generated outputs; selected-source retrieval; never overwrite a newer revision or authored output. Root serializes Cargo/dependencies/builds/index.
4. Desktop UI developer (after design handoff): src/ui/research.rs only. Contextual Collect references with independent candidate selection; Add selected/all; native file picker/drop; source availability/actions; batch download progress/retry; text/PDF reader/search/OCR; evidence panel and explicit report/slide/infographic/map output actions/readiness. Root handles src/ui.rs and transcript/note citation event integration.
5. Sol6.1 xhigh reviewer: initial UX review; stable wide/narrow native screen review and final correctness/recovery review. No edits or Cargo.

## Interaction contract
- Collecting references is explicit, deduplicates normalized URLs, supports ordinary links/files without DOI, retains origin thread/answer context. Add all reports skipped duplicates.
- Source collection is independent of agent turns; workspace-changing controls serialize while an acquisition commits. Downloads display reference/queued/fetching/ready/failed states on rows. Durable queue resumes safely after restart; interrupted active operations become retryable. Retry never duplicates an existing source or overwrites edited metadata.
- One unified add-file retains original and extracted source text; batch queue serializes workspace mutations. Drag/drop/native picker does not silently send to an agent.
- Source reader preserves chat drafts/scroll, has Back, page/search/Text-PDF controls and explicit OCR. Citations open an exact stable source/page, with missing-source diagnostics. No guessed current-note offsets.
- Discovery returns uncollected candidates from an explicitly requested provider search; no source permanently added without collection. Relevant-passage retrieval uses selected local sources; disclose matched snippets and context budget, no claim of semantic/verified trust scoring.
- Triage records contextual human/agent assessment separately with decision history. Suggestions prepare prompts, never silently send/enable network.
- Explicit Input/Output artifact roles; old unclassified records remain Unclassified rather than guessed. Outputs include editable Markdown reports plus real local slide HTML and SVG infographic/mindmap renderings. A requested model/skill prepares content; local rendering/export needs no external publishing. Missing optional runtime dependencies show actionable readiness.

## Validation / stopping
Regression-first checks cover normalized reference dedup, bounded exact retrieval/citations, queue retry/restart/revocation/source identity, role legacy compatibility and safe output rendering. Review existing tests once at integration; reuse unchanged tests. Native wide/narrow/empty/populated review covers collect→acquire→read→cite→output with actual files; live public PDF/OCR/provider checks use disposable vaults. No claim of mobile/sync/multi-agent acceptance. No commit/publication requested.

## Integration decisions
- Discovery uses Crossref public paper metadata and returns uncollected candidates; general web investigation remains available through the existing agent runtime.
- OCR is explicit and currently English. It never replaces a readable generation if recognition fails.
- Local passage matching is lexical, opt-in per question, and previewed; exclusions and current workspace revision bind every attachment. It is not semantic verification.
- Reference collection, multi-file import and retries persist batch intent in a single command. An interrupted running job is retryable; already committed queued jobs remain queued. Storage errors pause automatic scheduling until explicit retry.
- Output rendering uses local standalone HTML slides/SVG infographic/mind map cards. Model/skill synthesis is an explicit prepared request; native card editing and rendering work without a model. Exported citations resolve to owner-derived original-file basenames or provenance HTTP(S) URLs, preserving page locators and avoiding a custom OS URL handler.
- Existing unclassified artifact roles remain Unclassified. Original file bytes and source identities stay unchanged; no mobile/sync/orchestrator increment.

## Delivery checkpoint

All required slices implemented. Frozen independent Sol6.1 xhigh static review found
no remaining blocker after workspace/query cache binding, failed-OCR preservation,
answer provenance, stale discovery, file-kind guards and search navigation fixes.
Native wide/narrow, retained PDF, chooser/import, collection and actual outputs
are captured in docs/design; final visual recommendation recorded in research.md.
Combined library checkpoint and final focused gui/headless tests/build passed;
actual OCR and live Crossref checked. See docs/research.md for commands, scope and
limits. No Android/sync/orchestration or new paid agent-turn claim. No commit.
