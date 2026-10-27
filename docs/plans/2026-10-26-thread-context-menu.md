# Thread context menu

Goal: expose Rename, Archive, Remove through right-click and a keyboard-focusable
ellipsis button beside each Codex thread, without expanding the main toolbar.
Use the pinned CLI 0.163.0 thread/name/set, thread/archive and thread/delete APIs.
Keep project identity unchanged. Remove means permanent deletion, including
spawned descendants, with an explicit confirmation dialog. Archive also warns
about spawned descendants. No archived-thread browser in this small follow-up;
archived conversations remain recoverable through Codex.

Implementation (root, sequential shared checkout):
1. Add a focused protocol regression covering project allowlist, busy rejection,
   mutation responses/errors and keeping selected context until acknowledgement.
2. Extend bounded requests/events with correlated thread mutations. Do not start
   turns or silently retry unknown outcomes. Clear selected server context only
   on acknowledged archive/delete, update titles only on rename acknowledgement.
3. Add row context/ellipsis menus and small rename/confirmation modal. Disable
   destructive actions for active threads and unsent target drafts. Preserve
   other conversations and note source. Surface service errors unchanged.
4. Run focused tests and relevant integrated build once; inspect real native
   context menu and rename/archive/delete only isolated smoke-test conversations.
   No new model generation needed. Document results; no commit requested.

User steering: use Codex visuals and add model/reasoning selection. Extend the
same adapter with bounded paginated model/list capabilities; no hard-coded model
catalog. Show a compact composer popup with model submenu and supported-effort
slider/reset. Selection applies only on explicit Send via turn/start model/effort;
leave CLI/thread defaults intact until the user makes a choice. On thread resume,
show its returned effective model/effort. Never change global CLI configuration.
Use rounded composer/user message surfaces, restrained controls and no new tools,
attachments, voice or permissions features. Validate supported overrides in the
protocol fixture; inspect the real catalog/picker without a new model turn.

Delivered: context/ellipsis menus, acknowledgement-based title/list changes,
rename and explicit archive/delete confirmation; dynamic catalog/model submenu,
supported-effort slider/reset and rounded composer. External removal preserves
selected/cached/off-page draft recovery and clears retired turn/approval state.
Validation: combined 37 passed/1 optional ignored, post-review focused2passed,
final binaries/example build, real rename/archive/remove on owned disposable
conversation, real catalog/model/slider UI without additional generation.
Independent contract/recovery review READY. See docs/codex-live-evidence.md.
No commit requested or made.
