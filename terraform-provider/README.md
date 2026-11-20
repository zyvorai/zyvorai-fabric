# Terraform Provider for Zyvor Fabric

Provision and manage Zyvor Fabric virtual machines using HashiCorp Terraform. The provider type is **`zyvor-fabricd`** (stable); registry namespace is **`zyvorai/zyvor-fabricd`**.

> A future registry alias `zyvorai/fabric` may be published without breaking existing `zyvor-fabricd` provider blocks.

## Installation

```hcl
terraform {
  required_providers {
    zyvor-fabricd = {
      source  = "zyvorai/zyvor-fabricd"
      version = "~> 0.1"
    }
  }
}
```

## Provider Configuration

```hcl
provider "zyvor-fabricd" {
  endpoint = "http://localhost:9095"
  # token  = var.zyvor_fabricd_token    # Required when auth is enabled
}
```

| Argument | Required | Default | Description |
|----------|----------|---------|-------------|
| `endpoint` | Yes | -- | Zyvor Fabric API URL (`zyvor-fabricd`) |
| `token` | No | -- | JWT or API key for authentication |

## Resources

### zyvor-fabricd_vm

Manages a virtual machine through its full lifecycle (create, update, delete).

```hcl
resource "zyvor-fabricd_vm" "web_server" {
  name   = "web-server"
  image  = "/var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2"
  cpus   = 4
  memory = 4096

  cloud_init = {
    user_data = <<-EOF
      #cloud-config
      packages:
        - nginx
        - qemu-guest-agent
      runcmd:
        - systemctl enable --now nginx
        - systemctl start qemu-guest-agent
    EOF
  }

  tpm = {
    enabled = true
    version = "2.0"
  }

  vnc = {
    enabled = true
  }
}
```

#### Argument Reference

| Argument | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `name` | string | Yes | -- | VM name (must be unique) |
| `image` | string | Yes | -- | Path to VM disk image |
| `cpus` | number | No | 2 | Number of vCPUs |
| `memory` | number | No | 2048 | Memory in MB |
| `cloud_init` | object | No | -- | cloud-init configuration block |
| `tpm` | object | No | -- | TPM configuration block |
| `vnc` | object | No | -- | VNC configuration block |

#### Attribute Reference

| Attribute | Description |
|-----------|-------------|
| `state` | Current VM state (running, stopped, paused) |
| `ip_address` | Primary IP address of the VM |

## Data Sources

### zyvor-fabricd_vm

Read information about an existing VM.

```hcl
data "zyvor-fabricd_vm" "existing" {
  name = "my-vm"
}

output "vm_state" {
  value = data.zyvor-fabricd_vm.existing.state
}

output "vm_ip" {
  value = data.zyvor-fabricd_vm.existing.ip_address
}
```

## Multi-VM Example

```hcl
variable "app_servers" {
  default = ["app-1", "app-2", "app-3"]
}

resource "zyvor-fabricd_vm" "app" {
  for_each = toset(var.app_servers)

  name   = each.key
  image  = "/var/lib/zyvor-fabricd/images/ubuntu-22.04.qcow2"
  cpus   = 2
  memory = 4096

  cloud_init = {
    user_data = templatefile("cloud-init.yaml.tpl", {
      hostname = each.key
    })
  }
}
```

## Development

### Build the Provider

```bash
cd terraform-provider
make tidy build
# or: go build -o terraform-provider-zyvor-fabricd .
```

Example configuration: [examples/basic/main.tf](examples/basic/main.tf)

### Install Locally

```bash
mkdir -p ~/.terraform.d/plugins/zyvorai/zyvor-fabricd/0.1.0/linux_amd64
cp terraform-provider-zyvor-fabricd ~/.terraform.d/plugins/zyvorai/zyvor-fabricd/0.1.0/linux_amd64/
```

### Run Tests

```bash
go test ./...
```

## Registry release

See [REGISTRY.md](REGISTRY.md). Tag with:

```bash
git tag terraform-provider/v0.1.0 && git push origin terraform-provider/v0.1.0
```

## Registry migration (planned)

When `zyvorai/fabric` is published to the Terraform Registry:

```hcl
terraform {
  required_providers {
    zyvor-fabricd = {
      source  = "zyvorai/fabric"
      version = "~> 0.1"
    }
  }
}
```

Provider type name `zyvor-fabricd` and resource names remain unchanged — only the `source` address changes.
