# Tutorial 04: Advanced VM Configuration

Fine-tune VM behavior with VMStartOptions, CPU and memory hotplug, online disk
resize, cloud-init customization, bind mounts, and credentials. `VMStartOptions`'
shape traces back to `systemd-vmspawn`'s own CLI options; VM lifecycle itself
is handled by [FluxVM](https://github.com/zyvorai/fluxvm), which
interprets this same request shape rather than shelling out to systemd-vmspawn.

> **Note:** `VMStartOptions` is only honored for a VM's first launch, not
> replayed on every restart. A handful of options have no FluxVM equivalent
> yet and are rejected with a clear error rather than silently ignored: TPM,
> SecureBoot, VSOCK passthrough, extra drives, systemd credentials, and
> SMBIOS injection. Bind mounts *are* supported -- each entry becomes a
> virtiofs share, auto-mounted in the guest via a generated cloud-init entry
> (see [Step 8: Bind Mounts](#step-8-bind-mounts) below). Everything else in this tutorial
> (CPU/memory, networking mode, kernel/initrd/firmware, extra kernel args)
> works as described.

**Level:** Intermediate
**Time:** 40 minutes
**Prerequisites:** Completed [Tutorial 01](01-first-vm.md)

---

## What You Will Learn

1. The complete VMStartOptions schema
2. TPM, SecureBoot, and VSOCK configuration
3. CPU and memory hotplug on a running VM
4. Online and offline disk resize
5. Cloud-init customization for automated provisioning
6. Bind mounts and credential injection
7. Resource control via systemd properties

---

## Setup

```bash
export FABRIC_HOST="http://localhost:3000"
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')
```

Create a test VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "advanced-demo",
    "image": "fedora-41",
    "cpus": 2,
    "memory": 2048,
    "disk": 20
  }' | jq .
```

---

## Step 1: VMStartOptions Reference

When you start a VM, you can pass a JSON body with any combination of these
options. All fields are optional -- omitted fields use auto-detected defaults.

### Full Example

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "scope": "system",
    "kvm": true,
    "secure_boot": true,
    "tpm": true,
    "tpm_state": "auto",
    "vsock": true,
    "vsock_cid": 42,
    "console": "interactive",
    "network_tap": true,
    "network_user_mode": false,
    "pass_ssh_key": true,
    "ssh_key_type": "ed25519",
    "discard_disk": true,
    "grow_image": "40G",
    "notify_ready": true,
    "register": true,
    "slice": "vm.slice",
    "properties": [
      "MemoryMax=4G",
      "CPUQuota=200%",
      "TasksMax=4096"
    ],
    "bind_mounts": [
      {
        "source": "/host/shared-data",
        "destination": "/mnt/shared",
        "read_only": true
      }
    ],
    "credentials": [
      {
        "id": "passwd.hashed-password.root",
        "value": "$y$j9T$saltsalt$hashedpasswordhere"
      }
    ],
    "load_credentials": [
      {
        "id": "ssh.authorized_keys.root",
        "path": "/root/.ssh/authorized_keys"
      }
    ],
    "forward_journal": "/var/log/journal/vm-advanced-demo",
    "quiet": false
  }' | jq .
```

### VMStartOptions Field Reference

#### Manager Scope

| Field   | Type   | Description                                    |
|--------|--------|------------------------------------------------|
| `scope`| string | `"system"` or `"user"` -- which manager to use |

#### Virtualization Hardware

| Field           | Type    | Description                                     |
|----------------|---------|--------------------------------------------------|
| `kvm`          | bool    | Enable KVM hardware acceleration (auto-detected) |
| `secure_boot`  | bool    | Enable UEFI Secure Boot firmware                 |
| `tpm`          | bool    | Enable TPM 2.0 emulation via swtpm              |
| `tpm_state`    | string  | TPM state path, `"auto"`, or `"off"`             |
| `vsock`        | bool    | Enable VSOCK host-guest communication            |
| `vsock_cid`    | integer | Specific VSOCK CID (auto-assigned if omitted)    |
| `firmware`     | string  | Custom firmware file path (e.g., OVMF)           |

#### Networking

| Field              | Type | Description                       |
|-------------------|------|-----------------------------------|
| `network_tap`     | bool | Create TAP device for bridged networking |
| `network_user_mode`| bool| Use QEMU user-mode networking (SLIRP) |

#### Disk and Image

| Field         | Type   | Description                              |
|--------------|--------|------------------------------------------|
| `directory`  | string | Boot from a directory instead of an image|
| `discard_disk`| bool  | Process TRIM/discard requests from the VM|
| `grow_image` | string | Grow the disk image to this size (e.g., `"50G"`) |
| `extra_drives`| string[] | Additional disk images or block devices|

#### Boot Options

| Field        | Type     | Description                             |
|-------------|----------|-----------------------------------------|
| `linux`     | string   | Kernel image path for direct kernel boot|
| `initrd`    | string[] | Initrd paths (merged if multiple)       |
| `extra_args`| string[] | Extra kernel command-line arguments      |

#### Console and Output

| Field       | Type   | Description                               |
|------------|--------|-------------------------------------------|
| `console`  | string | `"interactive"`, `"read-only"`, `"native"`, or `"gui"` |
| `background`| string| Terminal background color (ANSI SGR code) |
| `quiet`    | bool   | Suppress Zyvor Fabric status output            |

#### Identity

| Field   | Type   | Description                            |
|--------|--------|----------------------------------------|
| `uuid` | string | Machine UUID (standard UUID format)    |

#### Legacy systemd Integration fields (no-op)

These fields are historical systemd-vmspawn / systemd-machined options. Fabric
no longer uses machined — they are accepted for request-shape compatibility
but silently ignored. Prefer FluxVM / Virtual Machines APIs.

| Field            | Type     | Description                             |
|-----------------|----------|-----------------------------------------|
| `slice`         | string   | Ignored (was systemd slice)             |
| `properties`    | string[] | Ignored (was unit properties)           |
| `register`      | bool     | Ignored (was systemd-machined register) |
| `forward_journal`| string  | Ignored                                 |
| `pass_ssh_key`  | bool     | Ignored                                 |
| `ssh_key_type`  | string   | Ignored                                 |
| `notify_ready`  | bool     | Ignored                                 |

#### User Namespacing

| Field          | Type   | Description                              |
|---------------|--------|------------------------------------------|
| `private_users`| string| User namespace mapping (`"yes"`, `"no"`, `"identity"`, `"pick"`, or `UID:COUNT`) |

#### Bind Mounts

| Field         | Type     | Description                             |
|--------------|----------|-----------------------------------------|
| `bind_mounts`| object[] | Host-to-guest bind mounts               |
| `bind_users` | string[] | Bind host users into the VM             |
| `bind_user_shell`| string| Shell for bound users                  |
| `bind_user_groups`| string[]| Auxiliary groups for bound users      |

#### Credentials

| Field             | Type     | Description                           |
|------------------|----------|---------------------------------------|
| `credentials`    | object[] | Inline credentials (`{id, value}`)    |
| `load_credentials`| object[]| File-based credentials (`{id, path}`) |
| `smbios11`       | string[] | SMBIOS Type 11 vendor strings         |

---

## Step 2: TPM and Secure Boot

> **Not currently supported.** `tpm`/`secure_boot`/`vsock` have no equivalent
> in the current VM driver yet -- a request setting any of them is rejected
> with a clear error. The rest of this section is kept for reference on the
> request shape.

Enable hardware security features for VMs that require measured boot or
disk encryption (e.g., BitLocker, LUKS with TPM binding):

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "tpm": true,
    "tpm_state": "auto",
    "secure_boot": true,
    "firmware": "/usr/share/edk2/ovmf/OVMF_CODE.secboot.fd"
  }' | jq .
```

The `tpm_state` field controls where TPM persistent state is stored:
- `"auto"` -- Zyvor Fabric picks a directory automatically
- `"off"` -- disable TPM regardless of the `tpm` flag
- A path (e.g., `"/var/lib/zyvor-fabricd/tpm/my-vm"`) -- explicit directory

### VSOCK Communication

VSOCK provides a high-performance socket interface between host and guest,
useful for agent communication without network configuration:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "vsock": true,
    "vsock_cid": 100
  }' | jq .
```

Inside the guest, connect to the host on CID 2:
```bash
# Guest side
socat - VSOCK-CONNECT:2:1234
```

---

## Step 3: CPU Hotplug

Add vCPUs to a running VM without downtime. This uses the QEMU Machine Protocol
(QMP) to activate pre-configured but unrealized CPU slots.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/hotplug/cpu" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "count": 2
  }' | jq .
