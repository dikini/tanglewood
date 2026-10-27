# Unified model picker and conversation state implementation plan

> **Execution:** Root coordinates this authorized follow-up in `codex/model-picker`. GPT-6.1 Sol/xhigh designs and independently reviews; GPT-6.1 Terra/max implements. Freeze this contract before implementation. Do not start the separate MVP goal.

**Goal:** Choose any enabled provider's model directly from the composer, with automatic preparation, valid adjacent reasoning controls and preserved independent conversations, including work continuing in the background.

**Architecture:** Tanglewood is a client/coordinator for the user and Codex/Kimi agents. It manages resources and conversations and routes explicit agent commands; possible future orchestration remains outside the visual shell. The visual shell queries partial observations and local drafts stored as conversation entities in the existing App ECS World; the agents own their actual execution and remote state. Add a small concrete observation/client-connection owner around existing adapters and event reducers; visible selection never owns a connection lifetime. A separately bounded metadata coordinator discovers all enabled catalogs without loading a person's history or disturbing a conversation.

**Tech stack:** Existing Rust package, egui, Serde, bounded worker channels and process cleanup. No new dependency, vault schema, provider framework, scheduler, authentication flow or configuration registry.

## Basis and scope

Read with `AGENTS.md`, `docs/plan/tanglewood-mvp.md`, `docs/design-system/README.md`, `docs/plans/2026-10-26-approved-egui-shell.md`, `docs/design/tanglewood-quiet-desktop.html` and `docs/plans/2026-10-27-kimi-acp.md`. Apply the installed taste skill's **app-screens** guidance: preserve the product shell, put the common job first, use one flat list, localize errors and keep decoration restrained.

The supplied native screenshot exposes three avoidable steps: choosing a provider, connecting/opening its conversation separately, then entering a nested model submenu. The normal path becomes open the model picker, choose a model, write, Send. Switching an existing conversation to another provider instead offers an explicit new conversation, preserving the original.

**Approved governing rule:** switching conversations, tasks, Research, Notes or another shell route never cancels, disconnects, resumes or restarts work merely because the view changed. Hidden conversations continue receiving available updates. Tanglewood observes agents; it does not own all their state merely because a conversation is visible or remembered. This supersedes the earlier Kimi plan's one-foreground-child and disconnect-on-backend-switch UI assumption. MCP still uses one application/vault owner.

| Responsibility | Authoritative owner |
| --- | --- |
| Notes, research resources, revisions, persistence and vault authorization | Existing Tanglewood App/ECS owner |
| Agent work, remote session/history, native configuration and execution lifecycle | Codex or Kimi agent, as supported by its real protocol |
| Conversation observations, local drafts, confirmed config, permissions and pending client intent | Conversation entity components in the existing App World |
| Client handles, event routing, explicit agent commands and future orchestration | Existing App coordination layer; transport handles keyed by ChatId |
| Visible selection, layout, scroll, focus and temporary controls | Visual shell/view state |

Tanglewood can serve as a user-controlled memory device: the person chooses which notes, research, decisions and results to retain. Remembered conversation observations carry their provenance, freshness and known gaps; they do not imply a complete copy of provider memory or hidden agent state. This is architectural context for existing resources and observation records, not a new schema, persistence engine or memory-feature delivery gate.

Agent activity and client observation are independent facts. A record tracks last observed remote activity separately from attachment/freshness (**attached**, **stale**, **disconnected**, **never observed**). Hidden, disconnected or unknown does not mean idle/stopped. Show last-known activity with freshness when appropriate. A terminal acknowledgement proves only the correlated operation's reported outcome; a dropped transport does not prove the agent stopped.

Screen inventory:

- A person repeatedly writes in Tanglewood's Linux conversation/research surface.
- The central job is choosing the next message's model without managing provider lifecycles.
- Objects are enabled configurations, catalogs, models, reasoning choices, conversation records, optional client handles and drafts.
- Existing `--codex` / `--kimi` flags define enabled configurations and canonical context directories.
- Actual installed providers advertise model names/counts; observed snapshots never become fixed production catalogs.
- Catalog pagination retains current adapter bounds; search works across model/provider names.
- Remote identity includes backend, canonical workspace and session; connection generation scopes events.
- Transcript and composer remain the largest content; model and Connections controls are temporary surfaces.

