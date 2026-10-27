# Manifest-authorized Wasm commands and script execution plan

**Status:** Implementation authorized by the user on 2026-10-28. Selected model B: manifest ∩ caller rights, with the existing vault agent-write switch retained. Optional delegation/per-script policy layers are not selected. Scope is all existing shared Command/ResearchCommand variants; internal conversation/lifecycle methods remain outside this increment.

**Goal:** Let Wasm components invoke every supported shared application command according to manifest permissions, and let interpreter components evaluate scripts supplied explicitly by agents or Tanglewood.

**Architecture:** App remains the sole owner of its ECS and application state. Wasm workers send bounded typed requests to an owner-thread command broker; authorization and normal domain validation happen before dispatch. Compiled extensions and interpreter extensions share that broker, while each invocation has its own immutable authority context and fresh guest state.

**Tech stack:** Existing Rust 1.94 package, Wasmtime/WASI 47.0.4, versioned WIT and wit-bindgen, existing Serde command types, and JSON manifests. Keep Rhai inside Wasm. Select any additional dependencies during implementation only after checking compatibility.

## 1. Current implementation and scope

Reference baseline: fc9ba3c on main. Read docs/plan/tanglewood-mvp.md, the existing Wasm plan, docs/wasm-extensions.md, and docs/plans/2026-10-27-vault-agent-permissions.md before implementation.

- src/wasm_extensions.rs currently provides isolated execution, logging and a single selected-note snapshot. It cannot call the live controller.
- src/main.rs owns the current headless component/script invocation path.
- src/commands.rs defines shared Command, CommandResult, CommandError and caller policy. Research subcommands currently share the coarse descriptor research.execute.
- src/app.rs dispatches shared commands, owns application state and processes worker completions in tick(). Research and some other controller state are outside the ECS but have the same owner.
- src/mcp.rs exposes capabilities/read/execute through a single-vault owner socket. Its acknowledgement handling distinguishes accepted from persisted work.
- Conversation operations also exist as crate-internal methods/actions; lifecycle methods such as flush/shutdown are not shared Command variants.

Proposed first scope is **every existing shared Command variant and ResearchCommand variant**, including currently local-only commands when a locally authorized plugin has the necessary grant. Platform/feature availability and domain validation still apply. This is broader than the MCP allowlist.

Do not interpret this as exposing Rust methods, World, raw connections, or a Caller::Local identity. Inventory conversation controls and lifecycle/query helpers separately. If those must also be callable, promote the intended operations to typed commands in an explicitly added increment; never wrap arbitrary App method names. Blocking shutdown/flush must not run inside a synchronous guest callback.

## 2. Permission models to choose between

| Model | Effective authority | Advantages | Costs and failure modes |
| --- | --- | --- | --- |
| A. Manifest-only plugin principal | An installed plugin receives its manifest grants regardless of caller | Simple; supports privileged application extensions and scripts invoked by constrained clients | Any accepted agent script obtains the interpreter's authority. A general interpreter with administrative grants becomes a privilege gateway. |
| B. Manifest intersected with caller rights | Both the manifest and invoking principal must allow each operation and resource | Predictable default for agent-supplied programs; preserves existing MCP boundaries | An agent cannot use a plugin to perform a narrowly approved operation outside its direct command rights. Tanglewood needs an explicit local invocation principal. |
| C. Per-script policy in addition to interpreter policy | Interpreter ceiling intersected with a named/hash-bound script manifest and origin policy | Different scripts can have different rights; good for reusable application workflows | More policy files and versioning; arbitrary new agent source has no trusted script identity and needs a default policy or explicit delegation. Interpreter permissions alone cannot authorize a script. |
| D. Explicit delegated invocation capabilities | Default intersection, augmented by a manifest-declared delegation to a specific caller/script/workflow | Supports bounded privileged automation without granting the agent the same broad direct rights | Requires precise scope/expiry/provenance and revocation; a blanket delegation to arbitrary source collapses back to model A. |

