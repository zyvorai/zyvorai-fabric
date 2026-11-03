# Encryption

## Purpose

Encryption — manage VM disk and vMotion encryption: register key management providers (KMIP, HashiCorp Vault Transit, or local software keys), define reusable encryption policies, and see which VMs are encrypted under which policy.

Providers → policies → encrypted VMs. Rotate keys on demand after suspected compromise.

## When to use it

- To connect an external KMS so Zyvor Fabric manages encryption keys outside the host
- To define a policy (algorithm, whether vMotion traffic is encrypted, key rotation schedule) and see who's using it
- To check which VMs are currently encrypted and under which policy
- To rotate a VM's encryption key on demand, e.g. after a suspected key compromise
- Prefer this page when the job matches the purpose above
- Before enabling encryption broadly, start with a Local provider in a non-prod window

## How to get there

- Route / id: `/app/encryption`
- Nav: **Security → Encryption** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Summary tiles (Key Providers, Policies, Encrypted VMs, Connected Providers), then three tabs:

1. **Key Providers** — name, type, endpoint, status. **Add Provider**: name, type (**KMIP**, **Local**, or **HashiCorp Vault Transit**), endpoint URL. Remove with trash + confirmation.
2. **Policies** — provider, algorithm, vMotion encryption, auto-rotate interval. **Create Policy**: name, description, provider dropdown, algorithm (**AES-256-XTS**, **AES-256-CBC**, or **ChaCha20-Poly1305**), Encrypt vMotion toggle, Auto-rotate toggle (+ days).
3. **Encrypted VMs** — encrypted yes/no, policy, algorithm, last rotation. **Rotate Key** on encrypted VMs (confirmation).

Typical flow: Add Provider → Create Policy → confirm VMs appear under Encrypted VMs → Rotate Key only when required. Merion endpoints as `https://<host>/…` (or your KMS URL) — not lab-specific IPs in shared docs.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Certificates](certificates.md)
- [Compliance](compliance.md)
- [Security Dashboard](security-dashboard.md)
- [Access Control](access-control.md)
- [Migrations](../operations/migrations.md)
- [Virtual Machines](../core/vms.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
