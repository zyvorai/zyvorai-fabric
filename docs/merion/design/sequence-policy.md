# Design: limits over a session's history

Status: **first rule built.** One rule, off by default.

Single-request rules (host, method, MCP tool, body) cannot see that an agent has made forty writes in a row. People
building agent gateways ask for rules over the sequence of calls; this adds the smallest useful one.

**As built**
- Agent manifest and `merion.policy.yaml` take `sequence.max_writes` (at least 1). After that many brokered writes in
  one session (any method but GET and HEAD), each further write is held as an approval whose reason reads
  `<n> writes already made in this session (limit <max>)`. A person approves or denies it like any other.
- The count is `writes` on the session record, saved with it, so a restart does not reset it. It counts a write after
  the upstream answered, so a denied or failed-to-send request does not use the allowance.
- The agent cannot change it: it is part of the deployed manifest and the signed policy, like `taint`.
- Reading content from an untrusted host already holds later writes (`taint`); this rule is the count, not a second
  taint.

**Not built**
- It applies to the **brokered** path only. The CONNECT proxy cannot see a method, so writes through it are not
  counted.
- No rule for ordering (for example "no send after reading from host X" other than what `taint` gives), byte totals,
  or time windows.
- Tested with unit tests and a local stub upstream; not against a real agent.