Model C is an additional policy layer and can combine with A, B or D. Do not implement four parallel policy engines: select one base model and only the additional layers actually approved.

### Grant granularity is a separate choice

- Role profiles such as reader, editor, researcher and administrator are convenient manifest presets, but expand to a versioned explicit rule set; a role must not imply Caller::Local.
- Per-command capabilities are more precise and map directly to shared dispatch, at the cost of longer manifests.
- Resource and parameter constraints narrow either approach: note/workspace IDs, filesystem roots, URL origins and administrative targets. A command-only allowlist cannot express “edit only this note.”
- Invocation approval is a separate mechanism from durable manifest policy. This plan does not require a prompt for each call; an optional explicit delegated-run grant can be added only if selected.

Recommendation: use scoped capabilities as the policy representation, with optional role presets for authoring convenience.

### Provisional recommendation

Use B as the default, with D available for explicitly named, scoped delegated workflows. Add C when Tanglewood has registered reusable scripts needing different grants; avoid requiring a separate manifest for every one-off read-only agent script.

For example:

- A locally invoked compiled plugin may have explicit permission to configure LAN or export Markdown.
- A Rhai interpreter may declare a ceiling covering note reads/edits. An agent without write authority receives a read-only execution grant by default.
- A manifest may explicitly delegate editing one designated note to one workflow. That is an intentional bounded authority grant, not a side effect of choosing Rhai.
- A Tanglewood-triggered script gets the permissions of its registered invocation binding, bounded by the plugin manifest; it never receives an implicit unrestricted Local caller.

Proposed policy calculation:

    effective grant =
        requested operations/resources
        intersect plugin manifest ceiling
        intersect optional registered-script policy
        intersect caller authority plus explicit applicable delegation
        intersect platform availability and fixed host restrictions

Grants are rules coupling an operation to resource/parameter constraints. Do not union all commands and all resource scopes independently: permission to read every note plus edit note A must not become permission to edit every note.

For agent-origin work, the provisional recommendation keeps the vault agent-write switch as a hard gate on mutations, including delegated ones. Local Tanglewood workflows are governed by their manifest grants. Whether a delegated agent workflow may override that switch is a deliberate product decision, not an implementation detail.

The existing accepted vault-permission design allows only a local user action to change that switch. Making automation.set_write_enabled available to an explicitly authorized local plugin would intentionally extend that rule and must be recorded as such. Under the recommended model, agents cannot grant themselves writes through an interpreter, a delegation profile, or an administrative alias. A different choice needs an explicit revision to the existing permission policy.

### Decisions to settle before implementation

1. Choose A, B, or D as the base model; confirm whether C is needed immediately.
2. Confirm whether agent delegation may cross the vault-wide write switch.
3. Confirm that “all commands” means shared Command/ResearchCommand variants, or add conversation/lifecycle controls to the required inventory.

Everything below is designed to support those choices without assuming unrestricted authority.

## 3. Manifest and identity design

Use a versioned JSON manifest, leveraging existing Serde/serde_json. Proposed fields:

| Field | Purpose |
| --- | --- |
| manifest_version, id, version | Stable plugin identity and schema |
| kind, languages, wit_version | Compiled/interpreter contract and discovery metadata; language names are metadata, not native dispatch cases |
| component, component_sha256 | Bundle-relative component path and identity of the bytes authorized |
| permissions.rules | Explicit command names with note/workspace/path/network/administrative constraints |
| invocation.sources | Whether local Tanglewood and/or agent requests may invoke the plugin or supply source |
| delegation | Optional explicit workflow/caller/resource grants, if model D is chosen |
| limits | Requested budgets capped by host ceilings |

Illustrative permission rules, not a finalized file schema:

    rules:
      - command: note.read
        notes: all
      - command: note.edit
        notes: [designated-note-id]
      - command: research.read_source
        workspaces: [designated-workspace-id]

