# Extraction and dependencies

The initial extraction starts from ash-doc commit
`e800faebc06fcafc0456f3e3f63f79d8b3f7a28d`, on 2026-10-25:
[source repository](https://github.com/dikini/ash-doc/tree/e800faebc06fcafc0456f3e3f63f79d8b3f7a28d).

Extracted and adapted:

- `rendering/build.py`: local sidecar resolution and outlined SVG validation.
- `rendering/illustrations/settings.mjs`, `size.mjs`, and their tests:
  appearance precedence and physical-size calculation.
- `sketch.mjs` and `split-sketch.mjs`: preset profiles and bounded shape/tree
  rough-stroke filters; local lesson selection and hard-coded paths removed.
- `fold-tree-labels.py`: simple SVG text outlining, extended to two declared
  bundled faces. Complex text layout and arbitrary font substitution remain
  outside its supported input.
- `rendering/diagrams.mjs`: Mermaid rendering, accessible SVG normalization,
  and explicit PDF-to-SVG font outlining. Other diagram engines and book
  orchestration remain outside this extraction.
- The illustration authoring skill and worked trials: adapted into portable
  process guidance and three self-contained examples, without Ash semantics,
  translation administration, book styles, or lesson-local adapters.

The initial API matches metadata by stable ID, rather than ash-doc's positional
diagram order. It emits native Pandoc figures so ordinary HTML writers display
captions separately from alternative descriptions without Quarto conventions.
It retains relative asset references rather than staging them
into a book project. The new tree adapter explicitly paints edges first,
as ash-doc's migrated lesson adapter does; the pinned split-md serializer
does not reorder its scene automatically. No upstream source is modified.

## Dependency records

`package-lock.json` owns exact npm dependency versions and integrity data;
`requirements-authoring.txt` pins Python font dependencies. `toolchain.json`
retains the browser, Poppler, split-md source hashes, Rough.js bundle hash,
and font hashes. Rebuild sidecars record the resolved toolchain and final
SVG hash. The regular font is IBM Plex Mono, not a reader-installed fallback.

- [Rough.js](https://github.com/rough-stuff/rough), version 4.6.6.
- [Mermaid](https://github.com/mermaid-js/mermaid), version 12.1.0;
  [Mermaid CLI](https://github.com/mermaid-js/mermaid-cli), version 12.0.0.
- [Puppeteer](https://github.com/puppeteer/puppeteer), version 25.10.0.
- [FontTools](https://github.com/fonttools/fonttools), version 4.61.1,
  and [Brotli](https://github.com/google/brotli), version 1.1.0.
- [split-md](https://github.com/dikini/split-md), exact commit in the toolchain
  record; externally supplied, not vendored or relicensed.
- Excalifont Latin WOFF2, retained from ash-doc with its original
  [font license](../fonts/Excalifont-LICENSE.txt) and hash. The source record
  names Excalidraw commit `b7de45e88111f5680ae4bb3c3f3541c7d3b279fe`.
- IBM Plex Mono regular TTF, retained with its original
  [license/attribution record](../fonts/IBM-Plex-license.txt) and hash.

Third-party dependencies and fonts retain their respective licenses. This
extraction does not select a blanket project license or authorize package
publication. Establish redistribution terms before publishing a release.
