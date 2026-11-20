# API Reference Overview

The Zyvor Fabric REST API provides comprehensive programmatic access to all platform capabilities. This reference documents the API design principles, common patterns, and provides an index to detailed endpoint documentation.

## API Design Principles

1. **RESTful resource model** -- Resources are identified by URL paths. Standard HTTP methods (GET, POST, PUT, DELETE) map to CRUD operations.
2. **JSON throughout** -- All request and response bodies use JSON (`Content-Type: application/json`).
3. **JWT authentication** -- All endpoints (except `/api/auth/login`, `/health`, and `/readyz`) require a Bearer token in the `Authorization` header.
4. **Role-based authorization** -- Three roles (Admin, User, Viewer) control access. Endpoints enforce minimum permission levels using extractors: `RequireRead` (Viewer+), `RequireWrite` (User+), `RequireAdmin` (Admin only).
5. **Consistent error format** -- All errors return `{"error": "message"}` with appropriate HTTP status codes.
6. **Pagination** -- List endpoints accept `?offset=N&limit=N` query parameters. Default limit is 200, maximum is 1000.

## Base URL

```
http://<host>:3000/api
```

## Authentication

Include the JWT token in every request:

```
Authorization: Bearer <token>
```

See [Authentication](authentication.md) for the full login flow and token details.

## Common Response Codes

| Code | Meaning |
|------|---------|
| 200  | Success |
| 201  | Resource created |
| 202  | Accepted (async operation started) |
| 204  | Success, no content (e.g., DELETE) |
| 400  | Bad request / validation error |
| 401  | Authentication required |
| 403  | Insufficient permissions |
| 404  | Resource not found |
| 409  | Conflict (duplicate name, invalid state) |
| 429  | Rate limited |
| 500  | Internal server error |
| 503  | Service unavailable (connection limit) |

## Endpoint Categories

| Category | Base Path | Endpoints | Description |
|----------|-----------|-----------|-------------|
| [Authentication](authentication.md) | `/api/auth` | 2 | Login and token management |
| VMs | `/api/vms` | 15+ | VM CRUD, lifecycle, clone, metrics |
| Images | `/api/images` | 8+ | Build, list, download, import, resize, ISOs |
| Snapshots | `/api/vms/:name/snapshots` | 6 | Create, list, get, delete, revert, tree |
| Backups | `/api/backups` | 10+ | Create, list, restore, policies, jobs |
| Networking | `/api/networkd` | 30+ | Bridges, VLANs, bonds, taps, port forwarding |
| Storage | `/api/storage` | 10+ | Local, NFS, LVM, ZFS, Ceph pools |
| Events | `/api/events` | 2 | SSE stream and event history |
| System | `/api/system` | 10+ | CPU topology, NUMA, hugepages, memory |
| Cloud-init | `/api/vms/:name/cloud-init` | 1 | Generate cloud-init ISO |
| Notifications | `/api/notifications` | 10+ | Channels, rules, history |
| [WebSocket](websocket.md) | `/api/vms/:name/console` | 1 | Interactive console |

## Paginated List Responses

List endpoints that support pagination return:

```json
{
  "items": [...],
  "total": 42,
  "offset": 0,
  "limit": 200
}
```

## Async Operations

Operations that may take a long time (VM start, image build, backup create) return `202 Accepted` immediately and perform work in the background. Monitor progress via the SSE event stream or by polling the resource status.

```json
{
  "status": "starting"
}
```
