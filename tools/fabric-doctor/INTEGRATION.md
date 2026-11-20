# Integrating this feature into `zyvorai/fabric`

This ZIP is a repository overlay. Copy/unzip its contents at the root of the Fabric repository.

It adds only new paths and does **not** replace the Rust daemon or existing CI:

```text
tools/fabric-doctor/
.github/workflows/fabric-doctor.yml
docs/FABRIC_DOCTOR.md
```

Recommended follow-up edits to the main repository (kept out of the overlay to avoid merge conflicts with a fast-moving `main`):

1. Add `Fabric Doctor` to the README's interfaces/tools section.
2. Add a top-level Makefile target that runs `$(MAKE) -C tools/fabric-doctor check`.
3. Package `fabric-doctor` next to `fabricctl` in release artifacts.
4. Later expose the report via `fabricctl doctor` and `/api/system/readiness` by reusing the stable JSON schema.

The implementation is intentionally standalone and standard-library-only, so it can land without adding dependencies to the Rust workspace or changing Fabric's runtime behavior.
