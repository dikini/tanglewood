# Desktop UX and accessibility evidence — 2026-10-27

Scope: UX0–UX5 and UX9 from the saved plan. UX6–UX8 remain follow-ups. The
saved audit remains a qualified review, not certification. The completed priority
pass and evidence are included in the local commit requested by the user; no publication.

## Baseline and ownership

The initial aefc48c dirty snapshot is preserved under `/tmp/tanglewood-ux-baseline`.
During capture, another authorized task merged the reviewed responsive design
system. Overlapping writes were paused, then resumed after that task completed.
The relevant baseline is **fe40a2a**, archived with `git archive HEAD` under
`/tmp/tanglewood-ux-head-baseline`. This UX delta is relative to that commit.

PM owns manifest/lockfile, evidence/status and integration. A Sol low developer
owned the existing UI modules; an independent Sol xhigh reviewer reviewed frozen
changes. No new UI module/framework/service or application background loop.

## Priority behavior

- UX1: desktop-only eframe AccessKit, contextual names for fields, icon actions,
  inclusion checkboxes and source selection. Labels preserve native multiline,
  value and placeholder semantics; ordinary actions remain ordinary buttons.
- UX2: already delivered by fe40a2a; reuse [responsive evidence](../design-system/adoption.md)
  for pane budgets, modal pullouts, focus return, retained drafts and one-host
  responsive regression. Native current baseline shows the whole first message
  when scrolled to the top; bottom-sticking and viewport-edge clipping are normal.
  No speculative transcript fix or replacement layout helper was added.
- UX3: full selected-source title remains wrapping (already true in baseline),
  separate inclusion/text/original status, title discovery on hover/focus, quiet
  selection marker and distinct focus outline. One title tooltip when both hovered
  and focused. Create output… opens the existing format choices.
- UX4: only active draft gets a clipped accent gutter. Short source fields fit
  content; long fields retain a bounded viewport and adjacent reachable footer.
  Apply changes / Cancel changes and Export current Markdown… accurately distinguish
  applied source from durable saving. Unapplied edits are excluded from export.
- UX5: latest-context review retains drafts and does not retry/save. Recovery copy
  names workspace-level CAS changes accurately. Existing explicit import retry and
  source/revision/IME command guards remain unchanged.

Measured palette pairs (WCAG sRGB luminance): light secondary #616161 on selected
#dedede **4.60:1**, on sidebar #f2f2f2 **5.53:1**; dark secondary #aaaaaa on
selected #303030 **5.68:1**. Accent/selected **4.74:1** light, **6.26:1** dark.
Only light secondary gray102→97 changed; baseline secondary/selected was4.27:1.

## Focused checks

All Cargo jobs serialized with `RUSTC_WRAPPER=` and existing shared target.

| Command | Result |
| --- | --- |
| `cargo test --locked --offline --lib --features gui,codex ui::` |34 passed |
| `cargo build --locked --offline --features gui,codex --bin tanglewood --example research_demo --example transcript_demo` |passed |
| `cargo test --locked --offline --lib --features gui,codex accesskit_click_opens_exact_whole_document_source` |1 passed,105 filtered |
| `cargo clippy --locked --offline --all-targets --features gui,codex` |passed; existing warning debt remains |
| `cargo fmt --check`; `git diff --check` |passed |

The first combined run had33 passing tests and one obsolete fixed-height assertion.
The existing scenario now checks compact short drafts, genuinely long custom-fence
content in a zero-height parent, substantial bounded viewport and reachable footer.
The rerun passed34. Existing exact-range/Unicode/cancel/conflict, retained research
inputs and responsive single-host/draft tests were reused. Label/tooltip/marker
fixes received focused static/native delta checks and relevant builds, without
repeating broad suites. No headless domain adapter duplication or test infrastructure.

## Native evidence

Private Xvfb :94, private D-Bus, real eframe window, system AT-SPI GI and Orca with
in-memory settings. Disposable SQLite backup contains similarly named Unicode
sources, included reference-only material and excluded readable material. No live
agent/provider request was performed.

