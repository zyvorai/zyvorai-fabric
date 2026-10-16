# Keep Sentinel — policy schema

Policy is a **signed file** you can diff in git.

**Keep mode** (`ZYVOR_AGENT_MERION_MODE=1`): runtime refuses to start without
`ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS`, and every `PUT …/policy` must carry
`X-Merion-Policy-Signature`. Unsigned policy must not load.

Without Keep mode, signatures are required only when trusted signers are configured
(unless `ZYVOR_AGENT_POLICY_REQUIRE_SIGNATURE=0`).

## Example (`merion.policy.yaml`)

```yaml
version: 1
default_egress: deny
allow:
  - { host: api.stripe.com, methods: [POST], action: checkout, ask: always }
  - { host: api.github.com, methods: [GET],  action: read,     ask: first }
deny:
  - { host: "*.onion" }
taint:
  on_untrusted_page: block_egress_until_ask
```

## Fields

| Field | Meaning |
|---|---|
| `default_egress` | `deny` (default) or `allow` |
| `allow[]` | host / methods / action / ask (`always` \| `first` \| `never`) |
| `allow[].rpc_methods` | JSON-RPC method names the request body may call (MCP: `tools/list`, `tools/call`, ...) |
| `allow[].mcp_tools` | For an MCP `tools/call`, the tool names (`params.name`) that may be called |
| `allow[].graphql_operations` | GraphQL operation types the body may run: `query`, `mutation`, `subscription` |
| `allow[].binaries` | Programs in the cell that may reach the host: `path`, and optionally a pinned `sha256` |
| `deny[]` | host patterns always refused |
| `taint.on_untrusted_page` | `block_egress_until_ask` — cockpit paints the process red |

## Risk check on policy changes

Every `PUT …/policy` is compared with the policy the agent runs now, after the signature check. The check is
syntactic: it reports what the change *adds*, and it is not a proof of what a cell can reach.

| Severity | Codes |
|---|---|
| High | `default_egress_allow`, `metadata_host`, `private_ip_host`, `wildcard_all_hosts`, `methods_widened` (a write method added, or the methods list dropped), `approval_weakened`, `taint_guard_removed`, `body_rules_widened`, `binaries_widened` |
| Medium | `internal_name_host`, `wildcard_host`, `new_host_any_method`, `new_write_host_no_approval`, `deny_removed`, `methods_widened` (read methods only) |
| Info | `new_host` |

A change with a High finding is refused with `409` and the reasons. Review it, then resend with
`X-Merion-Policy-Ack-Risk: 1` (`MERION_POLICY_ACK_RISK=1 merionctl policy set …`). The response lists every finding under
`risks`, and the audit row `merion.policy.set` records the codes and whether they were acknowledged. Per-host checks
run for newly added hosts only, so an unchanged entry does not block later edits.

## Suggested rules from denials

A `deny`-mode agent that asks for a host outside its allowlist leaves a `Denied` journal row. `GET
/v1/agents/{name}/policy-suggestions` (operator token only) groups those rows by host and returns the narrowest allow
entry that would have let the calls through, with `ask: always`:

```bash
merionctl policy suggest my-agent draft.yaml     # summary, and the draft policy in draft.yaml
$EDITOR draft.yaml                              # read it; drop what you do not want
merionctl policy sign draft.yaml && merionctl policy set my-agent draft.yaml draft.yaml.sig
```

It changes nothing itself. The agent chooses which hosts it asks for, so a suggestion is a prompt for a person, not a
request that can be approved by the agent. Every suggestion is run through the risk check above; one with a High
finding is listed with `NEEDS ACK` and is left out of the draft. Only denials for a host missing from the allowlist
count: an operator's refusal or a reviewer's verdict does not produce a suggestion. Hosts are limited to plain host or
IP characters, so an agent cannot put YAML or terminal escapes into the draft.

## Body rules: JSON-RPC, MCP and GraphQL

A host with `methods` only limits the verb. For a JSON-RPC, MCP or GraphQL endpoint every call is a `POST` to one
URL, so the verb says nothing. The three body fields make the broker read the request body before anything leaves:

