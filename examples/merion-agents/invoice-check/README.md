# invoice-check (a use case you define, not a built-in)

A declarative use case: no code, just an extractor and a few summary rules in `pack.json`.
This is the shape to copy when you want your own.

```bash
# from the CLI (dry run first, then deploy and test it on the sample)
node sdk/agent-runtime/src/cli.js pack deploy examples/merion-agents/invoice-check --dry-run
node sdk/agent-runtime/src/cli.js pack deploy examples/merion-agents/invoice-check --test
# or: ./scripts/merionctl deploy examples/merion-agents/invoice-check --test
# or in the console: /app/merion → Deploy your own use case → paste pack.json (JSON tab)
```

The runtime validates the spec (bounded rules, an extractor from a fixed list, no commands),
runs it in the same sealed cell as the built-ins, and requires 0 CONNECT.
See [Tutorial 19](../../../docs/tutorials/19-build-your-own-use-case.md).
