# Agent skills for Keep

Skills teach a coding agent (Claude Code, Codex, Cursor) to drive Keep without a source checkout. Each skill is a
folder with a `SKILL.md`. Copy a folder into your agent's skills directory (for Claude Code, `.claude/skills/`).

| Skill | Use it to |
|---|---|
| [`keep-cli`](keep-cli/SKILL.md) | Run use cases and inspect runs with `merionctl` |
| [`keep-policy`](keep-policy/SKILL.md) | Write, sign and load a `keep.policy.yaml` |
| [`keep-debug`](keep-debug/SKILL.md) | Work out why a cell was blocked or froze |

The skills only restate what is in `docs/merion/`. If a skill and the docs disagree, the docs win; fix the skill.
