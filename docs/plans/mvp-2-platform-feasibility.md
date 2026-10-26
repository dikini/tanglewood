# MVP-2 P0: filter/platform feasibility

## Actual Linux result

Inspected local `/home/dikini/Projects/illustration-md`, clean HEAD exactly `9f38098d2355d4d77e41a538b48354fa8145d92a`. Read pinned `docs/contract.md`, `docs/comics.md`, `illustration_md/prepare.py` and `png.py`. Saved-artwork preparation uses Python's standard library only. The optional Node/browser/font authoring toolchain is not a preparation dependency.

Command run on Linux:

```sh
python3 docs/fixtures/mvp-2/prove-linux.py --upstream /home/dikini/Projects/illustration-md
```

Observed PASS with Pandoc 3.7.0.2: immutable logical source/assets snapshot → actual Pandoc Markdown reader → existing `docs/plan/examples/callout.lua` → Python preparation stage → actual JSON or standalone embedded HTML writer. Pandoc runs the ordered filter list; the shim accepts its `json`/`html` positional argument, then invokes `python3 -m illustration_md.prepare --source SNAPSHOT/source.md` without that positional argument. No live source directory is used. Both target-specific executions retain the JSON envelope and produce two native Figures with matching IDs/captions/alt text/classes/widths and a transformed callout Div/BlockQuote. HTML embeds actual SVG and PNG bytes. Source SHA-256 stays `3920d7763c21cc5bb0959f0ab90b85bfa59846e7cd4912186c97935b17f70efd`. Renaming away saved SVG causes upstream preparation failure (`existing local SVG`) with source unchanged and no generation attempt.

Retained run evidence: `/tmp/tanglewood-p0-filter-uz39xg6s/` (snapshot, pinned Python modules, actual prepared JSON, JSON/HTML outputs, semantic summary and rejection stderr). Fixture inputs and the reproducible script live in `docs/fixtures/mvp-2/`. The exact upstream illustration is 13,349 bytes; comic PNG is 3,660,354 bytes, 1536×1024. Two sidecars are retained unchanged, including opaque construction/provenance. No similar invented filter or regenerated image substitutes for upstream. `provenance.json` and upstream/font notices state the actual licensing boundary: user-authorized local research, no blanket upstream license or external redistribution assertion.

## Android decision and proposed next increment

Inventory only: actual `adb devices -l` found `emulator-5580`, `sdk_phone64_x86_64` / Android SDK built for x86_64. SDK path `/tmp/tanglewood-android-sdk`, NDK 29/API 35 available per the integration environment. Actual Android processing evidence is recorded below. The first sandboxed inventory could not open the ADB socket; approved escalated inventory succeeded. No Android UI/app mutation was performed.

Select a small local/offline Rust adapter for **illustration-md saved-artwork preparation subset v1 at the pinned commit**, not arbitrary Pandoc compatibility. Bundling Pandoc/Python would require separate Android ABI/runtime/package/lifecycle work; a host Linux executable is not an Android runtime. Host-assisted processing is not selected or assumed. No document leaves the device for this backend.

Implemented ownership: `src/illustration.rs` only, gated by optional `documents` feature; PM owns manifest/lib wiring. Owned API:

```rust
pub struct AssetFile { pub relative_path: String, pub bytes: Vec<u8> }
pub struct PreparationInput {
    pub source: String,
    pub logical_path: String,
    pub assets: Vec<AssetFile>,
}
pub fn prepare(input: PreparationInput)
    -> Result<PreparedIllustrations, PreparationError>;
```

Output is an owned list of figures: stable ID, kind, exact literal caption/description, optional positive millimetre width, exact original source byte range, resolved safe asset path, content hash, validated format and dimensions. The root's immutable snapshot/job identity wraps this input/output; it is not replaced by a local opaque revision. The module owns no World, SQLite, GUI, filesystem, process or dispatch handle. It never modifies source/sidecars, interprets construction or generates artwork. Root input needed: complete matching source text, logical relative Markdown path and validated snapshot file map (not arbitrary live filesystem paths). The downstream renderer receives figures plus original source/diagnostics rather than a second canonical model.