This slice supports the existing enabled Codex/Kimi configurations unchanged. More provider types/configuration editing are outside scope. Catalog UI and job queuing iterate enabled entries rather than adding a prerequisite provider screen. Multiple conversations of the **same** provider are supported by independent concrete client/adapter handles; there is no new multiplexed transport or agent scheduler.

## Options

| Approach | Benefit | Cost / decision |
| --- | --- | --- |
| **Recommended: application-owned conversation collection plus temporary catalog helpers** | Selection changes projection only; independent work continues, all enabled catalogs appear automatically, and existing protocols remain concrete. | Extract existing runtime/reducer ownership from the visual panel; add narrow metadata startup behavior. |
| Unified picker over the current parked single-provider panel | Smallest visual patch. | Current backend switching disconnects work; it cannot satisfy background observation or same-provider concurrency. Reject. |
| Generic provider/session service with permanent multiplexed connections | Broad future extensibility. | Introduces transport/framework work beyond the user problem. Independent current adapters and a concrete owning collection are sufficient. Reject. |

## UX contract

The picker answers **“Which model will answer my next message?”**

1. Remove the top-level backend prerequisite picker. The composer has a frameless **Model** control, labelled with its actual selection, and an adjacent **Reasoning** control when the selected model offers choices. Native mode/access controls stay separate and provider-specific.
2. The model popover opens directly to a labelled search field and one scrollable list grouped by quiet **Codex** / **Kimi** headings. Rows have names, optional description tooltips and a check for the actual selection. Full provider/model identity determines the check. No nested model submenu or reasoning slider inside this list.
3. Every enabled provider heading appears immediately, with **Loading models…**, **Unavailable · Retry**, or actual rows. One failed provider leaves other rows usable. Footer: **Connections…**. Search matches provider/model names and wire IDs; matching a provider name includes its rows. Escape restores focus to Model.
4. First opening the conversation surface/picker queues discovery for **all** enabled configurations once, without per-provider connect actions. Last-good rows stay visible during refresh. No automatic retry loop after failure. Preparation already started continues if the view is hidden.
5. Reasoning is a separate compact menu containing only the selected model's valid advertised values, descriptions and current check. Codex uses its per-model effort list/default. Kimi uses its latest confirmed whole session config snapshot. Hide when absent; disable while model/config preparation is pending. Unknown reasoning for another Kimi model is not an advertised empty list.
6. A fresh conversation, including one with an unsent draft, accepts model selection directly. Missing connection/session preparation happens automatically; draft editing remains available while safe. Send is disabled with a local **Preparing {provider}…** explanation until identity/model are confirmed. Selecting a model never emits a prompt.
7. In a conversation with sent/loaded history, selecting another provider opens **New conversation with Kimi** (or Codex). Copy: **“Your draft and attached snapshots will be copied. Earlier messages stay in this conversation.”** The explicit New conversation action and Cancel are the only actions. The original chat, running work, history and draft remain. The target is a genuinely new record/session; no transcript migration, summary or automatic send.
8. Same-provider model changes affect only the selected conversation's next message. A busy conversation cannot mutate its own model/config until its pending turn/permission/open settles, but the person can navigate to other conversations or create another, including choosing a cross-provider new conversation without touching the busy source. A pending operation in one chat never globally disables other chats.
9. Conversation rows expose quiet **Working / Needs approval / Error** indicators, with accessible text and tooltips. A waiting permission belongs to that chat; navigating away preserves it. Selecting the indicator opens the right chat. Stop and permission answers target the selected chat's operation explicitly. Hidden errors do not replace another chat's transcript/error.

Connections is one scrollable settings/dialog surface reachable from the picker and existing connection affordance. Show catalog **Not loaded / Loading / Available / Unavailable** separately from actual runtime **Connected / Working / Needs approval** counts. Provide default-on automatic discovery, **Refresh model lists** / **Retry unavailable** across enabled entries, and **Stop discovery**. Per-provider Retry is optional convenience, never setup. “Available” does not imply a persistent agent connection; do not label a refresh “Reconnect all”.

