# CLI Guide

This section covers how to interact with the Zyvor Fabric REST API from the command line using standard tools like `curl` and `jq`.

## Contents

- **[API Reference](api-reference.md)** -- Complete REST API reference with all endpoint categories, request/response schemas, and curl examples.

## Prerequisites

- A running Zyvor Fabric daemon (`Zyvor Fabric`)
- `curl` for HTTP requests
- `jq` for JSON formatting (optional but recommended)
- A valid user account (PAM-authenticated system user)

## Quick Start

### 1. Authenticate

```bash
TOKEN=$(curl -s -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"secret"}' | jq -r '.token')
```

### 2. List VMs

```bash
curl -s http://localhost:3000/api/vms \
  -H "Authorization: Bearer $TOKEN" | jq
```

### 3. Create a VM

```bash
curl -s -X POST http://localhost:3000/api/vms \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"my-vm","cpus":2,"memory_mb":2048,"disk_gb":20}' | jq
```

### 4. Start a VM

```bash
curl -s -X POST http://localhost:3000/api/vms/my-vm/start \
  -H "Authorization: Bearer $TOKEN" | jq
```

## Authentication

All API endpoints (except `/api/auth/login`, `/health`, and `/readyz`) require a valid JWT token in the `Authorization` header. Tokens are obtained by authenticating against PAM with system credentials.

See the [Authentication Reference](../../reference/api/authentication.md) for full details on the token lifecycle and role-based access control.

## Error Responses

All error responses follow a consistent JSON format:

```json
{
  "error": "Description of what went wrong"
}
```

Standard HTTP status codes are used:

| Code | Meaning |
|------|---------|
| 400  | Bad Request -- invalid input or validation failure |
| 401  | Unauthorized -- missing or invalid token |
| 403  | Forbidden -- insufficient role permissions |
| 404  | Not Found -- resource does not exist |
| 409  | Conflict -- resource already exists or invalid state transition |
| 429  | Too Many Requests -- rate limit exceeded |
| 500  | Internal Server Error -- server-side failure |
| 503  | Service Unavailable -- connection limit reached |
