# Pinned saved-artwork fixture

This local preparation fixture uses actual [illustration-md](https://github.com/dikini/illustration-md/tree/9f38098d2355d4d77e41a538b48354fa8145d92a) saved artwork, original metadata and prose briefs at that exact revision. It contains one SVG illustration and one PNG comic, with their exact sidecars. `provenance.json` records origin and byte hashes. Construction fields remain opaque; missing authoring inputs referenced there do not trigger generation.

Upstream does not declare a blanket project license. Its provenance notice and relevant outlined-font license records are retained in `upstream-notices/`. The comic's generated-artwork provenance remains in its sidecar. These files support the user's authorized local integration work; no external publication or redistribution grant is asserted.

Run the real Linux proof (requires installed Pandoc and Python, plus access to the pinned Git object):

```sh
python3 docs/fixtures/mvp-2/prove-linux.py --upstream /home/dikini/Projects/illustration-md
```

The script materializes an isolated source/assets snapshot and exact upstream preparation modules, runs the existing Lua callout before Python preparation, and invokes actual `json` and `html` writers. The shim consumes Pandoc's positional writer argument and passes only `--source SNAPSHOT` to upstream. Output is retained in a printed `/tmp/tanglewood-p0-filter-*` directory. JSON semantic figure output is compared across both targets; HTML embeds both assets. A missing-SVG rejection verifies the source stays unchanged. No Node, browser, font authoring tool or image generation runs. `expected-semantic.json` is a small observed upstream result, not hand-invented filter output.

`android-probe.rs` is the small actual-device harness used by P0. It depends on the repository library with `default-features = false, features = ["documents"]`, plus `serde_json` and `png`; it does not reimplement preparation. The temporary Cargo project and target binary remain at `/tmp/tanglewood-p0-android-probe/`. `android-result.json` is its actual API 35 x86_64 emulator result, including a real comic pixel decode and the same missing-SVG rejection. Exact build/run commands and limits are in `docs/plans/mvp-2-platform-feasibility.md`. This native runtime proof does not claim integrated APK rendering or arbitrary Android Pandoc/Lua support.
