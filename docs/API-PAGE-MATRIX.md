# Web page → API endpoint matrix (tier-1)

Generated from [`scripts/audit-ux-apis.sh`](../scripts/audit-ux-apis.sh) and primary web routes. Use this to detect UI/backend drift.

| Web page | Primary API endpoints |
|----------|----------------------|
| Dashboard | `/api/vms`, `/api/system/info`, `/api/system/metrics` |
| VirtualMachines | `/api/vms`, `/api/vms/{name}` |
| Logs / AuditLogs | `/api/audit/logs`, `/api/logs` |
| Migrations | `/api/migrations`, `/api/migrations/history` |
| Network | `/api/networkd/*`, `/api/network/topology` |
| NetworkSecurity | `/api/network-policies`, `/api/firewall-profiles`, `/api/nat-rules` |
| Storage | `/api/storage/pools` |
| Alerts | `/api/system/alerts`, `/api/system/alerts/rules` |
| ComplianceDashboard | `/api/system/compliance`, `/api/compliance/results` |
| Certificates | `/api/certificates`, `/api/certificates/health` |
| Backups | `/api/backups`, `/api/schedules` |
| Profiles | `/api/profiles` |
| Events (Machina) | `/api/events`, `/api/events/stream` |
| Time Machine (v0.4) | `/api/config/snapshot`, `/api/events/retention` |

## CI gates

1. `scripts/audit-ux-apis.sh` — tier-1 GET JSON smoke (no SPA HTML fallthrough)
2. `scripts/audit-ux-apis-post.sh` — POST/PUT/DELETE round-trip on sandbox objects
3. `scripts/test-api-prefix-parity.sh` — `/api` vs `/api/v1` equivalence

Run together via `scripts/ci-api-audit.sh`.
