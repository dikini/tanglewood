# Structured plan and approval corrections

**Goal:** Replace repeated progress-list narration with real plan updates and expose the documented stronger approval combinations.

## Evidence and diagnosis

The user tried the suggested prompt in the real demo vault. Its Context Plan panel remained empty while assistant messages repeated numbered status lists. Read-only scoped history inspection of the exact project/thread found ordinary agent messages and MCP calls, with no typed plan item. History alone cannot prove whether ephemeral plan notifications were emitted. Investigate the installed0.163.0 planning-tool setting; enable the actual built-in update_plan tool and instruct the agent to update it rather than narrating the whole list repeatedly. Do not scrape Markdown lists or delete authored history.

The bridge currently publishes capabilities and a multiplexed execute tool with no read-only annotations. The execute tool can write under a read-write owner, so marking it universally read-only would be incorrect. Add an honest read tool using the existing command schema/validation. Tool annotations and MCP approval mode are independent of command approval and sandbox settings.

## Documented combinations

Official sources checked2026-10-27: learn.chatgpt.com/docs/config-file/config-reference,
learn.chatgpt.com/docs/extend/mcp, learn.chatgpt.com/docs/sandboxing/auto-review,
and learn.chatgpt.com/docs/app-server. Compare these to pinnedCLI-generated schemas.

- Ask: approvalPolicy on-request, approvalsReviewer user, readOnly sandbox, explicit network boolean.
- Approve for me: on-request, auto_review, same sandbox/network boundary.
- No prompts: never, user reviewer (irrelevant without prompts), readOnly sandbox and explicit network. Actions requiring escalation can be rejected; never is not a permission grant.
- Full access: never, user reviewer, dangerFullAccess sandbox. Includes unrestricted command/file/network access and must only follow explicit UI selection, never the default. Note MCP owner permissions/revision checks are independent; instructions still route vault operations through the shared command boundary.
- MCP read: truthful readOnlyHint annotations allow ordinary reads without unnecessary prompts under default auto policy.
- MCP mutations: default tool approval remains honest; stronger server per-tool approval values are auto/prompt/writes/approve, not the reviewer setting. Do not silently write global Codex configuration or auto-approve unknown forms. Verify what never/full-access does with actual pinned tool policy before claiming approval-free mutations.

## Ownership / tasks

MT1 developer owns src/mcp.rs only: add read tool with read-only schema and enforce mutation/local-only rejection before forwarding; annotate capabilities/read truthfully, keep execute compatibility and existing owner validation. Focused metadata/forged-call fixture; no Cargo until root grants it.

CP1 context_protocol owns src/codex.rs only after diagnostic handoff: verified planning knob and guidance; extend ApprovalMode NoPrompts/FullAccess and controlled-turn wire combinations, preserving legacy safe sends. Command/network/MCP failure paths remain explicit. Focused real-field fixture and source-only diagnostic, no user-vault mutation.

UI1 root owns src/ui.rs and docs: approval picker options/tooltips and accurate effective Access display for Full access. Keep explicit network preference when leaving Full access, reset elevated modes when opening conversations. Preserve draft/goal/attachment behavior and normal reading UI. Existing plan updates continue to replace the single view; old assistant messages remain authored history.

RV1 reviewer reads frozen delta for truthful metadata/validation, plan tool contract, explicit grants/no default elevation, stale ownership and source preservation. No writes/Cargo. Root serializes combined checks/build and a bounded disposable live plan/read test if authorized by task scope. Never infer service behavior from mocks. No user-visible new task, commit or global config changes.

## Stopping rule

Regression fixtures cover the discovered gaps. One relevant combined host check and native picker smoke; live test only in an owned disposable vault/thread, with any generated remote thread cleaned up explicitly. No repeated Android/network matrix. Record actual approval counts/typed plan events and remaining managed-policy limitations.