Default deny. Unknown fields, capabilities and incompatible manifest/WIT versions fail closed with actionable errors. Expand any explicit “all commands” grant to the known command set for that manifest/API version; do not silently grant future command additions.

Manifest storage is a locally controlled registry/bundle location, distinct from synced note content. Agent requests select an installed plugin ID, never an arbitrary manifest/component path. Receiving a plugin reference or script in a note does not install or run it.

Current MCP authentication establishes a same-user socket client and a vault policy, not a distinct trusted identity for each named agent. Initially use that authenticated origin class, or add a host-issued session-bound capability before enabling per-agent grants and private run ownership. Client-provided names, plugin IDs and run IDs are not credentials. The same identity rule governs polling/cancellation and reconnects; do not claim one agent's results are private from another without that boundary.

Manifest changes are the grant mechanism; there is no mandatory prompt for each callback. Installation/loading must establish trusted local provenance. Application file-writing commands must not provide an indirect write route into the authority registry. If an administrative plugin-management capability is later introduced, it needs its own explicit design.

Read and bind the actual component bytes to the manifest identity before compiling; avoid check-then-reopen hash races. Bind each run to plugin ID, component digest, manifest generation, vault identity/generation, invocation origin and applicable script identity/digest. Reject path escapes/symlinks according to the bundle policy. A vault replacement/join invalidates affected run bindings rather than transferring old authority to a new vault.

Reductions/revocation affect the next owner dispatch and cancel affected runs. Expansions apply only to newly authorized invocations. Plugins cannot rewrite the grant attached to an existing run.

Keep authorization to application-mediated filesystem/network operations separate from WASI grants. Permission to SavePaper from an allowed URL does not grant WASI sockets; permission to export through App does not grant ambient WASI filesystem access.

Carry scope restrictions to the actual worker operation. A lexical path check at command admission does not constrain a later symlink resolution; an allowed initial URL does not authorize every redirect. Retries and previously collected job IDs must retain their original scoped authority and cannot be used to recover a broader local operation.

## 4. Command contracts and dispatch

Inventory every shared variant, including fine-grained research actions, and define stable canonical command names. Avoid granting the whole research family merely because its existing descriptor is research.execute.

Inventory effect and completion semantics alongside permission names. Existing descriptor.mutation and Command::is_mutation() differ for some commands, including local-only operations, so neither Boolean is a sufficient new-plugin policy model. Distinguish query/action/program-execution admission, actual vault-mutation authority, and completion mode.

Add a trusted plugin invocation context to dispatch rather than pretending the plugin is Local. It carries origin and validated grants from the host; no serialized command/script payload can construct it. Audit all existing caller checks, including parameter-sensitive and retry-job checks for local files. Granting a command name alone must not bypass its resource restrictions.

Preserve existing Local and MCP behavior. Locally restricted commands can be dispatched for an explicitly authorized plugin principal; agent-origin restrictions follow the selected permission model. Validation of revisions, paths, current state and feature availability stays in the same domain handlers.

Publish a new WIT package version, provisionally 0.2.0:

- Typed command/request/result/error variants and records, organized by subsystem where that makes the contract smaller to maintain.
- Explicit UUID/revision values, source ranges, job receipts, permission failures and availability errors.
- Compiled run(input) and interpreter evaluate(source, input) continue to share the same host command capabilities.
- Discovery reports the currently effective grant, not just every command compiled into the host.

Do not replace the host API with one opaque JSON-RPC string. Define the WIT-to-domain mapping explicitly and require exhaustive Rust conversion matches plus a parity check against the shared command inventory. Allow opaque strings only for intentionally opaque source/input or metadata. Decide whether 0.1.0 stays as a read-only compatibility world; do not silently reinterpret old imports.

Commands returning large lists/text need bounded responses and an explicit paging/continuation strategy or a structured TooLarge response with a usable bounded read path. Do not silently truncate data or remove commands from the inventory to avoid handling their result sizes.

