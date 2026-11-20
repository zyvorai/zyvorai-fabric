# Sessions

## Purpose

Inspect and control agent-runtime sessions (`/api/sessions`); hibernate, resume, cancel, or delete. 503 unless `[agent_runtime]` is configured.

## When to use it

- Monitor session status / sandbox id / errors
- Hibernate, resume, cancel, or delete a session

## How to get there

- List: `/app/sessions` — **Operations → Sessions**
- Detail: `/app/sessions/:id` (from Agents → Run session, or a list row)

## What you can do

| Action | API |
|--------|-----|
| List | `GET /api/sessions` |
| Detail | `GET /api/sessions/{id}` |
| Hibernate / resume / cancel | `POST /api/sessions/{id}/{action}` |
| Delete | `DELETE /api/sessions/{id}` |

## Related

- [Agents](agents.md)
- [How users interact](../../../tutorials/14-interacting-with-agents.md)
- [Agent Runtime Quickstart](../../../tutorials/11-agent-runtime-quickstart.md)
- [Page index](../../PAGE_INDEX.md)
