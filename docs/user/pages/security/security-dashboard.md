# Security Dashboard

## Purpose

Security — a real-time security posture and threat-monitoring view: an overall risk score, active security alerts, recent failed login attempts, and listening network ports on the host. Data auto-refreshes every 5 seconds.

Distinct from [Compliance](compliance.md) (configuration checks) and [Certificates](certificates.md) (PKI health) — this page is about **live threats and activity**, not point-in-time audits.

## When to use it

- To get a fast read on overall risk from the single risk-score gauge
- To see currently active security alerts by severity (critical/warning/info)
- To spot a brute-force or credential-stuffing attempt via the failed-logins table
- To audit which ports are open and listening on the host, and which process owns each one
- Prefer this page when the job matches the purpose above
- During on-call when you need a security pulse before opening Audit/Access Control

## How to get there

- Route / id: `/security-dashboard`
- Nav: **Security → Security Dashboard** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. View the risk score gauge (0–100, color-coded) alongside Critical Alerts, Warnings, and Failed Logins count tiles.
2. Review the Security Alerts list — severity badge, message, source, timestamp; "No active alerts" when clear.
3. Review the Failed Logins table (time, user, source) — only when there are failed attempts.
4. Review the Listening Ports table (port, protocol, process, PID) — only when ports are open.
5. Auto-refresh every 5 seconds; manual refresh in the header.

Typical flow: check risk score → read critical alerts → if failed logins spike, disable the user on [Access Control](access-control.md) and pull [Audit](../monitoring/audit.md). Unexpected listeners → correlate PID on [Processes](../monitoring/processes.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Access Control](access-control.md)
- [Compliance](compliance.md)
- [Certificates](certificates.md)
- [Encryption](encryption.md)
- [Audit](../monitoring/audit.md)
- [Alerts](../monitoring/alerts.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
