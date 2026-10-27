# Research workspaces design

Approved in conversation on 2026-10-27. Research covers any subject, including food fermentation, boat design and fashion trends. It is not an academic-paper manager. This extends the local notes application; it does not invoke the executable MVP goal.

## Product model

A research workspace groups a question, explicitly chosen sources, interlinked Markdown research notes, agent conversations and generated outputs. Notes remain ordinary editable notes with stable IDs, source-preserving commands and backlinks. Sources include existing notes and imported PDFs/text initially. Web pages, videos/transcripts, images, manuals, catalogs, recipes and personal observations fit the same provenance model as later import adapters. Author, publication date and URL are optional; title, origin and inspected content matter across all domains.

Source collection and source discovery are separate. The initial release operates only on material the user adds. Reserve a Discover sources action and candidate model for a later network-enabled increment. A discovery run may inspect and cite temporary candidates; permanently collecting a candidate is explicit, using the same extraction/snapshot pipeline. Preserve origin and capture/version information; do not treat search snippets as inspected full sources.

## Evidence and assessments

Each imported source or note selection captures an immutable version. A note's ID and revision identify the content used, even if the live note later changes. PDF text is page-aware, with stable source IDs and original page numbers. Citation navigation opens the captured passage/page, not a guessed current note range. Original PDF locations are provenance; retained extracted text is enough to inspect the evidence used. Initial PDF extraction uses an installed local tool, with an actionable unavailable/OCR error for scanned or unsupported documents; OCR itself is deferred.

Relevance and quality are separate and contextual. Consider fit to the question/constraints, directness, evidence/method, demonstrated expertise, applicability, currency, independence, and incentives. Weight those dimensions by purpose rather than issuing a universal numerical trust score. A build log can be useful evidence of construction experience without establishing a general safety rule; a retailer's catalog directly documents its marketing but not public adoption.

Recorded metadata, agent assessments and human judgments remain distinguishable. Assessments explain their evidence and limitations; unknown values stay unknown. Preserve user corrections and exclusions. Findings should distinguish supported claims, disputes, inferences and insufficient evidence. Citations establish traceability, not automatic correctness. Initial assessment UI records contextual relevance, evidence quality and limitations; it does not pretend to automatically verify sources or calibrate confidence.

## Research strategy and agents

The general loop is: question -> working plan -> gather evidence -> compare -> identify gaps -> investigate -> compose. Strategies vary by purpose: learn/explain, compare/decide, troubleshoot, design/explore, track trends, or plan an experiment. Suggested continuations target gaps such as independent corroboration, conflicting observations or applicability to different conditions. Initial suggestions are editable prompts, not claims that an agent has already evaluated the evidence.

