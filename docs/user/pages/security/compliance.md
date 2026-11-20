# Compliance

## Purpose

Compliance Dashboard — a security and configuration compliance scorecard: an overall score, pass/warning/fail counts by category, and remediation guidance for anything that isn't passing.

Point-in-time configuration checks. For live threats see [Security Dashboard](security-dashboard.md); for patch baselines see [Lifecycle](../operations/lifecycle.md).

## When to use it

- To get a single number for how compliant your deployment currently is
- To find exactly which checks are failing or warning, and how to fix each one
- To trigger a fresh compliance scan on demand instead of waiting for the next one
- To narrow the check list down to one category, e.g. only network or only auth checks
- Prefer this page when the job matches the purpose above
- Before an audit, export mental notes from Fail/Fix lines and remediate

## How to get there

- Route / id: `/app/compliance`
- Nav: **Security → Compliance** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. View the compliance score (0–100, green/amber/red) plus Passed, Warnings, Failed, and Total Checks tiles.
2. Click **Run Scan** — button reads "Scanning…" then refreshes results when complete.
3. Filter checks by category pills (**all**, plus each category from the last scan).
4. Review each check's name, status (pass/warning/fail), description; non-passing rows show a "Fix:" remediation note.
5. "Last scan" timestamp at the bottom shows when data was collected.

Typical flow: Run Scan → filter to Failed → apply Fix guidance on the owning page → Run Scan again until score recovers. Pair with Certificates/Access Control for PKI and account hygiene.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Security Dashboard](security-dashboard.md)
- [Certificates](certificates.md)
- [Access Control](access-control.md)
- [Encryption](encryption.md)
- [Lifecycle](../operations/lifecycle.md)
- [Audit](../monitoring/audit.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