```

Expected response:

```json
{
  "status": "ok",
  "added": 2,
  "total_cpus": 4
}
```

> **Note:** CPU hotplug requires QMP socket access. The maximum number of
> hotpluggable CPUs depends on the QEMU machine type and initial configuration.

---

## Step 4: Memory Hotplug

Add memory to a running VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/hotplug/memory" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "size_mb": 1024
  }' | jq .
```

Expected response:

```json
{
  "status": "ok",
  "added_mb": 1024,
  "total_memory_mb": 3072
}
```

The guest kernel must support memory hotplug (most modern Linux kernels do).
After adding memory, verify inside the guest:

```bash
free -h
```

---

## Step 5: Disk Hotplug

Attach additional disks to a running VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/hotplug/disk" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/var/lib/zyvor-fabricd/images/extra-data.qcow2",
    "bus": "virtio"
  }' | jq .
```

Expected response:

```json
{
  "status": "ok",
  "device_id": "virtio-disk-1"
}
```

### NIC Hotplug

Add a network interface to a running VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/hotplug/nic" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "bridge": "vm-bridge",
    "model": "virtio-net"
  }' | jq .
```

---

## Step 6: Disk Resize

Resize a VM's primary disk image. This can be done offline (VM stopped) or
online (VM running with QMP).