Explicit **Disconnect conversation** targets a named client handle; **Disconnect all conversations** names the affected count. If any target is working or awaiting permission, explain the actual transport consequence before the explicit disconnect action: observation ends and an owned stdio process may be interrupted, while remote continuation is protocol-dependent. Do not claim remote Stop without acknowledgement. Ordinary navigation has no such action. Stopping discovery affects helpers only. Paused/cancelled discovery is not restarted by the next frame. No credential editor, per-provider setup dashboard or permanent transcript status rows are added.

Preserve semantic tokens, keyboard focus, accessible names, composer wrapping and compact sheet behavior. Model/reasoning/Send remain reachable at narrow widths; no horizontal page/toolbar scrolling. Use quiet selected rows and one local status cue, not stacked cards/badges.

## Application owner and identity contract

Use `src/conversation_runtime.rs` for plain conversation components/reducers and the concrete client-handle owner. **Conversation records are entities in the existing `App.world`.** One plain `ConversationRecord` component per chat is sufficient for this slice; split components only for a demonstrated need. Do not introduce entity-per-message/token, another World, a parallel authoritative record HashMap, a generic repository or a persistence migration. Existing extracted plain record/reducer code is the integration seam and can be attached without discarding that work.

```text
ChatId = stable local UUID, allocated before any remote session exists
ConversationIndex = ChatId -> existing-World Entity (lookup only)
RemoteIndex = ConversationKey -> ChatId (lookup only)
ConversationRecord component = { ChatId, ProviderKey, remote_key?, draft + draft_revision,
    transcript, confirmed_model/options, last_observed_activity, observation_freshness,
    errors, context, permissions, pending history/config/open/submission + ModelIntent }
ProviderKey = { backend, canonical_workspace }
remote_key = existing ConversationKey { backend, workspace, session }
App client handles = ChatId -> { concrete adapter, connection_generation,
                               transport routing/cleanup state }
ConversationAction = targeted { ChatId, expected_generation/revision?, action }
ConversationView = { selected ChatId, per-chat layout/scroll/focus/render cache,
                     temporary popover/search/dialog/preview presentation }
```

The existing App is the only mutable owner. A record exists independently of whether Tanglewood created its remote session or has a client handle. ChatId owns the unbound draft before a remote identity is acknowledged. Indices contain identity/entity lookups, never copies of authoritative record fields. Same-string remote IDs in different backends/workspaces remain distinct; resolve an existing remote key to its entity instead of spawning another client. Entity handles stay internal; commands/events use ChatId and validated remote/generation identity.

The App-owned handle map contains optional transport objects, not a second conversation-state collection. Adapters keep their existing worker-local protocol bookkeeping, but the application's observed state, draft, permission UI data and pending client intent live in ECS. Every new/replaced adapter receives a fresh generation. `App::tick` drains bounded events from handles, then resolves ChatId through the index and reduces each event into the corresponding component after checking generation/session/turn/operation identity. Late events cannot mutate another entity or clear a newer draft. No I/O worker or renderer acquires mutable World access; no `Arc<Mutex<World>>`, second owner or new daemon.

Move client lifecycle/reduction behavior from `ConversationPanel` / `KimiState` to App-owned coordination, preserving concrete provider branches. Replace `Parked` swapping with stable entities. Keep egui focus, scroll, layout, temporary dialogs and disposable render/preview caches in view state. Persisted or unsubmitted draft text, attachments/skills, config, transcript/activity, errors, permissions and pending operations must not live in UI objects. An egui text widget may edit a frame-local projection buffer and emit `SetDraft { ChatId, expected_draft_revision, text }`; that scratch buffer is not an independent retained draft.

**Rendering boundary:** expose narrow immutable queries/projections such as `App::conversation_projection(ChatId)` and targeted `App::execute_conversation(ConversationAction)`. Borrow only the needed record read-only, or create a bounded display snapshot; collect actions while rendering and apply them through App after the render borrow ends. Snapshots are disposable projections, never a second authoritative collection. Use split field borrows or drain transport events before World mutation in the owner. Do not move/take/replace the ECS World or temporarily transfer whole record ownership into the shell to evade borrowing; do not use a take-render-restore manager helper. Hiding/dropping a widget cannot drop a client or record. This application coordination boundary can support future explicit orchestration; a scheduler/new task system is not a current gate.

