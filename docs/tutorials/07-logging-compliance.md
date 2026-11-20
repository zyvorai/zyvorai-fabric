# Tutorial 07: Logging, Compliance, and Secrets Management

Centralize VM log collection, run automated compliance scans, and manage secrets
securely. This tutorial covers the full observability and governance workflow
available through the Zyvor Fabric API.

**Level:** Intermediate
**Time:** 35 minutes
**Prerequisites:** Completed [Tutorial 01](01-first-vm.md), Zyvor Fabric running with at least one VM

---

## What You Will Learn

1. Query VM and system journal logs with priority filtering
2. Forward VM journals to the host for centralized collection
3. List compliance profiles and scan VMs against security baselines
4. Review compliance scan results
5. Create, list, and delete secrets with encrypted storage
6. Inject secrets into VMs at boot time

---

## Prerequisites

- Zyvor Fabric running on the host
- At least one VM created (this tutorial uses `web-01`)
- Admin credentials for API access

---

## Setup

```bash
export FABRIC_HOST="http://localhost:3000"
TOKEN=$(curl -s "$FABRIC_HOST/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "your-password"}' | jq -r '.token')
```

If you do not have a test VM, create one:

```bash
curl -s -X POST "$FABRIC_HOST/api/vms" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "web-01",
    "image": "fedora-41",
    "cpus": 2,
    "memory": 2048,
    "disk": 20
  }' | jq .
```

---

## Part 1: Log Aggregation

Zyvor Fabric provides two log query endpoints that read from the systemd journal.
Logs are returned as structured JSON entries with timestamps, messages, and
priority levels.

### Query VM Logs

Retrieve recent journal entries for a specific VM:

```bash
curl -s "$FABRIC_HOST/api/vms/web-01/logs?lines=20" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
{
  "vm": "web-01",
  "entries": [
    {
      "timestamp": "2026-04-12T10:30:00Z",
      "message": "Started web-01.service",
      "priority": "6",
      "unit": "web-01.service"
    },
    {
      "timestamp": "2026-04-12T10:30:01Z",
      "message": "VM boot complete",
      "priority": "6",
      "unit": "web-01.service"
    }
  ]
}
```

### Filter by Priority

The `priority` parameter filters entries by syslog priority level. Only entries
at the specified level or more severe are returned.

| Priority | Name      | Description           |
|---------|-----------|-----------------------|
| 0       | Emergency | System is unusable    |
| 1       | Alert     | Immediate action needed |
| 2       | Critical  | Critical conditions   |
| 3       | Error     | Error conditions      |
| 4       | Warning   | Warning conditions    |
| 5       | Notice    | Normal but significant |
| 6       | Info      | Informational         |
| 7       | Debug     | Debug-level messages  |

```bash
# Show only errors and above (priority 0-3)
curl -s "$FABRIC_HOST/api/vms/web-01/logs?priority=3" \
  -H "Authorization: Bearer $TOKEN" | jq .

# Show warnings and above
curl -s "$FABRIC_HOST/api/vms/web-01/logs?priority=4&lines=50" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Query System Logs

Retrieve host-level journal entries (not VM-specific):

```bash
curl -s "$FABRIC_HOST/api/logs?lines=30" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Filter system logs by priority:

```bash
# System errors only
curl -s "$FABRIC_HOST/api/logs?priority=3&lines=100" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Forward VM Journal to Host

When starting a VM, use the `forward_journal` option to copy the VM's journal
entries to a directory on the host. This enables centralized log collection
without installing agents inside the VM.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/web-01/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "forward_journal": "/var/log/journal/vm-web-01"
  }' | jq .
```

After the VM starts, its journal entries are forwarded to the specified host
directory. You can then query them with standard `journalctl` or ship them to
your SIEM.

### Log Query Parameters

| Parameter | Type    | Description                           |
|----------|---------|---------------------------------------|
| `lines`  | integer | Number of recent entries to return    |
| `priority` | integer | Maximum priority level (0-7)       |

---

## Part 2: Compliance Scanning

Zyvor Fabric includes a built-in compliance scanning framework that checks VMs
against security profiles. Each profile contains a set of rules with severity
levels.

### List Compliance Profiles

