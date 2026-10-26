# MVP-2 execution status

Implementation authorized 2026-10-26 by the active goal: implement the updated [proposal plan](plans/2026-10-26-mvp-2-proposal.md). Baseline: `8aaa704` (unsigned), original MVP completed. Preserve `mvp-status.md` as that delivery record.

## Scope correction — 2026-10-26

User requested ruthless MVP scope reduction. The proposal's **Controlling MVP scope** now overrides the broad phase table/audit below. Those rows are retained as backlog, not current release gates. Current delivery is a minimal one-document egui shell and source-backed Ready/Cancel loop, syntax highlighting, real Linux filters/Android named subset, local illustration/comic resources and the smallest native presentation needed. Tabs/splits, extra publishing profiles/export, new recovery storage, bundle persistence and asset synchronization are deferred.

The reduced slice is implemented and accepted. Linux demonstrated real filters, SVG/comic, fence-only Ready, Cancel, processing failure and corrected-build restart. Android demonstrated actual APK artwork plus final Ready/Cancel, Saved and force-stop/relaunch with exact source bytes. The earlier 5–10 engineering-day range was a planning estimate, not elapsed agent-runtime; it is not a reason to add deferred scope.

## Current task board

| Task | Owner | State | Evidence / next dependency |
| --- | --- | --- | --- |
| Widget/icon/font selection | Platform | done | Bounded R0 source research; vendored assets and notices |
| Required preparation feasibility | Core + reviewer | done | Actual Linux Pandoc/Lua/Python fixture, host Rust parity, Android native execution; independent review READY |
| Existing UI extraction | PM | done | `RUSTC_WRAPPER= cargo check --locked --lib --features gui,documents` PASS before E1 changes |
| Minimal shell and block/source Ready/Cancel | Platform | done | Exact-source/controller and pending-input regressions passed; independent review READY after bounded handoff fixes |
| Bounded processing worker | Backend | done | Actual ordered Pandoc/Lua/Python, cancellation/timeout/group cleanup and ordinary-note metadata regression passed; reviewed READY |
| Native processed preview and artwork | PM | done | Actual native SVG/comic and filtered Linux preview; coherent applied-settings regression; reviewed READY |
| Integrated Linux/Android acceptance | PM + independent reviewer | done | Linux exact source loop/failure/restart; final Android APK Ready/Cancel/Saved/restart passed with original notes preserved; all blocking reviews READY |

Only assigned paths have active writers; PM owns manifest/lock/lib, composition integration after editor handoff, and index. One Cargo job at a time. Source is frozen after acceptance. Integration uses an unsigned local checkpoint; no push/publication.

## Current completion audit

- [x] Bounded component/icon/font research and selected assets/licenses.
- [x] Real required preparation feasibility on Linux and Android; named compatibility boundaries.
- [x] Quiet one-document shell and resizable navigation.
- [x] Exact original block/whole source Ready/Cancel, cached highlighting, stale/composing draft safety.
- [x] Actual bounded async Linux filter runner and Android local required subset integrated.
- [x] Native notes and actual SVG/PNG/caption presentation from immutable local resources.
- [x] Missing-artwork/processing failure preserves source and clearly marks stale preview.
- [x] Real Linux and integrated Android workflow, focused checks and resolved review findings.
- [x] Accurate run/configuration/support/evidence documentation; unsigned checkpoint recorded in Git.

The previous asset bundle/transfer and publishing/multi-view phase gates are deferred by the user's scope correction. Existing note persistence/sync remain supported; artwork is local only in this increment. Browser, framework migration and custom text/IME engines remain outside scope. Do not claim integrated APK completion from the P0 native probe.

## Combined validation checkpoint

- `cargo fmt --check`: PASS.
- `RUSTC_WRAPPER= cargo clippy --locked --all-targets --features gui,lan,codex`: PASS with nonblocking style/dead-code warnings; no automated fix sweep.
- `RUSTC_WRAPPER= cargo test --locked --lib --features gui ui::`: PASS, 3 focused source/coordinator regressions.
- `RUSTC_WRAPPER= cargo check --locked --lib --no-default-features`: PASS; graphics remain optional.
- Actual backend, input and platform evidence are linked from `mvp-2.md`, `mvp-2-linux-evidence.md` and `mvp-2-android-evidence.md`. Do not rerun unchanged domain suites.

## Delivery

Run instructions and limits: `mvp-2.md`. Final Android APK: `android/app/build/outputs/apk/debug/app-debug.apk`, SHA-256 `b8ac724774377fd7c6ccd530cdf2e8ae8b098e3be767a582a096102010b23724`. Final Android accepted fixture: 1742 bytes, SHA-256 `2365c848ae53e7709b9f80fcda7418684fad36113068c467b7905f20f16c87de`, unchanged across force-stop/relaunch; existing notes preserved. Platform details are in the two MVP-2 evidence records.

Completed against the user-reduced controlling scope, not the deferred multi-view/publishing/asset-sync backlog. No claim of arbitrary Android Pandoc filters, physical-device coverage or crash-durable unsubmitted drafts.

## Approved quiet-shell follow-up

The user rejected the original exposed-control layout and approved the Codex-inspired sketch. Subsequent UI implementation and verification are tracked separately in [quiet shell evidence](quiet-shell-evidence.md); the original delivery evidence above remains historical. Future GUI changes follow the preserved reference and project instructions.
