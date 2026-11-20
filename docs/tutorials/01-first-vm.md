# Tutorial 01: Your First VM

Create, start, connect to, and tear down a virtual machine using the Zyvor Fabric
REST API. By the end of this tutorial you will understand the core VM lifecycle
and be ready to explore more advanced features.

**Level:** Beginner
**Time:** 30 minutes
**Prerequisites:** Zyvor Fabric running, `curl`, `jq`, KVM-capable host

---

## What You Will Learn

1. How to download a cloud image from the built-in catalog
2. How to create a VM definition via the API
3. How to start the VM with custom options
4. How to connect to the VM console over WebSocket
5. How to configure cloud-init for first-boot customization
6. How to stop, restart, and delete a VM

---

## Setup

Set your environment variables. Every command in this tutorial uses them.

```bash
export FABRIC_HOST="http://localhost:3000"

# Authenticate (replace credentials with your own)
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')

echo "Token: $TOKEN"
```

Verify the connection:

```bash
curl -sk "$FABRIC_HOST/readyz" | jq .
curl -s "$FABRIC_HOST/api/vms" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected `/readyz` includes `"ok": true` when the local store and FluxVM are ready.
Expected VM list (empty on a fresh install):

```json
{
  "items": [],
  "total": 0,
  "offset": 0,
  "limit": 200
}
```

---

## Step 1: Download a Cloud Image

Zyvor Fabric includes a catalog of well-known cloud images. List what is available:

```bash
curl -s "$FABRIC_HOST/api/images/cloud" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "name": "ubuntu-24.04",
    "distro": "ubuntu",
    "version": "24.04",
    "url": "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img",
    "format": "qcow2",
    "arch": "amd64"
  },
  {
    "name": "fedora-41",
    "distro": "fedora",
    "version": "41",
    "url": "https://download.fedoraproject.org/pub/fedora/linux/releases/41/Cloud/x86_64/images/Fedora-Cloud-Base-Generic-41-1.4.x86_64.qcow2",
    "format": "qcow2",
    "arch": "x86_64"
  },
  {
    "name": "debian-12",
    "distro": "debian",
    "version": "12",
    "url": "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2",
    "format": "qcow2",
    "arch": "amd64"
  },
  {
    "name": "alma-9",
    "distro": "almalinux",
    "version": "9",
    "url": "https://repo.almalinux.org/almalinux/9/cloud/x86_64/images/AlmaLinux-9-GenericCloud-latest.x86_64.qcow2",
    "format": "qcow2",
    "arch": "x86_64"
  }
]
```

Download the Fedora 41 cloud image. This runs in the background and returns
immediately:

```bash
curl -s -X POST "$FABRIC_HOST/api/images/cloud/download" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "fedora-41"
  }' | jq .
```

Expected response:

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "name": "fedora-41",
  "state": "pending",
  "output_path": null,
  "error": null,
  "started": "2026-04-12T10:00:00Z",
  "completed": null
}
```

Check the download progress:

```bash
curl -s "$FABRIC_HOST/api/images/downloads" \
  -H "Authorization: Bearer $TOKEN" | jq '.[] | select(.name == "fedora-41")'
```

Wait until `state` is `"completed"`. The image is saved to
`/var/lib/zyvor-fabricd/images/fedora-41.qcow2`.

You can also verify it appears in the image list:

```bash
curl -s "$FABRIC_HOST/api/images/list" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "name": "fedora-41",
    "path": "/var/lib/zyvor-fabricd/images/fedora-41.qcow2",
    "format": "qcow2",
    "size_bytes": 456789012
  }
]
```

> **Tip:** You can also provide a custom URL to download any image:
> ```json
> {
>   "name": "my-custom-image",
>   "url": "https://example.com/my-image.qcow2"
> }
> ```

---

## Step 2: Create a VM

Create a VM definition. This registers the VM in Zyvor Fabric but does not start it.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-first-vm",
    "image": "fedora-41",
    "cpus": 2,
    "memory": 2048,
    "disk": 20,
    "hostname": "my-first-vm",
    "tenant": "tutorial",
    "tags": ["tutorial", "fedora"],
    "labels": {
      "env": "dev",
      "tutorial": "01"
    }
  }' | jq .