### Offline Resize

```bash
# Stop the VM first
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .

# Resize to 50GB
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/disk/resize" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "size": "50G",
    "online": false
  }' | jq .
```

Expected response:

```json
{
  "status": "resized",
  "vm": "advanced-demo",
  "new_size": "50G"
}
```

### Online Resize

Grow the disk while the VM is running. The guest must support online resize
(e.g., via `growpart` and `resize2fs`):

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/disk/resize" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "size": "100G",
    "online": true
  }' | jq .
```

Expected response:

```json
{
  "status": "resized",
  "vm": "advanced-demo",
  "new_size": "100G"
}
```

After an online resize, run these commands inside the guest to expand the
filesystem:

```bash
# For ext4 on /dev/vda1
growpart /dev/vda 1
resize2fs /dev/vda1

# For XFS
growpart /dev/vda 1
xfs_growfs /
```

> **Note:** Disk resize only grows images. Shrinking is not supported because it
> risks data loss.

---

## Step 7: Cloud-Init Customization

Cloud-init runs on first boot and configures the VM automatically. Zyvor Fabric
generates a cloud-init ISO that is attached to the VM.

### Full Cloud-Init Example

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/cloud-init" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "advanced-demo",
    "users": [
      {
        "name": "deploy",
        "groups": "wheel,docker",
        "ssh_authorized_keys": [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample deploy@ci-server"
        ],
        "sudo": "ALL=(ALL) NOPASSWD:ALL",
        "shell": "/bin/bash"
      },
      {
        "name": "monitoring",
        "ssh_authorized_keys": [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample monitoring@ops"
        ],
        "sudo": false,
        "shell": "/bin/bash"
      }
    ],
    "packages": [
      "vim", "htop", "tmux", "curl", "jq",
      "docker-ce", "docker-compose-plugin",
      "prometheus-node-exporter"
    ],
    "write_files": [
      {
        "path": "/etc/sysctl.d/99-vm-tuning.conf",
        "content": "vm.swappiness=10\nnet.core.somaxconn=65535\n"
      },
      {
        "path": "/etc/docker/daemon.json",
        "content": "{\"storage-driver\": \"overlay2\", \"log-driver\": \"journald\"}\n"
      }
    ],
    "runcmd": [
      "systemctl enable --now docker",
      "systemctl enable --now prometheus-node-exporter",
      "sysctl --system",
      "echo 'Provisioning complete' > /var/log/cloud-init-done"
    ]
  }' | jq .
```

Expected response:

```json
{
  "status": "created",
  "iso_path": "/var/lib/zyvor-fabricd/cloud-init/advanced-demo-cloud-init.iso"
}
```

---

## Step 8: Bind Mounts

