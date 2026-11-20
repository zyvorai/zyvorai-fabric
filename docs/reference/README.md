# Reference Documentation

This directory contains detailed specifications and reference materials for the Zyvor Fabric platform.

## Contents

### [API Reference](api/)

Detailed API specifications covering authentication, data models, and protocol documentation.

- **[API Overview](api/README.md)** -- API design principles, versioning, common patterns, and endpoint index.
- **[Authentication](api/authentication.md)** -- Login flow, JWT token structure (claims: sub, role, exp, jti), token revocation, role-based access control (Admin, User, Viewer), PAM integration, and rate limiting.
- **[WebSocket API](api/websocket.md)** -- WebSocket console protocol for interactive VM terminal access, including connection authentication, message format, connection limits, and idle timeout behavior.

## Related Documentation

- **[CLI Guide / API Reference](../guides/cli/api-reference.md)** -- Practical endpoint-by-endpoint reference with curl examples.
- **[Architecture Overview](../architecture.md)** -- System architecture, component diagram, and crate dependency graph.
- **[Product Overview](../PRODUCT_OVERVIEW.md)** -- High-level feature summary and use cases.
