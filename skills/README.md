# Agent skills for Merion

Skills teach a coding agent (Claude Code, Codex, Cursor) to drive Merion without a source checkout. Each skill is a
folder with a `SKILL.md`. Copy a folder into your agent's skills directory (for Claude Code, `.claude/skills/`).

| Skill | Use it to |
|---|---|
| [`merion-cli`](merion-cli/SKILL.md) | Run use cases and inspect runs with `merionctl` |
| [`merion-policy`](merion-policy/SKILL.md) | Write, sign and load a `merion.policy.yaml` |
| [`merion-debug`](merion-debug/SKILL.md) | Work out why a cell was blocked or froze |

The skills only restate what is in `docs/merion/`. If a skill and the docs disagree, the docs win; fix the skill.
