# Explain

## Purpose

Explain — plain-language, AI-generated explanations for a chosen system metric: its current value and trend, an assessment of its status, what's contributing to it, and what to do about it. This is the interpretive layer on top of the raw numbers you'd see in Analytics or Debug Tools.

Nothing loads until you pick a metric. The page never mutates the system — it only explains and recommends.

## When to use it

- When a metric looks off and you want a written explanation of what's driving it, not just the raw number
- To get concrete recommendations for a specific resource (CPU, memory, disk, or network) instead of interpreting a chart yourself
- Before escalating an issue, to see whether the system already has a plausible explanation and fix
- After spotting a spike on [Live Metrics](live-metrics.md) or a flag in [Analytics](analytics.md)
- When sharing a short status narrative with someone who will not dig through charts

## How to get there

- Route / id: `/explain`
- Nav: **Monitoring → Explain** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Pick a metric** — CPU, Memory, Disk, or Network. Selecting one fetches its explanation and last-hour timeseries.
2. **Current Value & Status** — current value, trend arrow (up/down/flat), status badge (e.g. normal, elevated, critical), short summary.
3. **Last Hour chart** — bar chart of samples; hover for exact value and time.
4. **Contributing Factors** — named factors with impact badge (high/medium/low) and short description.
5. **Recommendations** — checklist of suggested actions when the backend provides any.

Typical flow: pick the hot metric → read status + factors → follow recommendations manually on Virtual Machines / System / storage pages. Cross-check raw numbers on Analytics or Debug Tools before making invasive changes.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Analytics](analytics.md)
- [Live Metrics](live-metrics.md)
- [Debug Tools](debug.md)
- [Optimizer](resource-optimizer.md)
- [Capacity](capacity-planning.md)
- [Alerts](alerts.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