## 5. Owner-thread broker and execution lifecycle

    UI / headless / MCP request
        -> App extension invocation command
        -> registry + origin policy -> effective grant -> bounded Wasm job
        -> guest host callback -> bounded owner request queue
        -> App.tick() checks current grant and dispatches the shared command
        -> typed receipt/result back to the worker -> guest

The ECS never crosses threads. Guest callbacks may wait only on their worker, with bounded queues, cancellation and a deadline. App.tick() drains a bounded number of requests; it never waits for a guest. Return Busy on admission/queue exhaustion.

Fuel cannot bound waiting/native work. Maintain a separate wall-time/cancellation path while callbacks await the owner. Audit command handlers for blocking I/O and large synchronous work; preserve the owner model and use existing jobs for slow operations.

Extract/reuse the existing MCP acknowledgement logic where appropriate instead of copying persistence rules into the Wasm host. Model accepted, pending, persisted and failed outcomes explicitly. A guest may receive an accepted job receipt and query it later; only a confirmed durable result is reported as persisted.

Reuse the checker for domain commands that actually await a note revision or I/O receipt. A plugin execution ID is a different receipt: the start command returns it immediately and must not enter the current generic MCP mutation acknowledgement wait. Otherwise MCP waits for an app.io_status entry that will never exist, or delays the response until the whole script finishes. Do not avoid that wait by labeling arbitrary script execution an ordinary read.

Cancellation/deadline/revocation removes requests not yet accepted. An already accepted mutation can finish after the guest stops. Retain a bounded receipt and report partial/unknown outcomes so callers can inspect state; never automatically replay a script after a trap or timeout.

Retain fresh Stores/instances, Engine/compiled-code reuse, current memory/table/stack/fuel bounds, bounded diagnostics and restricted WASI. Add per-run command count, bytes, queue and downstream-job limits. Measure elapsed budgets across owner waits. Preserve the documented limitation that compilation is off-thread and cannot be interrupted like guest execution.

Prevent recursive plugin invocation by default. If nested execution is later allowed, carry the original principal and narrowed grant, bound depth/total budgets, and never create an independent fresh authority context. Use the same rule for a guest that starts an agent conversation: automated descendants must not regain broader credentials.

## 6. Interpreter registration and script sources

Register Rhai as an ordinary interpreter component implementing the shared interpreter world. Another installed plugin can advertise another language and implement the same world; no native Rhai dependency or language-specific case in App is required.

Provide typed extension commands for discovery/description, compiled invocation, script evaluation, run-status/results and cancellation. Exact variant names are provisional; all entry points use the same owner path.

Support two source providers:

- Agent: bounded inline source and input submitted through MCP, referencing an installed interpreter ID. Origin/caller identity comes from the trusted adapter.
- Tanglewood: explicit local command or registered workflow supplies bounded source/input, including an explicitly selected immutable note revision or registered local script file. Record provenance; do not execute code merely because Markdown contains it.

The interpreter manifest sets a ceiling; the chosen permission model supplies the script's actual grant. Passing arbitrary code to an interpreter is a different delegation from passing arguments to a fixed compiled plugin.

MCP should expose invocation/status/cancel through its existing typed execute path. The read tool must remain side-effect-free and must not execute arbitrary scripts, even scripts requesting only read capabilities. Separate execution admission from direct-mutation classification so a read-only agent can run an authorized read-only script without gaining writes.

| Operation | Admission | Authority required | Completion |
| --- | --- | --- | --- |
| Start compiled plugin / evaluate source | Execute tool only; permission to run the selected plugin | Effective manifest/caller/delegated grants; may be read-only | Immediate accepted execution ID; does not imply durable guest effects |
| Read run status or bounded result | Query path after ownership/access checks | Authenticated run access; retain outcome/receipt access after write revocation without regranting data rights | Immediate bounded snapshot |
| Cancel run | Execute/control path after ownership checks | Run-control authority, independent of current vault-write permission | Prompt cancellation acknowledgement; already accepted domain effects may finish |