Share host directories with the VM. Bind mounts are set through VMStartOptions,
declared at VM-create time (there's no way to add one to an already-running
VM). Each entry becomes a virtiofs share; the guest auto-mounts it via a
generated cloud-init entry written into `/etc/fstab`, so it also survives a
later stop/start.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "bind_mounts": [
      {
        "source": "/srv/shared-data",
        "destination": "/mnt/shared",
        "read_only": true
      },
      {
        "source": "/var/log/vm-logs",
        "destination": "/var/log/host",
        "read_only": false
      }
    ]
  }' | jq .
```

### Bind Mount Validation

- The `source` path must be absolute and must not contain `..` (path traversal)
- The `destination` defaults to the same as `source` if omitted
- `read_only: true` prevents the VM from modifying host files

---

## Step 9: Credential Injection

> **Not currently supported.** `credentials`/`load_credentials` rely on
> systemd-vmspawn's own SMBIOS credential-injection mechanism, which has no
> equivalent in the current VM driver -- a request using either field is
> rejected with a clear error rather than silently ignored. Use `cloud_init`
> (e.g. its `write_files`/`runcmd` fields) to get secrets into a VM instead.
> The rest of this section is kept for reference on the request shape.

Pass secrets to the VM securely using systemd credentials. The VM receives them
via SMBIOS or VSOCK without exposing them on the command line.

### Inline Credentials

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "credentials": [
      {
        "id": "passwd.hashed-password.root",
        "value": "$y$j9T$saltsalt$hashedpasswordhere"
      },
      {
        "id": "app.database-url",
        "value": "postgresql://user:pass@db-host:5432/myapp"
      },
      {
        "id": "app.api-key",
        "value": "sk-live-abc123def456"
      }
    ]
  }' | jq .
```

Inside the VM, credentials are available via:
```bash
systemd-creds cat app.database-url
```

### File-Based Credentials

Load credentials from files on the host:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "load_credentials": [
      {
        "id": "ssh.authorized_keys.root",
        "path": "/root/.ssh/authorized_keys"
      },
      {
        "id": "tls.certificate",
        "path": "/etc/ssl/certs/vm-cert.pem"
      }
    ]
  }' | jq .
```

### Credential ID Rules

- Must be alphanumeric with dots, hyphens, and underscores only
- Must not contain colons (`:`), slashes (`/`), or control characters
- Maximum value length: 64 KB

---

## Step 10: Resource Control

Use systemd properties to limit VM resource consumption. These are set as
VMStartOptions and applied to the VM's scope unit.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "slice": "vm.slice",
    "properties": [
      "MemoryMax=8G",
      "MemoryHigh=6G",
      "CPUQuota=400%",
      "CPUWeight=100",
      "IOWeight=200",
      "TasksMax=8192",
      "LimitNOFILE=65536",
      "Description=Advanced Demo VM"
    ]
  }' | jq .
```

### Allowed Property Prefixes

Only resource-control and informational properties are permitted. The API
rejects properties that could compromise host security.

| Category | Allowed Prefixes                                      |
|----------|------------------------------------------------------|
| Memory   | `MemoryMax=`, `MemoryMin=`, `MemoryHigh=`, `MemoryLow=`, `MemorySwapMax=` |
| CPU      | `CPUQuota=`, `CPUWeight=`, `CPUShares=`, `AllowedCPUs=` |
| I/O      | `IOWeight=`, `IOReadBandwidthMax=`, `IOWriteBandwidthMax=` |
| Tasks    | `TasksMax=`                                          |
| Network  | `IPAddressAllow=`, `IPAddressDeny=`                  |
| Limits   | `LimitNOFILE=`, `LimitNPROC=`, `LimitMEMLOCK=`      |
| Info     | `Description=`                                       |

Properties like `ExecStartPost=`, `DeviceAllow=`, or `Delegate=` are blocked
to prevent privilege escalation.

---

## Step 11: Direct Kernel Boot

Boot a VM directly from a kernel image, bypassing the bootloader. Useful for
testing custom kernels or embedded systems:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "linux": "/boot/vmlinuz-6.8.0-custom",
    "initrd": ["/boot/initramfs-6.8.0-custom.img"],
    "extra_args": ["enforcing=0", "console=ttyS0"]
  }' | jq .