```yaml
allow:
  - host: mcp.example.com
    methods: [POST]
    rpc_methods: [tools/list, tools/call]
    mcp_tools: [search]              # a tools/call to any other tool is refused
    ask: first
  - host: api.github.com
    methods: [POST]
    graphql_operations: [query]      # no mutations
    ask: first
```

- Fail closed: a body that is not the expected JSON, is empty, or is over 1 MiB never satisfies a rule that reads the
  body. A JSON-RPC batch, or a GraphQL batch, needs every entry allowed. A `tools/call` with no readable tool name is
  refused.
- JSON-RPC method names are case-sensitive. A response to a server-initiated request (an `id` and a `result` or
  `error`, no `method`) is passed, since MCP clients send them.
- GraphQL: every operation in the document must be an allowed type, whichever `operationName` selects. A document the
  scanner cannot read with confidence (unbalanced braces, an unclosed string, `extend`, an unknown keyword) is refused.
  Only request bodies are read, so a GraphQL `GET` with the query in the URL cannot satisfy such a rule.
- The CONNECT proxy cannot read a body, so it already refuses any host that has rules. Dropping or widening these
  lists in a policy change is a High finding (`body_rules_widened`) in the risk check above.
- This limits what the agent may ask for. It does not make the endpoint's tools safe, and a rule that allows
  `tools/call` for a tool allows whatever that tool does.

## Which program may call: `binaries`

`binaries` limits an `allow` entry to programs in the cell, by executable path:

```yaml
allow:
  - host: api.github.com
    methods: [GET]
    binaries:
      - path: /usr/bin/node
      - path: /usr/bin/curl
        sha256: 52e0a13e60a981d8c4b6478be2ba5176f69da07948a056bf49cf6f077e30cb41
```

For a request through the JSON broker, the runtime asks the guest agent which program owns the connection. The
guest reads `/proc` as root (socket, then process, then `/proc/<pid>/exe` and its SHA-256), through
`/opt/zyvor/attribute.sh`, which the runtime writes into the cell when a rule names binaries. It sees processes in the
inner container's own PID namespace.

- **Trust on first use, or a pin.** With a `sha256` the program's hash must match. Without one, the first hash seen for
  that agent and path is remembered in `binary-pins.json` under the state directory, and a different hash is refused.
  After a rebuild, an operator clears the pin: `GET` and `DELETE /v1/agents/{name}/binary-pins` (`?path=` for one
  program; operator token only; clearing is journaled).
- **Fails closed.** If the guest cannot say which program it was (no such connection, an unusable answer, the lookup
  failing) or the path is marked ` (deleted)`, the request is refused. So is a request whose program is not listed.
- **A rule is checked as a whole.** A `GET` rule limited to `curl` is not satisfied by a looser `POST` rule for another
  program.
- **What it does not do.** It stops an agent *process* that was talked into calling out through the wrong program, or
  that dropped in its own binary. It trusts the guest kernel, so it does not stop a compromised guest. It needs the
  host channel into the guest, so it does not work for a `confidential` cell (requests are refused, not allowed). It
  covers the JSON broker only: the CONNECT proxy and intercepted tunnels cannot be attributed, and already refuse any
  host that has rules. Each lookup costs a round trip into the guest; results are cached for 3 seconds per connection.
- Dropping the list, adding a program, loosening a pin, or replacing a pinned hash is a High finding
  (`binaries_widened`) in the risk check above.

## Signature

Ed25519 over the exact YAML bytes the API receives (`X-Merion-Policy-Signature: <hex>`).

```bash
./agent-runtime/target/release/examples/keep_sign_policy sign "$SEED" merion.policy.yaml \
  > merion.policy.yaml.sig
./scripts/merionctl policy set <agent> merion.policy.yaml merion.policy.yaml.sig
```

See [PRODUCTION.md](../PRODUCTION.md) and [Tutorial 16](../../tutorials/16-merion-workstation.md).