`App::tick` drains all attached clients every frame, even when Notes/Research/another chat is visible. Bound work globally (for example 64 events total per tick) and use round-robin fairness so a noisy chat cannot starve another's permission/terminal event. Repaint wakeups schedule the owner, independent of visibility. Preserve existing per-adapter byte/request/approval limits; impose a small explicit live-handle budget (initially 8) and return actionable Busy at capacity. This bounds local client handles, not the number of agents allowed to work remotely. Never evict/cancel another chat to make room. Catalog helpers have their own limit and do not consume or replace a live chat handle.

Opening an already-attached record changes selection only. Opening a finished remote record may attach a new adapter/read history through supported list/load/resume after membership validation; it never submits a prompt. Closing/hiding a visual pane is selection/layout. Explicit Stop sends the supported targeted stop/cancel command and waits for its correlated result. Explicit client disconnect/application shutdown clean up only owned client processes/handles off rendering paths; record observation loss and any transport-specific consequence without inventing a remote terminal state. Preserve records/drafts on disconnect.

### External-origin conversation boundary

Future work started in Codex desktop can appear in Tanglewood when the provider's real list/history APIs expose the same authorized workspace/project session. Origin is not inferred from local creation, and an optional local client handle does not describe all provider activity. Reuse current confirmed list/load/history paths for discoverable completed external sessions and their available results; do not create a replacement session or restart work merely to inspect it.

For an externally working session, display the current list/status snapshot and its freshness. Attach/load for observation only where the installed protocol supports it without taking over execution; otherwise retain a status-only observation and say that live updates/history are unavailable on this connection. Do not claim live external streaming merely because a local child exists, take over competing execution, automatically send/resume work, or fabricate inaccessible artifacts. A remembered conversation can exist with no local process at all. This slice reserves record-versus-execution identity and reuses existing capabilities; it does not add cross-app live observation, broaden current project/membership authorization, scan arbitrary files, or implement a provider service. Report current discovery/history limitations explicitly.

## Catalog protocol contract

Add `src/model_catalog.rs` for the application-owned concrete discovery coordinator, with `src/ui/model_picker.rs` for its egui projection. Existing enabled configs populate entries; no new registry. Each entry holds ProviderKey, a generation, one last-good in-memory snapshot, phase, error and separately tracked cleanup issue. Phases: Idle, Queued, Loading, CleaningUp, Available, Failed, Paused. No disk cache.

At most **one metadata helper** runs at a time, independent of live conversations. Deduplicate queued work by ProviderKey; initial discovery queues every enabled entry. Each helper owns its adapter, cancellation and teardown on a worker thread. UI/owner receives bounded completion records and does not join a child during rendering. Match ProviderKey and catalog generation before applying results; helper events can never update a ConversationRecord. Follow existing timeouts and continue the queue after a failed/expired job. No retries or paid prompts are automatic.

**Codex:** Add `CodexAdapter::spawn_catalog` with a private startup-purpose enum. After initialize/initialized request `model/list` directly. Suppress project/read, project/create, thread/start and thread/resume even if config has project/resume fields. Restrict catalog mode to model pagination/disconnect; reject conversation operations. Automatically follow bounded pagination, retaining the existing 128-model limit and rejecting cursor cycles; tear down when complete. Normal spawn behavior remains unchanged. Merely passing `project: None` is insufficient because current initialization starts a thread in that branch.

**Kimi:** Reuse KimiAdapter with `resume_session: None`: initialize, create one owned empty session, capture its actual whole `configOptions`, close/delete **only that helper-created session**, then tear down. Never load a historical session, send a prompt, enumerate by mutating every model, or change credentials/global configuration. Preserve native category/group/value IDs. Model category is `model`, reasoning placement `thought_level`; dispatch always uses the actual option ID. Other/unknown categories stay usable in native controls instead of guessing that they are models.

Use existing `DeleteCreatedSession` ownership validation. Cancellation/failure after creation must attempt the same bounded cleanup. Retain a valid returned created ID even if subsequent config parsing fails. Cleanup success must be acknowledged. On failure keep rows separately from the cleanup issue, label **Models loaded; temporary-session cleanup failed** in Connections, and retain bounded owned-ID diagnostics. An explicit bounded retry is allowed only while the original helper connection and its connection-created ownership set remain alive. After teardown, keep diagnostics/manual safe-cleanup guidance; do not reconnect and bypass `DeleteCreatedSession` ownership, promote a remembered ID into a new created set, or delete an arbitrary listed session. Any manual cleanup remains an explicit provider-management action after verifying the helper-created identity. Never claim deletion without acknowledgement. Other providers continue after the deadline.

