# Content Library

## Purpose

Content Library — a catalog of reusable provisioning building blocks: libraries of templates/ISOs/OVFs/scripts, guest customization specs (per-OS hostname/domain/DNS settings), and host compliance profiles.

Organize artifacts by **library** and path instead of scattering files across ad-hoc directories. Guest customization and host profiles live on the same page as separate tabs.

## When to use it

- To organize VM templates, ISOs, OVF packages, and scripts into named libraries instead of scattering them across storage paths
- To define a reusable guest customization spec (hostname prefix, domain, DNS servers) for Linux or Windows guests
- To check host compliance profile status — how many hosts are compliant vs. non-compliant against a profile
- When onboarding a team that needs a shared ISO/template catalog
- Alongside [Templates](templates.md) (resource presets) when you also need packaged media and guest identity specs

## How to get there

- Route / id: `/app/content-library`
- Nav: **Operations → Content Library** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

Summary tiles show total libraries, items, guest customization specs, and host profiles. Four tabs:

1. **Libraries** — cards with status, item count, and total size. **Create Library** sets name, optional description, **Local** or **Subscribed** type, and a required storage path. **Browse** jumps to Item Browser filtered to that library; trash deletes the library and all items (confirmation).
2. **Item Browser** — every item across libraries (template, ISO, OVF, script, or file) with type, version, size, last-updated. Filter by library dropdown or view all. Delete from the row (confirmation).
3. **Guest Customization** — specs (OS type, hostname prefix, domain, DNS). **Create Spec** sets name, Linux/Windows, hostname prefix, domain, and comma-separated DNS. Delete from the row (confirmation).
4. **Host Profiles** — compliant/non-compliant host counts and status. **Create Profile** sets name and optional description. Delete from the row (confirmation).

Typical flow: create a Local library with a storage path → add/browse items → create guest specs for naming/DNS → optionally track host profiles. For host patch baselines, also see [Lifecycle](lifecycle.md).

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Templates](templates.md)
- [ISO Images](../more-images-migrations-managers/iso-images.md)
- [Disk Images](../more-images-migrations-managers/disk-images.md)
- [Lifecycle](lifecycle.md)
- [Compliance](../security/compliance.md)
- [Create VM](../core/create.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