The owner/bridge must represent these axes explicitly. Turning off writes must still permit a caller to cancel its running script and inspect its outcome. Underlying guest mutations keep normal durability rules even though the outer invocation was acknowledged immediately.

ACP and Codex agents use the registered MCP tools; do not add a second ACP-specific application command surface. Script requests from agents must not accept origin=local or arbitrary permission/manifest payloads.

Return an execution ID promptly, allowing the UI/agent to poll or cancel while the owner stays responsive. Large outputs/diagnostics use bounded retrieval. Keep run ownership so one caller cannot cancel or read another caller's private result without an explicit grant.

## 7. Staged implementation and file ownership

This is a future execution plan. Do not begin these tasks until the user authorizes implementation and the policy decisions above are settled. Use Terra 6.1 max for development and Sol 6.1 max for review if the current model preference remains in force. PM owns shared files/index and serializes Cargo work.

### P1 — Command and permission contract

Files: src/commands.rs, src/research.rs, src/app.rs; proposed src/plugin_permissions.rs; docs command inventory in this plan or a companion table.

1. Inventory every command/parameter-sensitive policy and identify internal methods outside the shared contract.
2. Write the selected authority rules, stable capability names, resource-scope semantics, command effect/completion inventory and revocation behavior.
3. Add focused policy regressions for one read, one scoped edit, one local-only administrative command and a parameter-sensitive local-file retry.
4. Implement trusted plugin principal authorization while preserving existing Local/MCP behavior. No Caller::Local substitution.

Acceptance: all current shared variants have an intentional capability mapping; denied/feature-unavailable operations fail clearly; existing automation cannot gain privileges.

### P2 — Manifest loading and registry

Files: proposed src/plugin_manifest.rs and src/plugin_registry.rs; example manifests under examples/wasm-extensions/; PM owns any Cargo/lib declarations.

1. Define versioned manifests for the compiled example and Rhai interpreter.
2. Test malformed/unknown grants, resource scopes, bundle escapes/digest mismatch and changed generations.
3. Implement locally trusted registration/loading and derived effective grants under the selected model.
4. Add explicit reload/revoke behavior; keep the authority store outside application file-write destinations.

Acceptance: an agent can invoke a registered ID but cannot supply its own manifest or choose component bytes. Reload does not expand active grants.

### P3 — Typed WIT and command mapping

Files: wit/ versioned packages; proposed src/wasm_commands.rs; guest binding users and fixture WIT; src/wasm_extensions.rs only by assigned owner.

1. Define complete subsystem command/reply/error types and compatibility behavior for 0.1.0.
2. Add exhaustive conversions and command-inventory parity checks; avoid duplicating domain validation.
3. Verify representative note, research, local-only and asynchronous commands, plus bounded result handling.

Acceptance: no existing command is silently omitted; types/results match native semantics; old components either retain their old contract or receive an explicit version error.

### P4 — App-owned command broker and execution jobs

Files: proposed src/plugin_jobs.rs; src/wasm_extensions.rs; PM integration in src/app.rs/src/commands.rs; narrowly extracted acknowledgement helper if needed by src/mcp.rs.

1. Add bounded job admission, worker-to-owner command requests, distinct immediate execution receipts/domain durability receipts and nonblocking owner polling.
2. Connect current authorization/revision checks at actual dispatch time.
3. Test UI-owner progress while a guest waits, cancellation before/after acceptance and after vault-write disable, revocation, Busy/backpressure and persisted/unknown outcomes. Verify a start receipt returns promptly without entering the domain durability wait.
4. Preserve execution budgets and isolation, including during host waits and downstream job creation.

Acceptance: all app mutations run on the owner through shared handlers; neither callback waits nor shutdown introduce an owner/worker deadlock; partial effects are inspectable.

### P5 — Script entry points and generic interpreters

Files: src/mcp.rs, src/main.rs, examples/wasm-extensions/rhai/src/lib.rs, example scripts/manifests, focused integration tests.