```

`tenant` is stored as `labels.tenant` and passed to FluxVM on start. Filter later with
`GET /api/vms?tenant=tutorial`.

Expected response:

```json
{
  "name": "my-first-vm",
  "state": "stopped",
  "cpus": 2,
  "memory": 2048,
  "disk": 20,
  "image": "fedora-41",
  "hostname": "my-first-vm",
  "tags": ["tutorial", "fedora"],
  "labels": {
    "env": "dev",
    "tutorial": "01",
    "tenant": "tutorial"
  },
  "created": "2026-04-12T10:05:00Z"
}
```

### Understanding the Parameters

| Parameter  | Type     | Description                              | Constraints            |
|-----------|----------|------------------------------------------|------------------------|
| `name`    | string   | Unique VM identifier                     | Alphanumeric, hyphens, underscores; max 64 chars |
| `image`   | string   | Disk image name or path                  | Must not be empty      |
| `cpus`    | integer  | Number of virtual CPUs                   | 1 -- 256               |
| `memory`  | integer  | RAM in megabytes                         | 128 -- 1,048,576 (1 TB)|
| `disk`    | integer  | Disk size in gigabytes                   | 1 -- 65,536 (64 TB)    |
| `hostname`| string   | Guest hostname (optional)                |                        |
| `tags`    | string[] | Freeform tags for filtering (optional)   |                        |
| `labels`  | object   | Key-value labels for policy matching     |                        |

### Verify the VM Exists

```bash
curl -s "$FABRIC_HOST/api/vms/my-first-vm" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### List All VMs

The list endpoint supports pagination:

```bash
# First page of 10 results
curl -s "$FABRIC_HOST/api/vms?offset=0&limit=10" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Step 3: Start the VM

Start the VM with default options (no request body needed):

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/start" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "status": "starting"
}
```

The start operation is asynchronous. The API returns `202 Accepted` immediately
and the VM transitions through `starting` to `running`. Poll the VM status:

```bash
curl -s "$FABRIC_HOST/api/vms/my-first-vm" \
  -H "Authorization: Bearer $TOKEN" | jq '.state'
```

Wait until the state is `"running"`.

### Starting with Custom Options

You can pass `VMStartOptions` in the request body for fine-grained control:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "tpm": true,
    "vsock": true,
    "console": "interactive",
    "network_tap": true,
    "pass_ssh_key": true,
    "ssh_key_type": "ed25519",
    "credentials": [
      {
        "id": "passwd.hashed-password.root",
        "value": "$y$j9T$salt$hash"
      }
    ]
  }' | jq .
```

See [Tutorial 04](04-advanced-vm-options.md) for the full VMStartOptions
reference.

### Check VM Metrics

Once the VM is running, you can view its resource usage:

```bash
curl -s "$FABRIC_HOST/api/vms/my-first-vm/metrics" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "cpu_usage": 5.2,
  "memory_usage": 524288000,
  "disk_usage": 1073741824,
  "network_rx": 12345,
  "network_tx": 6789
}
```

---

## Step 4: Connect via WebSocket Console

Zyvor Fabric provides a WebSocket endpoint for interactive console access. You can
connect using any WebSocket client. Here is an example using `websocat`:

```bash
# Install websocat if needed
# cargo install websocat
# or: dnf install websocat / apt install websocat

websocat "ws://localhost:3000/api/vms/my-first-vm/console" \
  -H "Authorization: Bearer $TOKEN"
```

This gives you a live terminal session inside the VM. Press `Ctrl+C` to
disconnect from the console (the VM keeps running).

### Console via the Web UI

If you have the React web UI running, navigate to:

```
http://localhost:3000/vms/my-first-vm/console
```

The web UI provides a full xterm.js terminal embedded in the browser.

---

## Step 5: Configure Cloud-Init

Cloud-init lets you customize the VM on first boot: set users, install packages,
run commands, and configure networking.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/cloud-init" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "my-first-vm",
    "users": [
      {
        "name": "tutorial-user",
        "ssh_authorized_keys": [
          "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample user@workstation"
        ],
        "sudo": "ALL=(ALL) NOPASSWD:ALL",
        "shell": "/bin/bash"
      }
    ],
    "packages": [
      "vim",
      "htop",
      "curl"
    ],
    "runcmd": [
      "echo 'Hello from cloud-init!' > /root/hello.txt",
      "systemctl enable --now cockpit.socket"
    ],
    "write_files": [
      {
        "path": "/etc/motd",
        "content": "Welcome to my-first-vm, managed by Zyvor Fabric!\n"
      }
    ]
  }' | jq .
```

Expected response:

```json
{
  "status": "created",
  "iso_path": "/var/lib/zyvor-fabricd/cloud-init/my-first-vm-cloud-init.iso"
}
```

