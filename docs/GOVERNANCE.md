# Repository governance

Notes for the canonical repo [`zyvorai/fabric`](https://github.com/zyvorai/fabric).

## Branch protection

**Currently off.** There is no active GitHub ruleset or classic branch
protection on `main`. Direct pushes and PR merges are allowed without
required reviews or required status checks.

CI workflows still run for signal; they do not block merge.

If you later want protection again, Settings → Rules → Rulesets (or
`POST /repos/{owner}/{repo}/rulesets`). Keep a maintainer bypass actor
so owners can always merge.

```bash
gh api repos/zyvorai/fabric/rulesets --jq '.[].name'
```

## CODEOWNERS

See [`.github/CODEOWNERS`](../.github/CODEOWNERS). Code owners are
advisory only while branch protection is disabled.

## Releases

- Prefer annotated tags (`vX.Y.Z`) from `main` after green CI when practical.
- Product release workflow (`.github/workflows/release.yml`) builds
  binaries, SBOM, Helm chart, and publishes a GitHub Release.
