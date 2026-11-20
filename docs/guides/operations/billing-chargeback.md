# Billing and Chargeback

How to configure pricing, track resource usage, and generate invoices for multi-tenant VM environments.

## Table of Contents

- [Overview](#overview)
- [Pricing Configuration](#pricing-configuration)
- [Usage Tracking](#usage-tracking)
- [Invoice Generation](#invoice-generation)
- [Tenant Assignment](#tenant-assignment)
- [Reporting and Automation](#reporting-and-automation)

---

## Overview

Zyvor Fabric includes a built-in billing system that tracks resource consumption per VM and per tenant. The billing engine supports:

- **Configurable pricing** -- Set per-unit rates for CPU, memory, disk, and network usage.
- **Automatic usage tracking** -- Resource consumption is metered continuously and aggregated per billing period.
- **Invoice generation** -- Generate itemized invoices per tenant on demand.
- **Tenant isolation** -- VMs are assigned to tenants via labels, enabling cost allocation across teams or users.

The billing system is designed for internal chargeback and showback. It does not process payments directly but produces the data needed for integration with financial systems.

---

## Pricing Configuration

### Viewing Current Pricing

```bash
curl -s http://localhost:3000/api/billing/pricing \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Response:**

```json
{
  "cpu_per_hour": 0.01,
  "memory_gb_per_hour": 0.005,
  "disk_gb_per_hour": 0.001,
  "network_egress_per_gb": 0.02,
  "currency": "USD"
}
```

### Updating Pricing Rules

Pricing changes take effect immediately for new usage records. Historical usage retains the pricing that was active at the time of recording.

```bash
curl -s -X PUT http://localhost:3000/api/billing/pricing \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "cpu_per_hour": 0.02,
    "memory_gb_per_hour": 0.008,
    "disk_gb_per_hour": 0.002,
    "network_egress_per_gb": 0.03,
    "currency": "USD"
  }' | jq
```

### Pricing Fields

| Field | Unit | Description |
|-------|------|-------------|
| `cpu_per_hour` | per vCPU-hour | Cost per vCPU allocated per hour |
| `memory_gb_per_hour` | per GB-hour | Cost per GB of memory allocated per hour |
| `disk_gb_per_hour` | per GB-hour | Cost per GB of disk allocated per hour |
| `network_egress_per_gb` | per GB | Cost per GB of outbound network traffic |
| `currency` | string | Currency code (e.g., `USD`, `EUR`) |

---

## Usage Tracking

The billing engine automatically tracks resource usage for every VM. Usage is aggregated per VM per billing period.

### Viewing Usage Records

```bash
curl -s http://localhost:3000/api/billing/usage \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Response:**

```json
[
  {
    "vm_name": "web-server",
    "tenant_id": "tenant-alpha",
    "period_start": "2026-04-01T00:00:00Z",
    "period_end": "2026-04-12T00:00:00Z",
    "cpu_hours": 264.5,
    "memory_gb_hours": 529.0,
    "disk_gb_hours": 10580.0,
    "network_egress_gb": 12.3,
    "total_cost": 15.42
  },
  {
    "vm_name": "db-server",
    "tenant_id": "tenant-alpha",
    "period_start": "2026-04-01T00:00:00Z",
    "period_end": "2026-04-12T00:00:00Z",
    "cpu_hours": 528.0,
    "memory_gb_hours": 2112.0,
    "disk_gb_hours": 26400.0,
    "network_egress_gb": 3.1,
    "total_cost": 42.78
  }
]
```

### Usage Fields

| Field | Description |
|-------|-------------|
| `cpu_hours` | Total vCPU-hours consumed (vCPUs allocated x hours running) |
| `memory_gb_hours` | Total GB-hours of memory consumed |
| `disk_gb_hours` | Total GB-hours of disk allocated |
| `network_egress_gb` | Total GB of outbound network traffic |
| `total_cost` | Calculated cost based on active pricing rules |

---

## Invoice Generation

Generate an itemized invoice for a specific tenant covering the current billing period.

```bash
curl -s -X POST http://localhost:3000/api/billing/invoice/tenant-alpha \
  -H "Authorization: Bearer $TOKEN" | jq
```

**Response:**

```json
{
  "invoice_id": "INV-2026-04-001",
  "tenant_id": "tenant-alpha",
  "period_start": "2026-04-01T00:00:00Z",
  "period_end": "2026-04-12T00:00:00Z",
  "line_items": [
    {"description": "web-server CPU (264.5 hours)", "amount": 5.29},
    {"description": "web-server Memory (529.0 GB-hours)", "amount": 4.23},
    {"description": "web-server Disk (10580.0 GB-hours)", "amount": 10.58},
    {"description": "web-server Network Egress (12.3 GB)", "amount": 0.25},
    {"description": "db-server CPU (528.0 hours)", "amount": 10.56},
    {"description": "db-server Memory (2112.0 GB-hours)", "amount": 16.90},
    {"description": "db-server Disk (26400.0 GB-hours)", "amount": 26.40},
    {"description": "db-server Network Egress (3.1 GB)", "amount": 0.06}
  ],
  "total": 74.27,
  "currency": "USD"
}
```

Each line item breaks down the cost by resource type per VM, giving tenants full visibility into their usage.

---

## Tenant Assignment

VMs are assigned to tenants with the first-class `tenant` field (merged into
`labels.tenant`) or by setting `labels.tenant` directly. Either way, start
passes the value to FluxVM, and you can filter with `GET /api/vms?tenant=`.

```bash
curl -s -X POST http://localhost:9095/api/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "web-server",
    "image": "/var/lib/fluxvm/images/ubuntu.qcow2",
    "cpus": 4,
    "memory": 4096,
    "disk": 40,
    "tenant": "tenant-alpha",
    "labels": {
      "env": "production",
      "team": "platform"
    }
  }' | jq

curl -s "http://localhost:9095/api/vms?tenant=tenant-alpha" \
  -H "Authorization: Bearer $TOKEN" | jq '.items[].name'
```

### Label-Based Cost Allocation

Labels provide flexible cost allocation beyond simple tenant assignment:

| Label | Purpose | Example |
|-------|---------|---------|
| `tenant` | Primary billing entity (also first-class create field) | `tenant-alpha` |
| `env` | Environment classification | `production`, `staging`, `dev` |
| `team` | Team or department | `platform`, `data`, `frontend` |
| `project` | Project or cost center | `project-x`, `cc-1234` |

Use these labels to group and filter usage records for internal reporting.

---

## Reporting and Automation

### Monthly Invoice Generation

Automate invoice generation at the end of each billing period:

```bash
#!/bin/bash
# generate-invoices.sh -- Run on the 1st of each month
HOST="http://localhost:3000"
TOKEN="$(cat /etc/zyvor-fabricd/api-token)"
AUTH="Authorization: Bearer $TOKEN"

# Get unique tenant IDs from usage records
TENANTS=$(curl -s "$HOST/api/billing/usage" -H "$AUTH" | jq -r '.[].tenant_id' | sort -u)

for tenant in $TENANTS; do
  echo "Generating invoice for $tenant..."
  curl -s -X POST "$HOST/api/billing/invoice/$tenant" \
    -H "$AUTH" | jq > "/var/lib/zyvor-fabricd/invoices/${tenant}-$(date +%Y-%m).json"
done
```

Add to cron:

```bash
# Generate invoices on the 1st of each month at midnight
0 0 1 * * /usr/local/bin/generate-invoices.sh >> /var/log/zyvor-fabricd/billing.log 2>&1
```

### Cost Monitoring

Monitor costs in real time to catch unexpected usage spikes:

```bash
# Check current total cost across all tenants
curl -s http://localhost:3000/api/billing/usage \
  -H "Authorization: Bearer $TOKEN" | jq '[.[].total_cost] | add'

# Get cost breakdown by tenant
curl -s http://localhost:3000/api/billing/usage \
  -H "Authorization: Bearer $TOKEN" | jq 'group_by(.tenant_id) | map({tenant: .[0].tenant_id, total: [.[].total_cost] | add})'
```