The cloud-init ISO is generated and can be attached to the VM. Restart the VM
for cloud-init to take effect:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/restart" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "status": "restarted"
}
```

> **Note:** Cloud-init typically runs only on the first boot. If you need to
> re-run it, you may need to clean the cloud-init state inside the VM first
> (`cloud-init clean`).

---

## Step 6: VM Lifecycle Operations

### Pause and Resume

Pause the VM (freezes execution but keeps state in memory):

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/pause" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

```json
{
  "status": "paused"
}
```

Resume the VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/resume" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

```json
{
  "status": "running"
}
```

### Clone a VM

Create an exact copy of a VM. The VM must be stopped for linked clones:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/clone" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "target_name": "my-first-vm-clone",
    "linked_clone": false
  }' | jq .
```

Expected response:

```json
{
  "name": "my-first-vm-clone",
  "state": "stopped",
  "cpus": 2,
  "memory": 2048,
  "disk": 20,
  "image": "fedora-41",
  "created": "2026-04-12T10:20:00Z"
}
```

A **full clone** (`linked_clone: false`) copies the entire disk image using
copy-on-write reflinks when the filesystem supports it.

A **linked clone** (`linked_clone: true`) creates a QCOW2 overlay backed by the
source image. It is much faster and uses less disk space, but the source VM must
be stopped.

### Stop the VM

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/my-first-vm/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

```json
{
  "status": "stopped"
}
```

### Delete the VM

Deleting a VM requires Admin privileges and permanently removes it:

```bash
curl -s -X DELETE "$FABRIC_HOST/api/vms/my-first-vm" \
  -H "Authorization: Bearer $TOKEN"

# Returns 204 No Content on success
```

Clean up the clone too:

```bash
curl -s -X DELETE "$FABRIC_HOST/api/vms/my-first-vm-clone" \
  -H "Authorization: Bearer $TOKEN"
```

---

## Step 7: Building Images with mkosi

Instead of downloading pre-built cloud images, you can build custom images using
mkosi (make operating system image):

```bash
curl -s -X POST "$FABRIC_HOST/api/images/build" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "custom-fedora",
    "distribution": "fedora",
    "packages": ["vim", "htop", "nginx", "python3"],
    "autologin": true
  }' | jq .
```

Expected response:

```json
{
  "id": "b2c3d4e5-f6a7-8901-bcde-f23456789012",
  "name": "custom-fedora",
  "distribution": "fedora",
  "state": "pending",
  "output_path": null,
  "error": null,
  "started": "2026-04-12T10:25:00Z",
  "completed": null
}
```

Supported distributions: `fedora`, `ubuntu`, `debian`, `centos`, `arch`,
`opensuse`, `alma`, `rocky`.

Monitor the build:

```bash
curl -s "$FABRIC_HOST/api/images/builds" \
  -H "Authorization: Bearer $TOKEN" | jq '.[] | select(.name == "custom-fedora")'
```

---

## Importing Existing Images

You can import VM images from other hypervisors (VMDK, VDI, VHD) by converting
them to QCOW2:

```bash
curl -s -X POST "$FABRIC_HOST/api/images/import" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "source_path": "/tmp/exported-vm.vmdk",
    "name": "imported-vm",
    "target_format": "qcow2",
    "cpus": 4,
    "memory": 4096
  }' | jq .
```

Expected response:

```json
{
  "vm_name": "imported-vm",
  "image_path": "/var/lib/zyvor-fabricd/images/imported-vm.qcow2",
  "source_format": "vmdk",
  "target_format": "qcow2",
  "size_bytes": 2147483648
}
```

---

## VM States Reference

| State      | Description                                  |
|-----------|----------------------------------------------|
| `stopped`  | VM is defined but not running                |
| `starting` | VM is in the process of booting              |
| `running`  | VM is fully operational                      |
| `paused`   | VM execution is frozen; state is in memory   |
| `stopping` | VM is shutting down                          |
| `failed`   | VM failed to start or crashed                |
| `unknown`  | State cannot be determined                   |

---

## Troubleshooting

### VM stuck in "starting" state

Check for errors on the VM object:

```bash
curl -s "$FABRIC_HOST/api/vms/my-first-vm" \
  -H "Authorization: Bearer $TOKEN" | jq '.last_error'
```

Common causes:
- The disk image path does not exist
- KVM is not available (`/dev/kvm` missing)
- Insufficient disk space

### "VM with this name already exists"

VM names must be unique. Either delete the existing VM first or choose a
different name.

### Permission denied (403)

- Creating VMs requires **Write** (`user` or `admin` role)
- Deleting VMs requires **Admin** role
- Read-only users can list and view VMs but not modify them

---

## Next Steps

- [Tutorial 02: VM Networking](02-networking.md) -- Set up bridges, VLANs, and network policies
- [Tutorial 03: Snapshots & Backups](03-snapshots-backups.md) -- Protect your VMs with point-in-time snapshots
- [Tutorial 04: Advanced VM Configuration](04-advanced-vm-options.md) -- TPM, SecureBoot, hotplug, and more
