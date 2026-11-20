# dependabot-alerts

The Dependabot alerts JSON in, a security triage view out: alert states, severities, ecosystems, the packages and manifests affected and the advisory summaries.

```bash
gh api repos/OWNER/REPO/dependabot/alerts > alerts.json
./scripts/merionctl deploy examples/merion-agents/dependabot-alerts --test
```

The field layout follows the real API response (`security_vulnerability.severity`, `dependency.package`), trimmed for the sample. Keep the upload under 200 KB; add `--paginate` only for small repositories. It reports what GitHub says; it is not a vulnerability scanner and does not tell you which alerts matter for your code.

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