## Model intent and draft contracts

An ECS-record-owned targeted ModelIntent contains operation ID, ChatId, ModelKey, current client generation and purpose **InPlace / Fresh / NewWithTransfer**. The selected view is not its owner. A fresh model choice prepares that record automatically, respects Codex durable project binding, creates a remote conversation when absent, then confirms the chosen model. A normal explicit reconnect of an existing record retains its validated remote identity.

A cross-provider choice creates a new target ChatId without changing the source record/client. Do not reuse or overwrite the destination provider's last record/draft. Its new-session operation suppresses any configured target resume ID. The source may remain streaming while the target is prepared. On failure preserve source, target draft and pending intent; Retry preparation never submits. Return to original changes selection only.

For Codex, await new/resumed thread model metadata before applying the pending next-turn model/valid effort override; a late ThreadModel event cannot erase the intent. For Kimi, await the target session config, validate the requested model is still offered, send its actual SetOption ID/value, await the matching **settled** response, replace the whole snapshot, then expose reasoning. Notifications update confirmed data but do not settle a pending set. Serialize changes **per chat**, not across chats. Never send an old model's reasoning value to the new model.

If a chosen model disappeared, keep the draft/intended selection and show an actionable error; disable that chat's Send until another model is chosen. Never silently fall back and send with a different model. Pending rows show preparing, not a confirmed check.

Cross-provider transfer copies exact prompt bytes and explicit attachment names/text snapshots. Do not trim, reread source files, summarize history or regenerate evidence. Source draft remains intact; a target's unrelated existing draft is a different record and remains intact. Keep an unbound draft under ChatId, never a global empty-session-string key. Install only the intended target's snapshot; model/session failure cannot lose it, and no success path sends automatically.

Codex skills, goal, access/approval/network choices, Kimi mode, plans, tool output, automatic relevant-evidence snapshots, permissions and provider metadata do not transfer. Model selection dispatches no permission/mode change and changes no owner vault policy. Whole ACP snapshots are authoritative; an unexpected permission-mode change during model mutation blocks that chat's Send for explicit review. Keep native controls and the Kimi ordinary-process-privilege explanation in Connections/help; no Codex access mapping onto Kimi.

MCP routing/policy/lifetime stays unchanged. Every live chat/helper bridge is an independent client of the same owner. Stopping/disconnecting one chat or helper closes only its own clients and cannot cancel another's pending reply or mutate another's permissions. Reuse current revision/revocation validation; no new vault task system is introduced.

## Implementation sequence and ownership

Root assigns **one Terra developer**, sequential writes across the following files. Root owns integration/index and `docs/mvp-status.md`; this design author owns only this new plan until freeze. No concurrent writer shares `src/ui.rs`.

1. **Ownership extraction:** create `src/conversation_runtime.rs`; modify `src/conversation.rs`, `src/app.rs`, `src/lib.rs`, `src/ui.rs`, `src/ui/kimi_panel.rs`, `src/ui/codex_context.rs` as needed. Attach extracted plain records as components in the existing App World; add ChatId/entity and remote identity indices plus App-owned optional transport handles. Expose immutable query projections and targeted commands; no take-render-restore helper, parallel authoritative record collection or second World. Reuse adapter reducers/native controls and add owner-wide fair polling; do not rewrite protocols. First extend existing provider/draft scenarios to reproduce navigation with two attached working chats, including same-provider chats, and assert no disconnect/interrupt/prompt from selection.
2. **Metadata:** create `src/model_catalog.rs`; modify `src/codex.rs` for restricted catalog startup. Extend its protocol scenario to prove no project/thread/prompt. Add one fake-process Kimi helper scenario for owned new/config/cleanup, cleanup failure and cancellation; modify `src/kimi.rs` only for concrete cleanup holes such as retaining created ID before option parse. No historical load/prompt in helpers.
3. **Intent and projection:** create `src/ui/model_picker.rs`; wire both composer branches to the shared list and adjacent reasoning. Add automatic record preparation and new-record transfer. Remove backend prerequisite and duplicate model controls; preserve unrelated native ACP options. Centralize Connections lifecycle controls and add per-chat background/approval indicators. Extend focused owner scenario for stale-event isolation, exact transfer, target draft preservation and local failure/retry.
4. **Integration evidence:** root runs combined checks once, inspects the real native window and freezes changes for the same independent Sol/xhigh reviewer. Terra fixes concrete findings and receives delta review; at most four cycles. Root records evidence/limits, and alone stages/commits if authorized using unsigned commits.

