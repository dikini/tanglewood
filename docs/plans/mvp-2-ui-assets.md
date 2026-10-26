# MVP-2 U0 UI assets

Frozen resource preparation, 2026-10-26. These are trusted application resources, not document artwork. No production Rust, dependency, Android or input changes are included.

## Provenance and licenses

- **DejaVu 2.37**: unmodified `DejaVuSans.ttf`, `DejaVuSerif.ttf`, `DejaVuSansMono.ttf` copied from the R0-verified local cache `/usr/share/fonts/truetype/dejavu/`. Their hashes match the R0 inventory. Font version reported by `fc-scan` is 155320 (16.16 ≈2.37). Official [2.37 source tag](https://github.com/dejavu-fonts/dejavu-fonts/tree/version_2_37), [downloads](https://dejavu-fonts.github.io/Download.html). Complete upstream `LICENSE` and `AUTHORS` fetched from that tag and preserved as `LICENSE.txt`/`AUTHORS.txt` beside the fonts. The license includes **Bitstream Vera and Arev/Tavmjong Bah notices**; DejaVu changes are public domain. This is not OFL. No font subsetting, renaming or modification was performed.
- **Lucide 0.575.0**: 22 unmodified SVGs fetched from `https://raw.githubusercontent.com/lucide-icons/lucide/0.575.0/icons/<name>.svg`. Official [tagged icon source](https://github.com/lucide-icons/lucide/tree/0.575.0/icons). Complete [tagged LICENSE](https://github.com/lucide-icons/lucide/blob/0.575.0/LICENSE) preserved as `LICENSE.txt`: ISC plus full MIT/Feather attribution. Keep both notices when bundling.

R0's proposed `more-horizontal` is an alias, not a file in this tag (HTTP 404); the exact upstream file is **`ellipsis.svg`**. Added `eye.svg` and `code-xml.svg` to make preview/source controls explicit; total remains 22, not the whole library.

## Intended use and integration contract

`file-text`/`folder-open`: documents/Notes; `search`: Search; `link`: Links; `sparkles`: AI; `panel-left`/`panel-right`: sidebars; `columns-2`/`rows-2`: splits; `plus`/`x`: add/close tab and Cancel; `check`: Ready; `undo-2`/`redo-2`: source history; chevrons: disclosure; `ellipsis`: secondary menu; `settings`: preferences; `refresh-cw`: retry; `circle-alert`: error; `eye`/`code-xml`: preview/source. Use visible text for Ready/Cancel and actionable errors.

Later UI code can expose a small concrete icon enum mapped to `include_bytes!` data; render owned trusted bytes at a stable resource identity. These SVGs use `currentColor`, 24×24 viewBox, stroke width 2. Resolve color explicitly for resvg (for example controlled `currentColor` substitution in this trusted static subset), cache by icon/size/color, and bound raster size. Do not mutate vendored originals or use a new unique URI every frame. No HTTP/file URI loader or document-provided SVG is authorized by this helper. Keep action labels/WidgetInfo, tooltips, focus state and phone hit targets separate from decorative pixels.

Register the three font files once using `FontDefinitions`; Sans for shell/Notes, Serif for Academic/Tufte prose, Mono for source/code. Keep existing emoji/default fallback families. Font atlas cost is additional to the raw 1,483,520 font bytes; these files do not prove small process memory or identical screen/HTML output. True bold/italic faces are not in this initial selection.

## Executed inspection and limits

`fc-scan` and `fc-query` on the **vendored** TTFs verified version and cmap presence of every character in `Привет Ελληνικά ∑∫√∞≤≥≈ℝαβγ→↔`. This checks representative Cyrillic, Greek and scientific characters, not shaping, all mathematical symbols or visual rendering. SVG XML parsing verified all 22 files are valid XML with the 24×24 viewBox and no href resources. SHA-256 and byte counts below were computed from the vendored files. No font/UI build, raster screenshot, Android/ADB action, Cargo job or new input acceptance was run for this resource-only task.

## Exact inventory

Paths are relative to `assets/ui/`. Hashes include the complete license/author notices, making the delivered set reproducible from the listed pins and local font cache.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `fonts/dejavu-2.37/AUTHORS.txt` | 842 | `915a742f0b242ea43fcbef934b6d24353b51351470f71d8a8d00c751892fad50` |
| `fonts/dejavu-2.37/DejaVuSans.ttf` | 759720 | `b4c632e3cdf9acc7f28758fb5a323c8524d7fc6660d46904d9b6cbe2809c419c` |
| `fonts/dejavu-2.37/DejaVuSansMono.ttf` | 343140 | `a54dca07c76d6289e717e75e0a58c0128f6d7269ef3faf76417c9d7d3bba37ab` |
| `fonts/dejavu-2.37/DejaVuSerif.ttf` | 380660 | `a52c2a69dfc93d0b511b98c1019a57eb3d6a62d328bdf1387f00851b02e3a315` |
| `fonts/dejavu-2.37/LICENSE.txt` | 8816 | `7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93` |
| `icons/lucide-0.575.0/LICENSE.txt` | 2004 | `668dcc52803480e0a026b31140a4cae668772663cd764e5991d252eef03f98db` |
| `icons/lucide-0.575.0/check.svg` | 239 | `7f33acc9a77a61659531044525fc008edebe215bf4dcf1c789c8674ad3277db0` |
| `icons/lucide-0.575.0/chevron-down.svg` | 236 | `66ea878e72ed3488bb3b464c39dfdccee8d1f78e560dccea40e5e12da0e87e87` |
| `icons/lucide-0.575.0/chevron-right.svg` | 237 | `2758143d7b2434e4aa7307dfd34405c87909ff4052f21b5f3f40d45224b4f19b` |
| `icons/lucide-0.575.0/circle-alert.svg` | 332 | `ce3e98b7a03b78c0071131df42466e4e1ff5187fccad30e739ec9ce951cf685a` |
| `icons/lucide-0.575.0/code-xml.svg` | 294 | `a6d2110227be375be21da7324d3c09262c1bf019d06bdeab356d515342ab7cce` |
| `icons/lucide-0.575.0/columns-2.svg` | 285 | `bddf3348a9c82d111a060d118299c90755b1ae8411bd16ea1d5ddb83c5f98a27` |
| `icons/lucide-0.575.0/ellipsis.svg` | 312 | `4f495cc72013ffdfec677f03b33a150f7b4dd741979283fd6853a09024bca112` |
| `icons/lucide-0.575.0/eye.svg` | 360 | `5bf90197dd7629cad64a2e48d1186a71559deb6121207d10e3dc5b19ebaffdcf` |
| `icons/lucide-0.575.0/file-text.svg` | 444 | `52ca55846c202335fb870c8aa23e9345630ec10489303756d62e750f7109605c` |
| `icons/lucide-0.575.0/folder-open.svg` | 401 | `ee57452b40e71df310214091dd9da1941a397c83197b8ff8d7ae66dd4242578b` |
| `icons/lucide-0.575.0/link.svg` | 359 | `4f3237183dcaabfac92f4e94c776a9a9e45c3c5e77a921061a6b34b61eb5413f` |
| `icons/lucide-0.575.0/panel-left.svg` | 284 | `029f67b9e3eeb7ddf7c7c499b174c50388725e8ea50d4ea59ed7dbaf8d61cb82` |
| `icons/lucide-0.575.0/panel-right.svg` | 285 | `327d197328f9feb1f88c36c9db04e9b00fe717d877459f92fcd892f5e4c349fa` |
| `icons/lucide-0.575.0/plus.svg` | 256 | `7f6af73bf1ff6c4bca3f18351c8d1bdec6749c0c2530c4de5da85d520c21df17` |
| `icons/lucide-0.575.0/redo-2.svg` | 308 | `4cbd2cdebc70b6b7979c681ed3f313d9cbb102465dcf7d0d2d6a72c937b344bc` |
| `icons/lucide-0.575.0/refresh-cw.svg` | 392 | `2e10dd403c85a24f163d59fc6151aa21147fe9402e1305dfc8979208caee8944` |
| `icons/lucide-0.575.0/rows-2.svg` | 285 | `92f8d9962230c2d2a93ff1d9f40cde8d969b3e4f9890fabf741593a8c83835f4` |
| `icons/lucide-0.575.0/search.svg` | 275 | `283d371c2e433817bb9c0c8310caa6c77fa4177c0f4f1168d9c83b97af7389dc` |
| `icons/lucide-0.575.0/settings.svg` | 586 | `0ae27fd0f81999229e3127ac96c5b32edfea448e291d509e76212b917551d66b` |
| `icons/lucide-0.575.0/sparkles.svg` | 567 | `f5499f33f09d7158151e9bd2ec0faf79ff8fb57292f84fdd7286d96d0f0424d8` |
| `icons/lucide-0.575.0/undo-2.svg` | 309 | `efad2edbbd2be6038cc23fd64df2d012fad5cc6460dfbf227cfa3d990e9e897f` |
| `icons/lucide-0.575.0/x.svg` | 260 | `4a9cdab38fbb96162e7dace28e33f4ca0e49d8963a6162abc3d4691b7d675117` |
