# Publishing to the Terraform Registry

Provider type: **`zyvor-fabricd`**  
Registry namespace: **`zyvorai/zyvor-fabricd`**  
Product: **Zyvor Fabric** (`zyvor-fabricd` daemon)

## Prerequisites

1. [HashiCorp Terraform Registry](https://registry.terraform.io/) account linked to GitHub `zyvorai`
2. Public [`zyvorai/fabric`](https://github.com/zyvorai/fabric) repository
3. GPG key (optional; recommended for signed checksums in production releases)

## Release flow

Tags use the prefix `terraform-provider/`:

```bash
git tag terraform-provider/v0.1.0
git push origin terraform-provider/v0.1.0
```

GitHub Actions (`.github/workflows/terraform-provider-release.yml`) runs GoReleaser and uploads:

- Multi-platform `terraform-provider-zyvor-fabricd` binaries
- Terraform Registry provider manifest (`registry.terraform.io/zyvorai/zyvor-fabricd`)

## Local install (development)

```bash
cd terraform-provider
make tidy build install
```

## Consumer configuration

```hcl
terraform {
  required_providers {
    zyvor-fabricd = {
      source  = "zyvorai/zyvor-fabricd"
      version = "~> 0.1"
    }
  }
}

provider "zyvor-fabricd" {
  endpoint = "https://fabric.example.com:9095"
  token    = var.fabric_token
}
```

## Future alias: `zyvorai/fabric`

The provider **type name** stays `zyvor-fabricd` for compatibility. A second registry namespace can mirror the same binaries once published:

```hcl
source = "zyvorai/fabric"  # planned mirror
```

## Troubleshooting releases

### GitHub Actions billing

If workflows show *"recent account payments have failed or your spending limit needs to be increased"*, fix billing at https://github.com/settings/billing then re-run the failed workflow or re-push the tag:

```bash
git tag -d terraform-provider/v0.1.0
git push origin :refs/tags/terraform-provider/v0.1.0
git tag terraform-provider/v0.1.0
git push origin terraform-provider/v0.1.0
```

### Local dry-run (no registry upload)

```bash
cd terraform-provider
goreleaser release --snapshot --clean
ls -la dist/
```

### Acceptance smoke (live daemon)

After `scripts/ci-api-audit.sh` or with a running `zyvor-fabricd`:

```bash
chmod +x scripts/acceptance-smoke.sh
ZYVOR_FABRICD_ADMIN_PASSWORD=... ./scripts/acceptance-smoke.sh
```

