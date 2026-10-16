---
name: keep-policy
description: Write, sign and load a Keep Sentinel policy (keep.policy.yaml). Use when asked to allow or deny an agent's network access, tighten egress, or set which actions need approval.
---

# Keep policy

Reference: `docs/merion/sentinel/README.md`, example `docs/merion/sentinel/merion.policy.yaml`.

## Shape

```yaml
version: 1
default_egress: deny
allow:
  - { host: api.github.com, methods: [GET], action: read, ask: first }
deny:
  - { host: "*.onion" }
taint:
  on_untrusted_page: block_egress_until_ask
```

- `default_egress`: keep it `deny`. Only change it if the user explicitly asks.
- `allow[]`: one entry per host. List the narrowest `methods`. `ask` is `always`, `first` or `never`; use `always` for anything that spends money or sends data out.
- `deny[]`: host patterns that are always refused; deny wins.

## Load it

Keep mode (`ZYVOR_AGENT_KEEP_MODE=1`) refuses unsigned policy. The signature is Ed25519 over the exact bytes of the file, so do not reformat the YAML after signing.

```bash
./agent-runtime/target/release/examples/keep_sign_policy sign "$SEED" keep.policy.yaml > keep.policy.yaml.sig
./scripts/merionctl policy set <agent> keep.policy.yaml keep.policy.yaml.sig
merionctl policy show <agent>
```

## MCP, JSON-RPC and GraphQL hosts

For these endpoints the verb is always `POST`, so limit the body instead: `rpc_methods` (JSON-RPC method names), `mcp_tools` (tool names allowed for `tools/call`) and `graphql_operations` (`query`, `mutation`, `subscription`) on the `allow` entry. Start with the narrowest list the agent needs: for a search-only agent, `rpc_methods: [tools/list, tools/call]`, `mcp_tools: [search]`, and `graphql_operations: [query]`. A body that cannot be read as the expected JSON is refused. Details in `docs/merion/sentinel/README.md`.

## Limiting which program may call

`binaries: [{path: /usr/bin/node}]` on an `allow` entry limits the host to that program in the cell; add `sha256` to pin its hash, or leave it out for trust on first use. If a rebuilt program is refused as "not the program first seen", an operator clears the pin with `DELETE /v1/agents/{name}/binary-pins`; do that only when the user confirms the change is expected. It does not work for confidential cells and covers the JSON broker only. Details in `docs/merion/sentinel/README.md`.

## Risk check

The runtime compares every new policy with the current one. A change with a High finding (default egress `allow`, a metadata or private host, a `*` host, a write method added, weaker `ask`, a removed taint guard, a dropped or widened body rule, a widened program list) is refused with `409` and the reasons. Show those reasons to the user. Resend with `KEEP_POLICY_ACK_RISK=1 merionctl policy set ...` only after the user says the change is intended. Codes are listed in `docs/merion/sentinel/README.md`.

## Drafting from denials

If an agent was blocked from hosts it needs, `merionctl policy suggest <agent> draft.yaml` lists the denied hosts and writes a draft policy with a narrow `allow` entry (`ask: always`) for each. Read the draft with the user, remove anything not wanted, then sign and load it as above. Suggestions marked `NEEDS ACK` are not in the draft; do not add them without the user asking.

## Before proposing a change

Tell the user, in plain words, what the change adds: new hosts, new methods, wider `ask`, wildcards. Flag any host that is a private address, a cloud metadata address, or a wildcard. Never sign a policy yourself; the signing seed is the owner's.