```

### Extra Arguments Validation

- Must not start with `-` (prevents flag injection into Zyvor Fabric)
- Must not contain control characters

---

## Step 12: User Binding

Bind host user accounts into the VM so they can log in with their host
credentials:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "bind_users": ["developer", "operator"],
    "bind_user_shell": "/bin/bash",
    "bind_user_groups": ["wheel", "docker"]
  }' | jq .
```

### Bind User Restrictions

- System users (`root`, `daemon`, `bin`, `nobody`, etc.) are blocked
- Numeric UIDs below 1000 are blocked
- Usernames must be alphanumeric with hyphens, underscores, and dots

---

## Step 13: SPICE Display Configuration

SPICE (Simple Protocol for Independent Computing Environments) provides
high-performance remote access to VM graphical consoles. Use it for Windows VMs,
desktop Linux, or any workload that requires a GUI.

### Start a VM with SPICE

Set `console` to `"gui"` in VMStartOptions to enable SPICE:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "console": "gui"
  }' | jq .
```

### Connect to the SPICE Console

Use `remote-viewer` or `virt-viewer` to connect:

```bash
# Connect with remote-viewer
remote-viewer spice://your-host:5900

# Or use virt-viewer
virt-viewer --connect spice://your-host:5900
```

### Console Mode Reference

| Mode          | Description                                         |
|--------------|-----------------------------------------------------|
| `interactive` | Serial console attached to the terminal (default)   |
| `read-only`   | Read-only serial console output                     |
| `native`      | QEMU native console                                 |
| `gui`         | Graphical display via SPICE                          |

---

## Step 14: USB Passthrough

Pass host USB devices directly into a VM. This is useful for hardware security
keys, USB storage, serial adapters, and specialized peripherals.

### List Available USB Devices

```bash
curl -s "$FABRIC_HOST/api/system/usb" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "bus": 1,
    "device": 4,
    "vendor_id": "1050",
    "product_id": "0407",
    "description": "Yubico YubiKey OTP+FIDO+CCID"
  },
  {
    "bus": 2,
    "device": 2,
    "vendor_id": "0781",
    "product_id": "5583",
    "description": "SanDisk Ultra Fit"
  }
]
```

### Pass a USB Device to a VM

Use the vendor ID and product ID to pass the device at start time:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "extra_args": [
      "usb_host=1050:0407"
    ]
  }' | jq .
```

Inside the guest, the device appears as a native USB peripheral:

```bash
# Verify the device is visible in the guest
lsusb
```

> **Note:** The USB device is exclusively attached to the VM. It will not be
> available on the host while the VM is running.

---

## Step 15: OVA / OVF Export

Export a stopped VM to OVA (Open Virtual Appliance) format for portability.
OVA files can be imported into VMware, VirtualBox, and other platforms.

### Export a VM

```bash
# Stop the VM first
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .

# Export to OVA
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/export" \
  -H "Authorization: Bearer $TOKEN" \
  -o advanced-demo.ova
```

The exported OVA file contains:

- The VM disk image (converted to VMDK for compatibility)
- An OVF descriptor with hardware configuration
- A manifest file with checksums

### Use Cases

| Scenario                     | Description                                      |
|-----------------------------|--------------------------------------------------|
| VMware migration            | Export from Zyvor Fabric, import into vSphere          |
| Disaster recovery           | Archive VM images to offline storage              |
| Template distribution       | Share VM templates across air-gapped sites        |
| Cross-platform testing      | Run the same VM on different hypervisors          |

> **Note:** The VM must be stopped before exporting. Running VMs cannot be
> exported to ensure disk consistency.

---

## Cleanup

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/advanced-demo/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .

curl -s -X DELETE "$FABRIC_HOST/api/vms/advanced-demo" \
  -H "Authorization: Bearer $TOKEN"
```

---

## Next Steps

- [Tutorial 05: Multi-Node Clustering](05-clustering.md) -- Distribute VMs across multiple hosts
- [Tutorial 06: Security Hardening](06-security-hardening.md) -- Secure your VMs with encryption and firewalls
- [Tutorial 07: Logging and Compliance](07-logging-compliance.md) -- Query logs, scan for compliance, manage secrets