Use existing offset-aware native `markdown::read`/fence-attribute recognition for supported fences, and an explicit frontmatter metadata reader. Parse `diagrams` YAML with a real parser, retaining the original YAML bytes; support literal metadata needed by this fixture, including folded descriptions, quoted Unicode caption, unique stable IDs and optional width. Unsupported metadata constructs fail explicitly for this named subset instead of guessing Pandoc semantics. Unknown ordinary Markdown remains available through existing source/reader behavior; do not write a general Pandoc AST implementation or infer source origins from rendered equality/order. Frontmatter editing stays exact-source editing.

PM-enabled optional dependencies: existing `serde_json = 1` and `sha2 = 0.11`; `yaml-rust2 = =0.10.4` with default features disabled; `quick-xml = 0.41.0`; `png = 0.18.1`; `regex = 1.13.1`; `percent-encoding = 2.3.2`; `flate2 = 1.1.10` and `crc32fast = 1.5.3` provide exact bounded PNG decompression/checksums. All except YAML already appeared in the lockfile as dependencies. The YAML crate is pure Rust and UTF-8 input does not need its encoding feature ([pinned primary crate documentation](https://docs.rs/crate/yaml-rust2/0.10.4)); this adapter does not expose arbitrary YAML deserialization into application state.

Port/check pinned validator boundaries: exact integer sidecar version 1, kind-specific SVG/PNG reference, unique matching IDs, duplicates/unused/missing/kind disagreement rejected, local contained decoded asset paths with no scheme/query/fragment, positive finite SVG viewBox and element/resource allowlist, static noninterlaced 8-bit RGB/RGBA PNG with CRC/framing/scanline/dimension limits. Construction remains opaque. Apply small declared source/metadata/file/count/byte bounds in addition to upstream's image limits; report quota failures separately. Missing or invalid artwork is a retained-source error, never an authoring request.

The host comparison and actual Android-native execution completed after explicit PM source/Cargo/ADB push/run authorization; see the commands/results below. A temporary Android-native probe is a bounded first proof; APK integration/renderer presentation remains a later F1/I1 obligation. No existing Activity, APK, app data or UI state was changed.

## Capability matrix (current evidence)

| Capability | Linux | Android |
| --- | --- | --- |
| Real Pandoc reader and ordered Lua/Python stage, JSON/HTML targets | Observed pass above | Not selected/claimed |
| Actual pinned saved SVG+comic PNG preparation | Observed upstream and Rust parity pass | Same Rust code observed on API 35 x86_64 emulator; integrated APK presentation pending |
| General arbitrary external JSON/Lua filters | Process path demonstrated by real stages; bounded production runner still F1 work | Unsupported by selected subset |
| Source/assets atomic publication and authenticated delivery | Root P0/A1/A2 contract work; not proven by this snapshot script | Same; no transport claim here |

P0 platform feasibility now includes actual execution of the real fixture and representative rejection on Android. Production job runner, publication, renderer and integrated APK gates remain in their assigned phases; no wider compatibility is implied.


## Rust and representative Android execution evidence

Final source SHA-256: `dbce436e36cee31e750c848f022fa0eec157222fb8ed735ddc3415d6fa1ca8ea` (`src/illustration.rs`). The pure adapter finds fenced blocks through all offset events, including nested quote/list structure instead of relying on top-level render wrappers. Its host scenario also relocates the logical source to `chapter/source.md` with correspondingly staged snapshot files, while preserving source-relative output asset targets.

Declared additional processing quotas: source 1 MiB; frontmatter 64 KiB; descriptions/figures 64; file count 128; individual file 64 MiB; combined owned file bytes 80 MiB; SVG 4 MiB and nesting 256; PNG upstream limit 32 million pixels. These are processing limits, not canonical source-edit limits. Snapshot file keys are exact safe logical paths; percent-encoded sidecar references are decoded once and checked. A pure owned-byte adapter cannot validate a symlink or filesystem escape; the snapshot materializer remains responsible for filesystem containment.

The named source subset uses LF-delimited frontmatter, literal string fields and supported fence attributes. YAML anchors/aliases/tags and Markdown-formatted metadata are explicitly unsupported; source remains available when preparation fails. SVG declaration/entity/namespace/resource checks are intentionally bounded and conservative, not a universal SVG sanitizer. Existing-artwork preparation ignores opaque construction fields. Source-relative output paths are distinct from the snapshot lookup keys prefixed by the logical source directory.

Host command (one meaningful combined scenario):

```sh
RUSTC_WRAPPER= cargo test --locked --lib --no-default-features --features documents illustration::tests
```

PASS: 1 test, final run 1.21 s. It compares IDs/captions/alt text/width/path with the actual upstream result, validates saved SVG/PNG, rejects the same missing SVG, recognizes a quoted nested fence, and preserves relocation semantics. `RUSTC_WRAPPER` was cleared only after the configured sccache wrapper failed with `Operation not permitted`; an initial `--locked` attempt found the PM's new direct feature dependencies needed lock resolution, which the PM performed serially. No worker manifest/lock edits occurred.

Actual Android probe source is retained as `docs/fixtures/mvp-2/android-probe.rs`. Temporary harness `/tmp/tanglewood-p0-android-probe/` depends on this repository's library with only `documents` enabled; it does not copy/reimplement the adapter. Native cross-build command:

```sh
RUSTC_WRAPPER= \
CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=/tmp/tanglewood-android-sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android35-clang \
CC_x86_64_linux_android=/tmp/tanglewood-android-sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android35-clang \
AR_x86_64_linux_android=/tmp/tanglewood-android-sdk/ndk/29.0.14206865/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar \
cargo build --manifest-path /tmp/tanglewood-p0-android-probe/Cargo.toml --target x86_64-linux-android --offline
```

Build PASS (26.27 s), isolated harness lock SHA-256 `d3525d422f1682f56396ffc82fe3f86055a230df1e167994ca21b408823d13b2`. Native debug ELF SHA-256 `24e9f2297d2b82d6e15cca7c98e41ac05af5469b304a78a0dcc870cf862f94fc`. Its 57,341,688-byte debug size is not a release APK/install/memory measurement.

After authorized isolated `adb push` of the probe, source, expected semantic result and four fixture files, ran:

```sh
/tmp/tanglewood-android-sdk/platform-tools/adb -s emulator-5580 shell \
  /data/local/tmp/tanglewood-p0-documents/probe /data/local/tmp/tanglewood-p0-documents
```

Actual device result PASS on Android API 35 / x86_64: two figures match upstream metadata and source-relative asset paths, including the 65 mm SVG and 170 mm comic. SVG viewBox is 400×100, hash `af450d6b40c07dce0aa605716d24fbc81933c60841a2f00a949b81b132e3c609`; PNG is 1536×1024, hash `e67d959ff7036f0f56e42bfdcde1cfe6e542385acb99dec83fdf810239c46b3c`. The probe actually decodes the accepted comic pixels with `png`, compares source ranges and metadata, and rejects the same absent SVG. Output: `/tmp/tanglewood-p0-android-probe/android-result.json`, also retained in the fixture; SHA-256 `3a3c1c15705471e5855ddd6010ef6a6fee15915b6a8acf30188a9c3199a93233`.

No external runtime, Pandoc, Python, authoring tool or network request executes on Android; the adapter consumes owned local bytes. The emulator network was not disabled, so this proves a local/offline-capable implementation rather than a measured airplane-mode UI flow. This is actual Android-native backend execution, not host-only compilation, integrated APK rendering or physical-device evidence. The existing Tanglewood APK/data/UI was left intact. The isolated probe and evidence remain available for integration review.
