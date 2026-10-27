# Codex context conveniences

User approved both a thread context panel and composer controls, with Codex as
the visual/functionality reference. Preserve minimal chrome and incremental MVP
scope. A collapsed right-side context panel shows goal, plan, files/results and
available configured capabilities. Composer + adds source snapshots/skills and
opens explicit goal/plan controls. No new generation merely by opening panels.
Sketch semantics pending user choice; generated previews can be added through
file/image events, but no drawing canvas is assumed.

Ownership: context_protocol developer owns src/codex.rs only; root owns UI,
any file loader and documentation; reviewer reads frozen delta. No shared index
changes/commits, no parallel Cargo. Existing changes preserved.

Protocol contract (public adapter-local types):
- ContextFile { path:String, kind:String }
- PlanStep { text:String, status:String }
- GoalInfo { objective:String, status:String, tokens_used:u64, token_budget:Option<u64> }
- SkillInfo { name:String, path:String, description:String, enabled:bool }
- ConnectionInfo { name:String, auth_status:String, tools:usize }
- PromptAttachment { name:String, text:String }
- Requests RefreshContext; SetGoal{objective:String}; PauseGoal; ClearGoal;
  SendPromptWithContext{text:String,model:Option<String>,effort:Option<String>,attachments:Vec<PromptAttachment>,skills:Vec<SkillInfo>}.
- Events Goal{thread:String,goal:Option<GoalInfo>}; Plan{thread:String,steps:Vec<PlanStep>,text:Option<String>};
  Files{thread:String,files:Vec<ContextFile>}; Skills(Vec<SkillInfo>); Connections(Vec<ConnectionInfo>).
Refresh gets selected thread goal plus skills/list for canonical cwd and bounded
mcpServerStatus/list. Skills must be allowlisted from latest catalog before send;
source snapshots become explicit text input items, never direct vault access.
Existing model/effort validation applies. Bound snapshot payloads to prompt budget,
max8 attachments/skills, context lists128 entries/plan64; thread correlation,
event byte budget and no new mutation retries. Goal setters carry user provenance,
only UI explicit confirmation; no goal started in this implementing conversation.
Handle live goal/plan/file/image events and hydrate derived items from history.
Do not use under-development plugin/list production API; show reliable skills
and configured MCP connections, with honest unavailable/empty states.

Root UI retains per-thread attachments/skill choices with draft ownership, keeps
revisioned source snapshots and provides copy/open previews through a bounded
background file reader (no UI-thread filesystem jobs). Goal set dialog explicit,
Pause/Clear explicit controls. Plan request is an editable composer prompt, not an
automatically submitted model turn. Native previews only supported safe assets;
no browser HTML execution or arbitrary external path guessing. Optional context
views do not change note source, AI permissions, or vault project identity.

Validation: focused protocol and UI ownership fixtures, serialized combined host
checks at integration, native layout/catalog smoke without fresh paid generation.

Delivered: context panel plus composer controls, exact source snapshots,
allowlisted skills, paged configured connections, explicit goal state controls,
live/history plan and file results, reviewed plan/sketch request prompts. PNG
worker preview is pixel/byte bounded with one owned texture; SVG stays source.
Review corrections retain queued context until correlated acceptance/rejection,
prevent stale goal overwrites, preserve pending file ownership, and keep optional
inventory errors independent of turn submission. Independent review READY.
See docs/codex-live-evidence.md for exact host/native checks and scope limits.
No additional model turn or real goal mutation, no commit made.