1. Add typed discovery/invoke/evaluate/status/cancel commands and CLI support selecting registered IDs.
2. Expose agent script evaluation through MCP execute with trusted provenance and applicable grant; keep MCP read non-executing. Use origin-class identity unless a separate authenticated session/run identity has been implemented.
3. Demonstrate one Tanglewood-supplied script and one agent-supplied script using the same Rhai component.
4. Demonstrate changing source without rebuilding and registration by interpreter contract rather than a hardcoded Rhai switch.

Acceptance: the same command executes through native and guest paths with the same domain result; each source gets its intended effective authority. The future-interpreter contract is documented without adding JS/Python now.

### P6 — Independent review, documentation and integration

Files: docs/wasm-extensions.md, docs/mcp.md, docs/capabilities.md, docs/mvp-status.md, new evidence record.

1. Freeze implementation and review authorization, data preservation, ownership, receipt semantics and compatibility.
2. Resolve concrete findings with focused regressions; review affected deltas.
3. Run combined relevant checks once, record live/headless evidence and limits, then make explicitly scoped unsigned integration commits when authorized.

GUI controls are a separate exposure decision. This increment needs an invocation path from Tanglewood and agents, not a plugin marketplace, arbitrary guest UI or automatic note-code execution.

## 8. Focused acceptance and validation

Reuse existing Wasm guests/scenarios and native command tests; do not duplicate every domain test through every adapter.

Required behavioral evidence:

- Typed command inventory covers all existing shared variants, including research and local-only commands.
- Manifest allow/deny and resource scoping work; read-any plus edit-one does not become edit-any.
- Selected policy distinguishes a local application run, an agent read-only run and an agent write-enabled run. If model D is selected, additionally verify an explicitly delegated run and its scope/expiry/revocation; otherwise delegation requests are unsupported and confer no authority.
- A valid administrative manifest can authorize a local-only controller command; an unauthorized agent cannot acquire that authority by selecting an interpreter, spoofing origin, nesting a run, or changing the manifest through a file command.
- Revocation/agent-write disable affects queued and subsequent calls; accepted effects remain accurately reported.
- Real compiled and Rhai guests read and mutate a real temporary vault with revision checks. A stale revision preserves the intervening edit.
- An MCP caller supplies source and receives an execution ID; Tanglewood supplies different source to the same interpreter; both paths support status/cancellation.
- A read-only caller can start a read-only script through execute, while the read tool rejects program invocation; execution admission does not imply mutation authority. Write disable preserves owned-run cancellation and outcome access. Another same-user client is not treated as a distinct private agent without a host-issued identity.
- Host waits, queues and downstream jobs are bounded; guest trap/cancel after one accepted mutation exposes the partial outcome rather than implying rollback.
- Existing fuel/memory/state-isolation/WASI tests remain applicable; compatibility behavior is verified.

Future implementation commands, not checks run for this document:

    ./scripts/build-wasm-extensions.sh
    cargo test --locked --features wasm-extensions --test wasm_extensions --test wasm_cli
    cargo test --locked --features wasm-extensions,mcp --test wasm_plugin_commands
    cargo test --locked --lib --no-default-features -- --test-threads=1
    cargo clippy --locked --all-targets --features wasm-extensions,mcp

The proposed wasm_plugin_commands integration target is created by P4/P5; feature-gate its MCP-specific portion. Extend existing tests where practical instead of inventing another test framework. Add focused per-module checks during development; use the combined commands at integration only when they add evidence.

Check formatting of changed Rust/guest files and git diff whitespace. Repository-wide formatting already has unrelated drift; preserve unrelated files. Host checks do not prove Android execution, live agent behavior or GUI responsiveness. A deterministic owner-loop check proves the threading contract; live MCP/UI smokes belong at their actual adapter gates.

## 9. Planning handoff

Only this proposed document is changed by the planning task. No dependency, WIT, runtime, controller, guest, UI or adapter implementation is included; no Cargo checks are needed for the document. Implementation remains stopped pending user direction and the three policy/scope decisions in section 2.