Perplexity publicly documents decomposition into multiple searches, iterative search/read/reason/plan revision, synthesis with citations, follow-up refinement, and mixed personal/external sources. These inform the design; its undisclosed internal algorithms are not assumed. References: [Pro Search](https://www.perplexity.ai/help-center/en/articles/10352903-what-is-pro-search), [Deep Research introduction](https://www.perplexity.ai/en-GB/hub/blog/introducing-perplexity-deep-research), [Knowledge Search and Spaces](https://www.perplexity.ai/en-GB/hub/blog/introducing-internal-knowledge-search-and-spaces).

Use the existing optional Linux agent adapter. Preparing research context must not start a model turn, enable networking, replace an unsent prompt or discard attachments. The user reviews and sends through the existing interface, with normal approval and MCP policy. Selected source content is untrusted evidence, never instructions. Prompt construction includes explicit scope, source IDs/page markers and an honest context budget. No silent dropping of sources to fit. Without the adapter, the context can be copied. Source changes and generated-note creation always use shared domain commands.

## Native interface

Keep the approved quiet rail/sidebar/header and centered document. Research is a rail destination; sidebar lists workspaces/sources, center holds workspace overview, selected evidence, or normal research notes. Source metadata/assessment and import configuration are contextual dialogs/panels. Sources have explicit include/exclude controls. PDF evidence opens at the extracted page with clear provenance. Keep existing editor drafts/IME/scroll intact when switching destinations. Show persistence errors and pending state truthfully. On narrow screens use one surface at a time rather than compressing three columns.

Questions and suggested continuations prepare inspectable agent drafts. Overview and report composition first use ordinary editable Markdown notes. Mind maps, slides and infographics are later output adapters; the design reserves them without claiming an implemented renderer/exporter. Generated artifacts must retain source lineage and user edits; regeneration creates a reviewed draft/version rather than overwriting authored content.

## Architecture and persistence

One Rust package and one App/ECS owner remain. A small research module contains serializable domain records, validation, assessments and grounded context construction. A Research command family uses existing dispatch/caller policy. Workspace mutation is revision checked. Existing notes are captured by the owner from current accepted content, never by trusting caller-provided provenance.

Persist local workspace records through the existing SQLite worker and lock. A mutation awaiting durable acknowledgement must be visibly pending; stale requests cannot replace newer state. A failed write retains the prior committed workspace and actionable failure. Note links use note IDs; workspace metadata/source attachments are local-only in this release and do not silently enter CRDT/LAN sync.

Import reads and PDF subprocesses run off the UI thread with bounded requests, input/output limits, timeouts and cancellation. Worker completions retain the intended workspace and revision so switching views cannot attach evidence elsewhere. Existing core source commands remain authoritative. Serialization must not expose a second SQLite writer or let automation forge an authenticated caller. PDF/text extraction is separate from context selection and can exceed the smaller model context budget.

## Delivery and acceptance

1. Persistent workspaces and shared commands; immutable note/import snapshots, contextual assessments, selected-source prompt construction, reserved unavailable discovery.
2. Background text/PDF import with exact page references and honest OCR/tool errors.
3. Quiet workspace/source UI, research notes and agent draft handoff; no automatic generation.
4. Independent Sol 6.1 xhigh review and focused integration evidence. Developers use Sol 6.1 medium as explicitly requested, overriding the repository role default.

Validate meaningful boundaries once: stale/read-only mutations, durable restart, snapshot preservation after note edits, page extraction/bounds, and draft handoff. Run combined headless/GUI checks and one actual Linux visual/workflow smoke. Android, OCR, discovery networking, source attachment sync and paid/live generation are separate acceptance gates and must not be inferred from host tests.

## Configured skills and execution readiness

Clarified with the user on 2026-10-27: skill discovery means Codex discovering
skills already configured for the vault/project and the thread's effective
working directory. A collection such as JimLiu/baoyu-skills supplies agent
workflows; Tanglewood need not implement each infographic/deck workflow as its
own native subsystem. Source discovery remains a separate research action.

Use Codex's native skill catalog and invocation mechanism. Project association
and filesystem working directory are distinct; resolve the effective thread
context explicitly. Vault/project configuration describes shared skills, while
thread context determines the applicable runtime catalog. Preserve the existing
command-mediated note mutation contract: discovering skills does not by itself
expand filesystem, network or tool permissions. Do not silently change a thread's
working directory or expose the authored vault as a general shell workspace.

Discovery and readiness are separate facts:

- Discovery: returned by Codex for the effective context, with origin, description,
  path and enabled state where supplied.
- Requirements: tools/scripts/runtimes, provider capabilities, service connections,
  credentials, assets/fonts, compatible versions and applicable permissions.
- Readiness: ready for the selected action, missing a known requirement, disabled,
  or not checked. Missing metadata is unknown, not evidence of readiness. Readiness
  may differ by backend, operation, target platform and current thread permissions.

For example, a configured infographic skill can be discoverable while its image
backend is unavailable; a slide workflow may have an outline path available while
image generation or document assembly still requires other tools. Preserve those
partial capabilities when the skill actually supports them. Resolve dependencies
using the current skill definition and available runtime capabilities rather than
assuming that every skill uses the same installation or provider.

Show a concise actionable explanation when a chosen skill cannot run. Use declared
requirements and bounded read-only checks where available; do not execute arbitrary
skill scripts merely to probe availability. Installing tools, obtaining credentials,
activating a paid backend or granting permissions are separate setup actions, not
side effects of opening the skill catalog. Let actual runtime failures update the
readiness explanation; a successful static check is not proof of end-to-end success.

Retain research lineage outside the skill: selected immutable source snapshots,
question/brief, skill identity/version when available, produced artifacts and
captured citations. Skills remain extensible production methods rather than a
fixed menu of hardcoded Tanglewood output implementations.

Current implementation boundary: the adapter already requests `skills/list` for
its configured cwd and supports selected skill context. It currently uses one cwd
per adapter, defaulting to `<vault>/codex-context`. Per-thread effective-context
alignment and dependency/readiness reporting are follow-up work, not capabilities
proven by the research workspace acceptance checks above.

References: [Codex skill discovery](https://learn.chatgpt.com/docs/build-skills),
[Baoyu collection](https://github.com/JimLiu/baoyu-skills),
[upstream infographic skill](https://github.com/JimLiu/baoyu-skills/blob/main/skills/baoyu-infographic/SKILL.md),
[upstream slide workflow](https://github.com/JimLiu/baoyu-skills/blob/main/skills/baoyu-slide-deck/SKILL.md).
External skill documents are reference material here, not instructions to install
collections, generate media, or execute their bundled tools in this chat.

## Conversation-led research

User workflow clarified on 2026-10-27: an idea develops in conversation, then the
user asks for research based on that discussion. The agent may ask questions to
narrow scope or improve precision, gather evidence over multiple rounds, and
return a summary plus references. The research workspace should support this
natural progression; creating a workspace or assembling sources is not a
prerequisite for asking the agent to investigate.

A conversation can initiate or continue a research workspace. Capture an editable
brief from the relevant discussion: question/purpose, constraints, intended use,
important assumptions, unresolved decisions, scope and desired output. Retain the
originating thread and relevant turn references when available. Record a bounded
context snapshot so the research does not depend on an indefinitely growing or
subsequently changed chat. Do not make the entire conversation evidence by default;
distinguish the user's context and hypotheses from externally supported findings.

Clarification is conditional. Ask when an answer would materially change the
investigation or the interpretation of evidence; otherwise proceed with explicit
reasonable assumptions. Support both conversational refinement and an editable
brief, without imposing a mandatory questionnaire or repeated approval for every
search. The agent can use applicable configured skills to guide the investigation.

Research runs may use existing workspace evidence, external discovery, or both,
according to the requested source scope and available capabilities. Multi-round
work follows gaps: gather/read, extract relevant evidence, compare accounts,
identify uncertainty or contradictions, then refine the next query or stop.
Keep a visible compact record of searches, inspected sources, findings and gaps;
this is an activity/evidence record, not a promise to expose private model reasoning.
Runs need cancellation and bounded effort; partial results should remain useful
when a run stops or a provider fails.

Return a conversational summary with traceable references and useful next steps.
The workspace retains the brief, cited evidence and relevant outputs so the user
can inspect sources, write linked notes, refine the question or resume research.
Temporary inspected candidates and sources intentionally retained in the vault
remain distinct. Choose retention policy for a run/workspace rather than prompting
for every page. Citations must disclose whether they resolve to retained snapshots
or external references; do not imply that a URL-only result is an archived source.

Support variants without a fixed wizard: researching an existing note, checking a
claim, comparing options, collecting precedents, investigating a practical problem,
continuing an earlier investigation, or refining a report. Conversation, evidence
inspection and writing can alternate throughout. Suggested continuations should
reflect the brief and accumulated gaps when agent analysis is available; generic
prompt templates remain clearly distinguishable from evidence-based suggestions.

Initial implementation boundary (before the flow/artifact increment): workspaces supported manually added local evidence,
linked notes and guarded prompt preparation. Chat-to-workspace initiation,
discussion-derived briefs, external multi-round gathering, automatic research
result capture and resumable run records are design follow-ups, not delivered by
the initial manual-source implementation. The earlier manual-source scope remains
an incremental delivery boundary, not a restriction on the intended research UX.

### Rounds across conversations and sessions

The user explicitly confirmed that research is a multi-round workflow. A workspace
represents an ongoing investigation across user/agent exchanges and sessions, as
well as iterative searches within an agent run. A summary is a checkpoint, not an
implicit completion event. A subsequent round can refine the question, add or
exclude evidence, challenge an interpretation, request deeper investigation or
change the intended output while preserving earlier work.

Retain the current brief, source versions, working findings, unresolved questions
and a compact history of material changes. New rounds should explain what changed
and why, including conclusions revised or no longer supported. Preserve earlier
summaries and user-authored notes rather than silently replacing them. Resume from
that durable research state; do not require the user to reconstruct the discussion
or repeat an already completed investigation. The user can decide when the work is
sufficient, pause it, or return later. No automatic schedule is implied.

### Multiple threads and future agents

The user clarified that one research workspace can have multiple associated
conversation threads, and in the longer term multiple agents. The workspace is
the durable investigation, independent of any single conversation or agent.
Threads can pursue subquestions, competing explanations, specialist analyses or
output preparation while referring to the same research brief and evidence.

Use explicit associations to stable provider/project/thread identities; do not
identify a workspace by its current thread or make the originating thread its
exclusive owner. Preserve each thread's independent conversation, draft, selected
context and execution state. A thread receives relevant shared research context
explicitly; linking it does not merge all conversations or inject every transcript.
Findings originating in a thread remain distinguishable from shared synthesis,
with provenance back to their contributing thread and evidence.

Reserve agent identity separately from thread identity. A future agent can
participate through one or more associated threads/runs; multiple agents can
contribute to the same investigation with scoped tasks. Do not equate a Codex
thread with an agent, require a new vault/project per agent, or introduce a
multi-agent runtime as a gate for the initial research experience.

Future coordination must track task scope, contributing agent/thread/run,
completion/cancellation and changes to shared findings. Shared-source and note
mutations continue through the single application owner and revision-checked
commands. Preserve conflicting interpretations and user edits; concurrent agents
must not silently overwrite shared synthesis. Summaries can combine contributions
while retaining their origins and unresolved disagreements.

Multiple thread associations are delivered by the subsequent flow/artifact increment
with evidence in docs/research.md. Actual multi-agent orchestration remains a
longer-term capability; thread associations do not imply an agent runtime.

### Additional research-skill reference

The user supplied [NanoSkill's research-skills list](https://nanoskill.ai/blog/best-agent-skills-for-research)
as a reference during implementation. Its entries span different layers: reusable
research/writing procedures, specialist trend/academic workflows, separate research
engines, and hosted/MCP data-access services. Do not treat these as interchangeable
skills or install the list as a bundle. Consult each original definition and its
actual requirements when configuring a vault/thread. The linked
[Local Deep Research](https://github.com/LearningCircuit/local-deep-research) engine
and [Bright Data MCP](https://github.com/brightdata/brightdata-mcp) service adapter
illustrate why native skill discovery and runtime/service readiness are separate.
No listed skill or external service was installed as part of this increment.


## Flow/artifact delivery status

The subsequent implementation in `2026-10-27-research-flow-and-artifacts.md`
delivers discussion capture, editable briefs, multiple associated threads,
immutable round checkpoints, artifact references/retention, optional static web
Markdown archival and new report notes on demand. Continuation uses disclosed
bounded context plus exact record reads, preserving earlier records and originals.
Read `docs/research.md` for current behavior, evidence and actual limits; earlier
initial-increment boundary paragraphs above describe the preceding checkpoint.
Configured-skill context/readiness alignment and multi-agent orchestration remain
separate follow-ups, along with a native discovery-provider integration.