`src/main.rs`, Cargo files, store, domain note/research commands and MCP implementation are expected unchanged. An evidenced need to touch them goes through root. Existing project persistence still uses the application command path; conversation ownership must not create a second store/ECS writer.

## Validation and stopping rule

This document needs no Cargo run. During implementation run the smallest affected owner/catalog/adapter scenario, serialize Cargo jobs and reuse existing fixtures/policy coverage. No test-per-widget, repeated domain adapter suites, coverage quota or broad platform matrix.

Meaningful regression coverage is bounded to: (a) ECS-backed state with view objects removed/recreated, proving the same ChatIds/components retain drafts/config/permissions and continue receiving routed events; two live chats and hidden-route polling, including same provider and targeted Stop/approval/error; (b) exact new-record transfer and stale-generation rejection; (c) metadata startup/owned cleanup without prompts/history load; (d) Kimi whole-snapshot reasoning replacement and isolated failures. Extend existing scenarios rather than testing every accessor/widget.

During development choose affected owner/catalog/adapter scenarios by exact filter, for example `cargo test --lib --features gui,codex,kimi conversation_runtime::tests` or `model_catalog::tests`, plus the changed Codex metadata-startup scenario. The developer reports exact filters and passing results; do not repeat unchanged domain or baseline suites.

At the stable checkpoint root combines the relevant UI/runtime/catalog/adapter scenarios once, using the focused filters from the handoff and reusing already-valid results. Then run formatting, the ordinary native GUI build required for inspection, and the Kimi-only compile protecting feature independence:

```sh
cargo fmt --check
cargo build --bin tanglewood --features gui,codex,kimi
cargo check --features gui,kimi
```

No unconditional full library suite or all-target Clippy run is required. Use a targeted Clippy check only if a concrete changed-code concern warrants it; do not fix unrelated baseline warnings. No release/Android build or paid prompt is needed/authorized for this slice. Use deterministic fake streaming/permission fixtures for background execution acceptance; metadata can use actual installed authenticated CLIs without prompts. Host tests do not prove new external-live subscription or network isolation.

Native acceptance, one bounded pass:

- In a regular Linux window open a fresh model picker. Both enabled headings appear immediately; real catalogs load without individual connects. Select either provider, type, reach enabled Send without a separate provider/connect/new-session action; do not Send. Inspect flat search/current check and separate valid reasoning; unavailable installed alternatives are reported honestly.
- With deterministic running conversations, navigate between two chats (including same provider), Research and Notes. Both continue receiving output; errors/permissions stay with their records, indicators locate the right chat, and answering/Stop affects only its target. Existing draft and Notes edits survive. No navigation-triggered disconnect or replayed prompt.
- From existing history select another provider: Cancel preserves selection/state; confirm creates a new target with exact draft/snapshots. Original history, original running work and existing destination-provider drafts remain. No prompt emitted. Check one local preparation failure/retry.
- Inspect truthful Connections catalog/runtime states and explicit disconnect consequences. Stop discovery without respawn; verify/disclose temporary-session cleanup. One failed helper does not disable another provider or running chat.
- Check about 390 logical points: search, keyboard selection/Escape/focus return, wrapped controls, reachable Send/model/reasoning and background indicators. Capture actual picker/Connections and cross-provider result against the quiet reference.

Reviewer gates: conversation components in the existing App World with identity-only indices and optional App-owned transport handles; no authoritative UI/parallel record copy or mutable World rendering access; partial observation/client ownership independent of view lifecycle; fewer normal-path steps; every enabled catalog represented; bounded metadata cleanup and fair event routing; no draft/history/context loss; actual model-specific reasoning; explicit targeted permission/cancellation/disconnect; unchanged MCP boundary; native visual evidence. Stop after passing relevant evidence; rerun only after a relevant change/failure. Report unsupported external discovery/live observation and unavailable native evidence accurately.
