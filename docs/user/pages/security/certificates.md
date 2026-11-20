# Certificates

## Purpose

Certificates & Security — a PKI console for Zyvor Fabric: certificate authorities, issued certificates, the CSR approval queue, host TPM/boot attestation, and VM security baselines, rolled up into one health dashboard.

Six tabs: Dashboard, CAs, Certificates, Requests, Attestation, Security Baselines.

## When to use it

- To see which certificates are expiring soon and renew them before they lapse
- To stand up a new certificate authority (root, intermediate, or external) for issuing certs
- To approve certificate signing requests waiting in the queue
- To check whether hosts have a TPM present and are passing boot/secure-boot attestation
- To track fleet compliance against a VM security baseline
- Prefer this page when the job matches the purpose above

## How to get there

- Route / id: `/app/certificates`
- Nav: **Security → Certificates** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Dashboard** — totals for certs (active/expiring/expired), CAs, pending requests, compliance %, plus certs expiring within 30 days.
2. **CAs** — table + **Create CA** (name, Root/Intermediate/External, subject e.g. `CN=My CA, O=My Org`).
3. **Certificates** — issued certs; **Revoke** on active ones (confirmation).
4. **Requests** — CSR queue; approve pending with one click (no reject action in this view).
5. **Attestation** — per-host TPM present/version, attestation status, boot integrity, secure boot, last check.
6. **Security Baselines** — VM count, compliant count, checks, % bar; **Create Baseline** (name, description; default check attached).

Typical flow: Dashboard for expiring-soon → approve Requests → Create CA only when standing up PKI → check Attestation after host changes. Cross-check overall posture on Compliance / Security Dashboard.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Security Dashboard](security-dashboard.md)
- [Compliance](compliance.md)
- [Encryption](encryption.md)
- [Access Control](access-control.md)
- [Audit](../monitoring/audit.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
