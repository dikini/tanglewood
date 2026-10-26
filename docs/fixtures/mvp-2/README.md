# Pinned saved-artwork fixture

This local preparation fixture uses actual [illustration-md](https://github.com/dikini/illustration-md/tree/9f38098d2355d4d77e41a538b48354fa8145d92a) saved artwork, original metadata and prose briefs at that exact revision. It contains one SVG illustration and one PNG comic, with their exact sidecars. `provenance.json` records origin and byte hashes. Construction fields remain opaque; missing authoring inputs referenced there do not trigger generation.

Upstream does not declare a blanket project license. Its provenance notice and relevant outlined-font license records are retained in `upstream-notices/`. The comic's generated-artwork provenance remains in its sidecar. These files support the user's authorized local integration work; no external publication or redistribution grant is asserted.

Run the real Linux proof (requires installed Pandoc and Python, plus access to the pinned Git object):

```sh
python3 docs/fixtures/mvp-2/prove-linux.py --upstream /home/dikini/Projects/illustration-md
```

The script materializes an isolated source/assets snapshot and exact upstream preparation modules, runs the existing Lua callout before Python preparation, and invokes actual `json` and `html` writers. The shim consumes Pandoc's positional writer argument and passes only `--source SNAPSHOT` to upstream. Output is retained in a printed `/tmp/tanglewood-p0-filter-*` directory. JSON semantic figure output is compared across both targets; HTML embeds both assets. A missing-SVG rejection verifies the source stays unchanged. No Node, browser, font authoring tool or image generation runs. `expected-semantic.json` is a small observed upstream result, not hand-invented filter output.

`android-probe.rs` is the small actual-device harness used by P0. It depends on the repository library with `default-features = false, features = ["documents"]`, plus `serde_json` and `png`; it does not reimplement preparation. The temporary Cargo project and target binary remain at `/tmp/tanglewood-p0-android-probe/`. `android-result.json` is its actual API 35 x86_64 emulator result, including a real comic pixel decode and the same missing-SVG rejection. Exact build/run commands and limits are in `docs/plans/mvp-2-platform-feasibility.md`. This native runtime proof does not claim integrated APK rendering or arbitrary Android Pandoc/Lua support.

## Saved image references in Markdown

The user-facing file example is [`with-image-references.md`](with-image-references.md). Its YAML `diagrams` entries contain `image` and `media-type`, alongside their IDs/captions/descriptions. The original prompts remain in fenced blocks. For example:

```yaml
diagrams:
  - kind: illustration
    id: fig-add-inputs
    image: assets/fig-add-inputs/figure.svg
    media-type: image/svg+xml
    caption: "add : Int → Int → Int"
    description: Three separate blue squares labelled Int, connected left to right by two arrows.
    width: 65mm
```

Tanglewood reads these persisted references directly and validates the actual image. When imported into a vault, place the referenced files at the corresponding vault-relative paths (`<vault>/assets/...` for this root note); opening it then renders automatically without Settings or Process. No sidecar or naming discovery is required for this file. An incomplete or invalid saved reference is an error, not permission to fall back. The earlier `source.md` is retained as the legacy compatibility/upstream proof fixture.

Writing generated references into Markdown in `illustration-md` belongs to that separate project. Tanglewood does not automatically rewrite source front matter during preview processing. Explicit processor output can override a saved reference for the processed view; otherwise the saved reference is used, and legacy sidecars are considered only when no reference was supplied.

## Derived processing representation

Authored `source.md` retains the original prompts and `diagrams` descriptions. Processing adds a separate derived map, shown in [`expected-derived-metadata.json`](expected-derived-metadata.json):

```yaml
illustrations:
  fig-add-inputs:
    image: assets/fig-add-inputs/figure.svg
    media-type: image/svg+xml
  fig-choosing-a-result:
    image: assets/fig-choosing-a-result/strip.png
    media-type: image/png
```

These are exact document-relative paths within the selected resource snapshot. They are not URLs or directory-discovery hints. For `chapter/source.md`, the first file is keyed `chapter/assets/fig-add-inputs/figure.svg` in the snapshot. A processor may instead return another safe relative location, such as `rendered/diagram.svg`; the renderer does not depend on `assets/<id>/` naming. The current named preparation subset supports SVG illustrations and PNG comics.

The local `prepare-stage.py` shim now adds this map to standard Pandoc `meta` using `MetaMap` and `MetaString`, after the pinned upstream processor produces its figures. Existing figure image targets and semantic content remain intact. `prove-linux.py` checks that both JSON and HTML processing publish the expected metadata, alongside the existing artwork/source checks. This shim enhancement is local, not an assertion that the pinned upstream implementation already emits the map.

After reading saved front-matter references (or its legacy inputs), native preparation publishes the same typed map in `PreparedIllustrations.metadata` and `ProcessedDocument.metadata`. Its legacy sidecar inputs remain compatible, but renderer resource resolution uses only the explicit output map. When Pandoc supplies `meta.illustrations`, the native validator uses those paths directly, without requiring sidecars. Missing entries/files, unsafe paths, unsupported or mismatched media types, and invalid image bytes fail explicitly; they do not fall back to conventional locations. Captions, descriptions and width retain their existing authored `diagrams` and prepared-figure representation.

`android-probe.rs` now checks the map on future runs. The checked-in `android-result.json` remains historical evidence from the original platform proof; it is not evidence for this change.
