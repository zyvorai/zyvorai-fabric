# Compliance Scanning

How to scan VMs against security compliance profiles, interpret results, and automate compliance checks across your fleet.

## Table of Contents

- [Overview](#overview)
- [Available Profiles](#available-profiles)
- [Scanning a VM](#scanning-a-vm)
- [Viewing Results](#viewing-results)
- [Interpreting Results](#interpreting-results)
- [Automating Scans](#automating-scans)

---

## Overview

Zyvor Fabric includes a built-in compliance scanning engine that evaluates VMs against industry-standard security benchmarks. Scans connect to the target VM, run a series of configuration checks, and produce a pass/fail report for each rule in the profile.

Compliance scanning helps you:

- Validate that VMs meet your organization's security baseline before deployment.
- Detect configuration drift over time.
- Produce audit evidence for regulatory requirements.
- Identify and remediate security weaknesses.

---

## Available Profiles

List the compliance profiles available on your Zyvor Fabric installation:

```bash
curl -s http://localhost:3000/api/compliance/profiles \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Response:**

```json
[
  {
    "id": "cis-level1",
    "name": "CIS Level 1 Baseline",
    "description": "Center for Internet Security Level 1 benchmark",
    "check_count": 85
  },
  {
    "id": "cis-level2",
    "name": "CIS Level 2 Hardened",
    "description": "Center for Internet Security Level 2 benchmark (stricter)",
    "check_count": 142
  }
]
```

### Profile Details

| Profile | Description | Use Case |
|---------|-------------|----------|
| **CIS Level 1** | Essential security settings that can be configured without significant impact on services | General-purpose VMs, development environments |
| **CIS Level 2** | Stricter hardening rules that may reduce functionality | Production workloads, security-sensitive environments |

---

## Scanning a VM

To scan a VM, send a POST request with the target profile ID. The VM must be in the `running` state.

```bash
curl -s -X POST http://localhost:3000/api/compliance/scan/web-server \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"profile_id": "cis-level1"}' | jq
```

**Response (202 Accepted):**

```json
{
  "scan_id": "550e8400-e29b-41d4-a716-446655440000",
  "vm_name": "web-server",
  "profile_id": "cis-level1",
  "status": "running",
  "started": "2026-04-12T10:00:00Z"
}
```

The scan runs asynchronously. Use the scan ID or the results endpoint to check progress.

### Scan Prerequisites

- The VM must be **running**. Stopped VMs cannot be scanned.
- The scan connects to the VM via the vsock guest agent's shell interface. The VM must have been created with the guest agent enabled.
- Only one scan can run against a given VM at a time.

---

## Viewing Results

### List All Results

```bash
curl -s http://localhost:3000/api/compliance/results \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Response:**

```json
[
  {
    "scan_id": "550e8400-e29b-41d4-a716-446655440000",
    "vm_name": "web-server",
    "profile_id": "cis-level1",
    "status": "completed",
    "passed": 72,
    "failed": 8,
    "skipped": 5,
    "score": 90.0,
    "started": "2026-04-12T10:00:00Z",
    "completed": "2026-04-12T10:05:00Z"
  }
]
```

---

## Interpreting Results

### Score Calculation

The compliance score is calculated as:

```
score = (passed / (passed + failed)) * 100
```

Skipped checks are excluded from the score. Checks are skipped when a prerequisite is not met (e.g., a service is not installed, so its configuration checks are irrelevant).

### Result Categories

| Field | Meaning |
|-------|---------|
| `passed` | Checks where the VM meets the profile requirement |
| `failed` | Checks where the VM does not meet the requirement (action needed) |
| `skipped` | Checks that could not be evaluated (prerequisite not met) |
| `score` | Percentage of applicable checks that passed |

### Recommended Score Thresholds

| Score | Assessment | Action |
|-------|------------|--------|
| 95-100% | Excellent | No action required |
| 85-94% | Good | Review failed checks, prioritize critical items |
| 70-84% | Needs improvement | Address failures before production deployment |
| Below 70% | Poor | VM should not be deployed until remediated |

---

## Automating Scans

### Scheduled Scanning with Cron

Create a script that scans all running VMs on a regular schedule:

```bash
#!/bin/bash
# compliance-scan.sh -- Run via cron daily
HOST="http://localhost:3000"
TOKEN="$(cat /etc/zyvor-fabricd/api-token)"
AUTH="Authorization: Bearer $TOKEN"
PROFILE="cis-level1"

# Get all running VMs
VMS=$(curl -s "$HOST/api/vms" -H "$AUTH" | jq -r '.items[] | select(.state == "running") | .name')

for vm in $VMS; do
  echo "Scanning $vm against $PROFILE..."
  curl -s -X POST "$HOST/api/compliance/scan/$vm" \
    -H "$AUTH" \
    -H "Content-Type: application/json" \
    -d "{\"profile_id\": \"$PROFILE\"}" | jq
done
```

Add to cron for daily execution:

```bash
# Run compliance scan daily at 2:00 AM
0 2 * * * /usr/local/bin/compliance-scan.sh >> /var/log/zyvor-fabricd/compliance.log 2>&1
```

### Alerting on Failures

Combine compliance scanning with the notification system. After a scan completes, check the score and trigger an alert if it falls below your threshold:

```bash
#!/bin/bash
# check-compliance-results.sh
HOST="http://localhost:3000"
TOKEN="$(cat /etc/zyvor-fabricd/api-token)"
AUTH="Authorization: Bearer $TOKEN"
THRESHOLD=85

RESULTS=$(curl -s "$HOST/api/compliance/results" -H "$AUTH")

echo "$RESULTS" | jq -c '.[]' | while read -r result; do
  score=$(echo "$result" | jq '.score')
  vm=$(echo "$result" | jq -r '.vm_name')
  status=$(echo "$result" | jq -r '.status')

  if [ "$status" = "completed" ] && [ "$(echo "$score < $THRESHOLD" | bc -l)" -eq 1 ]; then
    echo "WARNING: $vm compliance score $score% is below threshold $THRESHOLD%"
    # Integrate with your alerting system here
  fi
done
```
