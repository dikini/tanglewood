# Tanglewood agents for OpenAI Codex

`AGENTS.md` provides repository instructions. This directory holds the detailed role prompts and shared workflow; it is not automatically a custom-agent registry. Codex-loadable role definitions are in `.codex/agents/*.toml`, with project defaults in `.codex/config.toml`.

| Role | Model | Reasoning | Prompt |
| --- | --- | --- | --- |
| `pm` | `gpt-6.1-sol` | `medium` | [PM](roles/pm.md) |
| `developer` | `gpt-6.1-sol` | `low` | [Developer](roles/developer.md) |
| `reviewer` | `gpt-6.1-sol` | `xhigh` | [Reviewer](roles/reviewer.md) |

“Small” means low reasoning effort here, not a smaller model. The role TOML files use Codex's `developer_instructions` field to load these prompts. These are role instructions, not replacements for the host's system instructions, permissions, or tool policies. The reviewer's read-only behavior is an instruction; its custom agent file does not impose a separate filesystem sandbox.

## Start the goal

In Codex, open this repository, choose GPT 6.1 Sol / Medium for the root PM, and ask it to execute `docs/mvp-goal.md`. With the CLI, run from the repository root:

```sh
codex --model gpt-6.1-sol -c 'model_reasoning_effort="medium"' \
  'Act as the PM agent. Read and execute docs/mvp-goal.md, following AGENTS.md and .agents/roles/pm.md.'
```

Project settings require the repository to be trusted by Codex. Existing sessions may need reopening to discover newly added roles. Verify the selected model and reasoning in the host; repository config does not retroactively change a running conversation.

## Delegation

Prefer the named `developer` and `reviewer` custom agents. The root performs the PM role; the `pm` definition is also available for a host that explicitly delegates orchestration.

If the host exposes a different subagent tool, provide the role prompt and explicit model/effort when spawning. In a host where full-history forks inherit settings, use the fresh-context mode that supports overrides and pass repository path, task scope, contracts, and evidence. Do not assume a role name alone selects a model. If the requested configuration is unavailable, report the limitation before a substituted run.

The project cap is three open spawned agents, excluding the root. Aim for two developers plus one reviewer; close/reuse finished agents and honor lower host limits. Do not launch separate sidebar chats as a substitute for subagents. Use the [workflow](workflow.md) for ownership, review, integration, and unsigned commits.

## Configuration references

The files target the current standalone custom-agent TOML format. The installed CLI at preparation time was `codex-cli 0.163.0`. Official references: [custom agents](https://learn.chatgpt.com/docs/agent-configuration/subagents), [model reasoning settings](https://developers.openai.com/api/docs/models/gpt-6.1-sol). Validate against the actual host if its version differs; do not add legacy role mappings alongside standalone definitions by guesswork.
