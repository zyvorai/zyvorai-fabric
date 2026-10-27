# Design: MCP gateway and registry

Status: **built, off until an operator registers a server.**

Agent teams put a gateway between agents and MCP tool servers to get one place for who may call what, how often, and
a record of it. Merion already had the hard parts (the egress broker with host rules, MCP body rules, DLP, taint,
approvals, the vault). This adds the registry and the relay on top, so none of those are re-implemented.

**As built**
- **Registry (operator only).** `GET /v1/mcp/servers`, `GET/PUT/DELETE /v1/mcp/servers/{name}` on the public router.
  An entry has `url` (`https`, or `http` to a loopback address; no credentials in the URL), an optional `credential`
  (a vault name the broker attaches; the agent never sees the value), `tools` (the tool names a `tools/call` may use;
  empty means none) and `max_calls_per_minute`. The name comes from the path. Stored in `mcp_servers.json`; set and
  remove are journaled (`merion.mcp_server.set`, `.removed`).
- **Relay (the cell).** `POST /v1/mcp/{server}` on the broker port, with the same session id and egress capability
  headers as `/v1/egress`. Body: one JSON-RPC 2.0 request. Relayed methods: `initialize`,
  `notifications/initialized`, `ping`, `tools/list`, `tools/call`. Batches and every other method (sampling,
  resources, prompts, completion) are refused with 403. A `tools/call` must name a listed tool.
- **Rate limit** per session and server per minute, in memory (a restart resets it).
- **Journal.** Each call writes an `mcp.call` row with the method, tool name and outcome. Arguments and results are
  never journaled.
- **Then the ordinary broker.** The relay builds an egress request to the server's URL and runs it through the same
  path as any brokered call, so the agent's own host rules, MCP body rules, DLP, taint, write limit and approvals
  still apply. **A registry entry is necessary, not sufficient:** the agent's policy must also allow the server's
  host, and an `ask` rule there holds a call for a person.

**Not built**
- No per-tool "ask" in the registry itself; approvals come from the agent's egress rules for the host.
- No discovery or health checking of servers, no SSE or streaming responses, no session ids for stateful servers.
- The rate limit is not shared across runtime replicas.
- Tested with unit tests and a local stub server that answers every request with `ok`; not against a real MCP server.
