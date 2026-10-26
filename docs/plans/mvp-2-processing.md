# F1 reduced document processing

The controlling reduced scope in `2026-10-26-mvp-2-proposal.md` applies. This is one owned-byte request path, not plugin discovery, durable asset publication, asset sync or HTML export.

## Contract

`document_jobs::DocumentSnapshot` owns note ID, opaque revision, exact source, safe logical Markdown path and explicitly selected `illustration::AssetFile` bytes. File keys are relative to a snapshot root; figure asset references are relative to the source's directory. `ProcessedDocument::figure_resource` resolves that distinction without live filesystem access. The owner must snapshot selected files before submission; a concurrent live-directory read is not an atomic directory snapshot. The input contains no arbitrary filesystem handles.

`DocumentJobs::submit(snapshot, config)` assigns a request generation and invalidates all earlier outcomes, even on invalid replacement input. At most one worker runs and one latest pending request is retained. A replacement cancels the active request; `poll` joins only an already finished worker, starts the latest pending request, and discards old generations. Resource/config changes require a new actual request even if the note revision is unchanged. Do not submit or clone/hash resources every UI frame. The owner must additionally compare the returned note ID/revision with the current requested note/revision before installing a result. Cancellation and worker results never dispatch source commands.

The optional `documents` feature exposes these modules; Linux uses its existing optional libc dependency for process-group termination. Android does not compile this process code. Results retain original source/resources alongside optional semantic Pandoc JSON and the existing native subset's exact-range figures. No egui or World handles cross the worker boundary.

## Backends and limits

`NativeIllustrations` calls the demonstrated `illustration::prepare` named subset only when the offset-aware Markdown scan identifies an illustration/comic fence (including nested blocks). Without artwork fences, ordinary notes bypass that subset's restricted metadata parser; their original frontmatter remains unchanged. Malformed intended artwork fences stay on strict validation. Linux applies the same artwork-only gate after Pandoc, so ordinary Pandoc metadata does not acquire Android-subset restrictions. It performs no process or network operation and is the Android backend. Its source ranges remain tied to the exact original snapshot. It is not arbitrary Pandoc compatibility. Cancellation is observed before and after this bounded pure preparation, not during every validation instruction.

`Pandoc(PandocConfig)` is Linux-only. The configured absolute Pandoc executable reads the private snapshot as Markdown into its actual JSON envelope. Ordered `FilterStage::Lua`, `Json`, and `Preparation` stages then execute in order. Lua uses actual Pandoc JSON-to-JSON execution; standard JSON executables receive target `json`. Preparation is explicitly different: its configured program/arguments receive appended `--source SNAPSHOT_PATH`, with incoming/outgoing Pandoc JSON on stdin/stdout. Thus Python `illustration_md.prepare` is not misrepresented as a standard Pandoc executable filter. Every stage output must retain the JSON envelope. The native illustration adapter also supplies validated saved-artwork figure metadata/resources to the renderer. General transformed Pandoc output has no inferred source bindings.

Programs are explicitly configured trusted local code, not sandboxed untrusted plugins. They inherit the application environment. No shell is invoked. Executable/script installation and configuration are local responsibilities; there is no hidden upstream checkout assumption in the default config. The default is `/usr/bin/pandoc`, no filters, a 15-second whole-pipeline deadline, and 16 MiB stdout per stage. Stderr is independently capped at 64 KiB. Configured stdout is capped at 64 MiB, timeout at 120 seconds and stages at 16. Each child has its own process group; cancellation, deadline/output failure and normal completion terminate remaining descendants and reap the direct child. A malicious executable that escapes its process group is outside this trusted-local-code boundary.

The private UUID-named temporary directory is mode 0700 and removed when execution ends. Source/resources are materialized only from owned bytes; resource files use create-new. Safe exact relative keys reject traversal, backslashes, aliases, duplicates and source collisions. Source is capped at 1 MiB, resources at 128 files, 64 MiB per file and 80 MiB total. The existing illustration adapter enforces its own image/metadata quotas. These are processing limits; the canonical note is never truncated or modified. Stage programs can modify their disposable snapshot, never the input's original filesystem location through a passed live path.

## Validation

Focused checks are `document_jobs::tests::latest_request_and_resource_validation` (latest-only result and traversal rejection) and `pandoc::linux::process_tests::deadline_output_and_cancel_are_bounded` (actual child timeout/output/cancel). The opt-in actual ordered fixture check is:

```sh
TANGLEWOOD_ILLUSTRATION_MD=/absolute/pinned/illustration-md \
RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features documents \
  pandoc::tests::real_ordered_fixture -- --ignored
```

It executes real Pandoc, the repository callout Lua and explicitly selected upstream Python preparation, requires two figures and the callout transformation, rejects missing SVG, and verifies original fixture source unchanged. Observed Linux validation on 2026-10-26 after the PM released the shared Cargo slot:

- `env RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features documents document_jobs::tests`: PASS, 1 test (4.64 s compile, 0.00 s test).
- `env TANGLEWOOD_ILLUSTRATION_MD=/home/dikini/Projects/illustration-md RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features documents pandoc:: -- --include-ignored`: PASS, 2 tests (0.40 s tests). Actual `/usr/bin/pandoc`, repository Lua and selected upstream Python ran; two figures/callout and missing-SVG rejection passed. Child deadline/output/cancel and a successful parent leaving a pipe-holding descendant were exercised.

Only assigned new Rust files were formatted; no manifest, shared index or commits were modified by this worker. No new Android device evidence is claimed here; the pure adapter's existing native probe evidence remains in `mvp-2-platform-feasibility.md`.

Review correction: the focused job test first reproduced rejection of ordinary `title: Hello!` metadata (`Illustration(Unsupported(...))`), then passed after the artwork-fence gate. Command: `env RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features documents document_jobs::tests` (1 passed, 0.01 s test; 1.36 s compile). Original source equality and no manufactured figures are asserted.
