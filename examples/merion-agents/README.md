# Merion packaged agents

| Pack | Purpose |
|---|---|
| [`infra-ops`](./infra-ops/) | Alerts, VMs, lifecycle — reboot/remediate behind ask |
| [`migration-op`](./migration-op/) | Fabric migrations + GuestKit inspect/rescue |
| [`deploy-op`](./deploy-op/) | `/readyz` + `/health` readiness artifact |
| [`browser-research`](./browser-research/) | Allowlisted a11y browse → research markdown |
| [`pdf-brief`](./pdf-brief/) | One-click PDF → `brief.md` (no browser, 0 CONNECT) |
| [`invoice-check`](./invoice-check/) | A use case **you** define: `kind: usecase`, no code |
| [`inbox-digest`](./inbox-digest/) | A use case **you** define: drop in a plain-text mail export, get a digest (no code, no model, 0 CONNECT) |
| [`gmail-triage`](./gmail-triage/) | Lists unread mail with a read-only Gmail credential (per-person Google account) |
| [`mail-compose`](./mail-compose/) | Drafts or sends a plain-text mail; every use waits for a phone-signed approval that shows the real text |
| [`calendar-agent`](./calendar-agent/) | Reads your next day, or adds an event after a phone-signed approval |
| [`goal-planner`](./goal-planner/) | Proposes the steps of a goal from a model socket; the person accepts or rejects them |
| [`suggestion-example`](./suggestion-example/) | The smallest agent that proposes suggestions the person accepts (as a goal) or dismisses |
| [`calendar-suggestions`](./calendar-suggestions/) | Reads the next day of calendar and proposes prep reminders and missing-location suggestions, by plain rules — no model |
| [`price-watch`](./price-watch/) | Suggests a tracked item once it drops to its target price, through a plain API-key connector (no OAuth) |
| [`outlook-triage`](./outlook-triage/) | Lists unread Outlook mail with a read-only Graph credential (per-person Microsoft account) |
| [`outlook-compose`](./outlook-compose/) | Drafts or sends a plain-text Outlook mail; every use waits for a phone-signed approval that shows the real text |
| [`outlook-calendar`](./outlook-calendar/) | Reads your next day, or adds an event after a phone-signed approval (Microsoft emails the guests) |
| [`_fabric`](./_fabric/) | Shared client + credential/policy recipes |

```bash
./scripts/merion-pack-demo.sh infra-ops
./scripts/merion-demo.sh list                     # one-click use cases; merion-demo.sh <id>
./scripts/merionctl deploy examples/merion-agents/invoice-check --test   # deploy your own
```

Every pack has a `pack.json` (kind `usecase`, `agent` or `builtin`): see [docs/merion/PACKS.md](../../docs/merion/PACKS.md).

Goals API: [docs/merion/goals/README.md](../../docs/merion/goals/README.md).