- [Conversation](desktop-ux-evidence/conversation.png): approved quiet shell,
  shorter output action and reachable composer.
- [Source reader](desktop-ux-evidence/source-reader.png): full identity, separate
  inclusion/text state, selected/focused row and one title tooltip.
- [Inline draft](desktop-ux-evidence/inline-editor.png): restrained active gutter,
  nearby Apply/Cancel, surrounding reader unchanged.
- [Long source](desktop-ux-evidence/document-editor.png): clipped marker follows
  visible field and ends before the reachable footer.
- [Note dialog](desktop-ux-evidence/note-dialog.png): native path field/action names.
- [Top transcript](desktop-ux-evidence/baseline-600-top.png) versus
  [scrolled transcript](desktop-ux-evidence/baseline-600-scrolled.png): normal
  scrolling distinguished from permanently hidden content at600×700.

The [native tree](desktop-ux-evidence/native-accessibility-tree.txt) exposes named
inclusion checkboxes with independent checked state, viewed-source pressed state,
text/reference labels, multiline composer and source entries, ordinary named
Send/Apply/Cancel/Copy buttons and a named new-note path field. Native source-block
activation works. [Orca generated focus speech](desktop-ux-evidence/orca-focus-sample.txt)
includes the full long source name, selected-state changes, Notes and dialog
controls. Generated output is not human auditory validation or full certification.
An initial AT-SPI cache warning did not prevent the sampled workflow.

One native boundary remains qualified: [fresh header result](desktop-ux-evidence/native-header-action.txt)
found exactly one current whole-source header, focus=True, advertised Click and
`do_action=True`, but no source field appeared in the sampled tree before further
input. Keyboard Enter then exposed the whole-source editor. The attempted one-shot
repaint did not establish a fix and was removed. Do not claim full native Action
compatibility. The focused raw-egui regression injects a Click at the fresh named
header node and passes: source mode opens with range=None, the exact full Unicode
Markdown draft and unchanged canonical bytes. This isolates the local source-mode
handler from the native adapter/render observation; no custom bridge workaround
was added. The keyboard-visible field is the whole-source editor, not a focused
block. Full native Action compatibility remains open.

Settled active-bridge idle observation: **0 CPU ticks over3 seconds**,100Hz clock.
No new per-frame I/O/parsing/process work or periodic repaint was added. This is
one observation, not a benchmark or latency guarantee.

## Matched size and accepted tradeoff

Same existing profiles/flags, clean fe40a2a archive versus task checkout,
`--locked --offline --features gui,codex --bin tanglewood`, shared target and
serialized builds. Only temporary binary copies are stripped with `strip`.
Both baselines were rebuilt; no stale binaries or raw debug metadata comparison.

| Profile | Baseline stripped | Final stripped | Delta |
| --- | --- | --- | --- |
| dev |43,528,848 bytes |51,287,984 bytes |+7,759,136 bytes (+17.825%) |
| release |20,399,816 bytes (19.455 MiB) |23,853,384 bytes (22.748 MiB) |+3,453,568 bytes (+3.294 MiB,+16.929%) |

The material dev increase justified one matched optimized comparison, requested
by the scope supervisor. No LTO/profile changes. Linux dependency identities296→340;
one is the local package path difference, leaving43 added dependency identities.
These are the existing eframe AccessKit/AT-SPI/D-Bus stack; lockfile also includes
other targets. Initial cache lacked these crates; an authorized fetch populated it.
The supervisor accepted the native accessibility cost, retaining the bridge without
optional switches or custom runtime complexity. Growth is disclosed, not zero-cost.

## Limits and deferred work

No adb/device runner is available. Android IME/native Back, actual device source
sizing and Android accessibility remain unverified; the bridge is desktop-only.
Shared revision/IME logic was preserved, host controller coverage reused. Recovery
command effects were inventoried and preservation coverage reused; no new interactive
stale-assessment/import-failure session was performed. UX6–UX8, PDF reflow, full
mobile design and complete screen-reader workflow certification remain deferred.
