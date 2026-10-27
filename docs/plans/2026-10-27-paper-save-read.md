# Save and read research papers

**Goal:** Agents can retain a PDF and read its extracted text through shared commands, updating an existing reference without duplication.
**Architecture:** Reuse the bounded asset worker and PDF importer. SavePaper revision-checks an existing source, retains original bytes under research-assets, extracts page-numbered text off the owner thread, then atomically persists artifact/source linkage. Extraction failure still retains original and exposes error. ReadSource returns bounded UTF-8 excerpts with page numbers. URL collection requires automation write authorization; local file input remains local-only. No direct agent vault writes, DOI requirement, arbitrary destination paths, OCR, or automatic paywall bypass.
**Tech:** Existing Rust package, SQLite owner, curl, Poppler.

## Ownership / steps
- P1 asset worker: src/research_assets.rs and src/research_import.rs. Add AssetJob.extract_text bool; RetainedAsset.pages Option<Vec<SourcePage>>, extraction_error Option<String>. Reuse importer after retention (not UI thread); distinguish failed extraction from failed download. Agent owns these files only.
- P2 contract: src/research.rs. SavePaper{id,expected,source_id,input:ArtifactInput}; only archived URL/local file allowed at dispatch. ReadSource{id,source_id,page:usize,offset:usize,limit:usize} returns SourceExcerpt with title, paper metadata, physical page number, total bytes, next_offset and next_page. ResearchSource.paper serde-default Option<PaperContent{artifact_id,extraction_error:Option<String>}>. Agent owns file only.
- P3 root: src/app.rs, src/commands.rs, src/codex.rs, examples/docs. Integrate shared dispatch/async durability/stale source protection; permit SavePaper archived URL to AutomationReadWrite, keep LocalFile local-only, keep generic archive restrictions. Preserve reference provenance and source identity. Existing CollectArtifact submits extract_text=false. SavePaper only existing source; deduplicate by source.paper. Explain exact workflow in MCP/Codex guidance.
- Regression first: read paging and paper permissions/owner save-read, then focused tests. Root serializes Cargo. Independent review frozen integration; one real public PDF save/read/restart in disposable vault. Do not modify user's running vault/process during code validation. No commit requested.

## Result
P1/P2/P3 implemented. Regression-first validation reproduced missing schema/policy,
empty error-state rejection and metadata-retry loss; fixes verified. Combined host
checks and opt-in real PDF MCP save/read/restart passed; see docs/research.md.
Independent review corrections cover retained paper receipts and publication after
cancellation checks. Rebuilt Linux app/bridge require restart/reconnect to expose
new commands; no source migration or automatic user-vault edits.