```bash
curl -s "$FABRIC_HOST/api/compliance/profiles" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Expected response:

```json
[
  {
    "id": "cis-baseline-v1",
    "name": "CIS Baseline v1",
    "description": "Default security baseline with 7 checks",
    "rules": [
      {
        "id": "disk-encrypted",
        "name": "Disk Encryption",
        "category": "Security",
        "severity": "Critical",
        "check_type": "DiskEncrypted"
      },
      {
        "id": "tpm-enabled",
        "name": "TPM 2.0 Enabled",
        "category": "Security",
        "severity": "High",
        "check_type": "TpmEnabled"
      }
    ]
  }
]
```

The default `cis-baseline-v1` profile is always available and includes these
7 checks:

| Check                  | Category | Severity |
|-----------------------|----------|----------|
| DiskEncrypted         | Security | Critical |
| TpmEnabled            | Security | High     |
| SecureBootEnabled     | Security | High     |
| FirewallAssigned      | Network  | High     |
| NetworkPolicyAssigned | Network  | Medium   |
| MinCpus               | Compute  | Low      |
| MinMemoryMb           | Compute  | Low      |

### Scan a VM

Run a compliance scan against a specific VM:

```bash
curl -s -X POST "$FABRIC_HOST/api/compliance/scan/web-01" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"profile_id": "cis-baseline-v1"}' | jq .
```

Expected response:

```json
{
  "id": "scan-a1b2c3d4",
  "profile_id": "cis-baseline-v1",
  "vm_name": "web-01",
  "scan_time": "2026-04-12T11:00:00Z",
  "checks": [
    {"rule": "DiskEncrypted", "pass": false, "severity": "Critical"},
    {"rule": "TpmEnabled", "pass": false, "severity": "High"},
    {"rule": "SecureBootEnabled", "pass": false, "severity": "High"},
    {"rule": "FirewallAssigned", "pass": false, "severity": "High"},
    {"rule": "NetworkPolicyAssigned", "pass": false, "severity": "Medium"},
    {"rule": "MinCpus", "pass": true, "severity": "Low"},
    {"rule": "MinMemoryMb", "pass": true, "severity": "Low"}
  ]
}
```

### Review All Scan Results

List all historical compliance scan results:

```bash
curl -s "$FABRIC_HOST/api/compliance/results" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Interpreting Results

- **pass: true** -- The VM meets the requirement
- **pass: false** -- The VM fails the check and needs remediation
- Address `Critical` and `High` severity failures first
- Use the check type to determine the remediation action (e.g., enable TPM,
  assign a firewall profile, enable disk encryption)

---

## Part 3: Secrets Management

Zyvor Fabric provides centralized secret storage with encryption at rest. Secrets
are managed through a dedicated API and can be injected into VMs at boot time
using systemd credentials.

### Create a Secret

```bash
curl -s -X POST "$FABRIC_HOST/api/secrets" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "db-password",
    "value": "s3cretP@ssw0rd!",
    "metadata": {
      "env": "production",
      "service": "postgresql"
    }
  }' | jq .
```

Expected response:

```json
{
  "id": "sec-abc12345",
  "name": "db-password",
  "created": "2026-04-12T11:10:00Z",
  "updated": null,
  "metadata": {
    "env": "production",
    "service": "postgresql"
  }
}
```

> **Note:** The `value` field is never returned in API responses. Secrets are
> encrypted at rest and only accessible to Admin users.

### List Secrets

```bash
curl -s "$FABRIC_HOST/api/secrets" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

Values are always redacted in the response:

```json
[
  {
    "id": "sec-abc12345",
    "name": "db-password",
    "created": "2026-04-12T11:10:00Z",
    "updated": null,
    "metadata": {"env": "production", "service": "postgresql"}
  }
]
```

### Get a Specific Secret

```bash
curl -s "$FABRIC_HOST/api/secrets/sec-abc12345" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Use Secrets in a VM

Inject secrets into a VM at boot time using the `credentials` field in
VMStartOptions. The secrets are delivered via SMBIOS or VSOCK -- never on the
command line.

```bash
curl -s -X POST "$FABRIC_HOST/api/vms/web-01/start" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "kvm": true,
    "credentials": [
      {
        "id": "app.database-url",
        "value": "postgresql://user:s3cretP@ssw0rd!@db:5432/myapp"
      },
      {
        "id": "app.api-key",
        "value": "sk-live-abc123def456"
      }
    ]
  }' | jq .
```

Inside the guest, read the credentials:

```bash
# Read a credential by ID
systemd-creds cat app.database-url

# List available credentials
systemd-creds list
```

### Rotate a Secret

To rotate a secret, delete the old one and create a new one with the same name.
Then restart VMs that use the credential to pick up the new value.

```bash
# Delete old secret
curl -s -X DELETE "$FABRIC_HOST/api/secrets/sec-abc12345" \
  -H "Authorization: Bearer $TOKEN"

# Create new secret with updated value
curl -s -X POST "$FABRIC_HOST/api/secrets" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "db-password",
    "value": "n3wR0tatedP@ss!",
    "metadata": {
      "env": "production",
      "service": "postgresql",
      "rotated": "2026-04-12"
    }
  }' | jq .

# Restart VMs to pick up the new credential
curl -s -X POST "$FABRIC_HOST/api/vms/web-01/restart" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

### Delete a Secret

```bash
curl -s -X DELETE "$FABRIC_HOST/api/secrets/sec-abc12345" \
  -H "Authorization: Bearer $TOKEN"

# Returns:
# {"message": "Secret deleted successfully"}
```

---

## Cleanup

```bash
# Delete test secrets
curl -s -X DELETE "$FABRIC_HOST/api/secrets/$SECRET_ID" \
  -H "Authorization: Bearer $TOKEN"

# Stop the test VM
curl -s -X POST "$FABRIC_HOST/api/vms/web-01/stop" \
  -H "Authorization: Bearer $TOKEN" | jq .
```

---

## Next Steps

- [Tutorial 04: Advanced VM Options](04-advanced-vm-options.md) -- SPICE display, USB passthrough, OVA export
- [Tutorial 05: Multi-Node Clustering](05-clustering.md) -- Distribute VMs across multiple hosts
- [Tutorial 06: Security Hardening](06-security-hardening.md) -- Firewall profiles, encryption, and network isolation
