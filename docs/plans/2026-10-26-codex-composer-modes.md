# Composer modes and goal banner

User requests Work/Plan/Research/Goal selector beside model selection, an approval selector,
and a compact current-goal banner above the composer. Context remains useful but
its existing small icon is hard to discover: label its header toggle Context.

Root owns src/ui.rs, src/ui/codex_context.rs and docs; context_protocol owns only
src/codex.rs for bounded turn controls. Reviewer reads frozen delta. Serialize
Cargo; preserve prior changes; no commits requested.

Contract: public WorkMode {Work,Plan,Research,Goal}, ApprovalMode {Ask,AutoReview},
TurnControls {mode:WorkMode,approval:ApprovalMode,network_access:bool} derives Clone/Copy/Debug/Default
(defaultWork/Ask). Add SendPromptWithControls with all SendPromptWithContext
fields plus controls:TurnControls. Preserve all existing legacy send behavior.
Use real turn/start collaborationMode plan/default settings (model and effort
from explicit effective selection, developer_instructions:null), not promptprefix
substitution. Goal uses default collaboration plus required persisted activegoal
managed by existing explicit goal controls. Approval routes reviewer user versus
auto_review, policy on-request, retaining read-only filesystem and independent MCP policy. Explicit network_access applies to each controlled turn and defaults false.
Full access is unavailable in this pass: a reviewer change must not silently
introduce direct vault file writes or grant MCP write permission.

UI: compact approval, Work/Plan/Research/Goal and model controls in the composer footer,
wrapping when needed; mode adjacent to model. ChoosingGoal opens goal setup if
absent, resumes if paused; switchingWork/Plan pauses an activegoal before Send.
Block inconsistent sends on goal mutation failure. Currentgoal banner shows
objective/status, server-reported elapsedtime, pause/resume,clear and expand.
Mode/approval reset to Work/Ask when opening a conversation; picking controls
does not submit turn/start. Activating a persisted goal is explicit and may
allow server-scheduled work; the setup dialog explains this. Currentgoal appears even ifthe
rightcontext panel is collapsed. Goalupdates remain thread-correlated.

Checks: focused real protocol field/guard fixtures and UI mode/state checks;
combined host checks at stable integration; native label/layout/menus smoke without
submitting generation prompts or granting unrestricted access. Record actual limits accurately.

User refinements: command execution should be visible, with command/status/exit
and expandable bounded output; commands already run inside the read-only sandbox,
with broader command grants handled by existing Allow once approvals. Add an
Access popup with explicit network permission, separate from approval reviewer.
No false promise of a blanket command-disable switch or unrestricted file writes.
Research is a convenience preset using default collaboration (Plan would prohibit
saving results), explicit research instructions, network enabled, selected model/
approvals and existing exact attachment snapshots. It can create notes via MCP
in Research/<topic>/ or the user-requested destination, with Summary.md/Sources.md
when useful. Existing note source stays intact unless the user requests edits.
The owner must be launched with --mcp-write to advertise/allow note creation;
profile selection cannot elevate the owner or bypass validated note commands.
Network defaults reset with profiles and when opening another conversation.
Configured arbitrary profiles and history command cards remain deferred.

Review corrections: live goal notifications retire old reads but never pending
mutations; only correlated acknowledgements resolve pending UI actions, followed
by a fresh goal read. Reconnect requires fresh goal state before Send. Goal entry
resets network off while later explicit Access choices survive usage updates.
Nullable command aggregate output retains existing streamed output/truncation.
Confirmed active goals can receive server-started turns through normal scoped
message, command, approval and timeout handling. Unknown unsolicited turns stay
ignored. Composer height is measured for bottom placement and wrapped controls.
