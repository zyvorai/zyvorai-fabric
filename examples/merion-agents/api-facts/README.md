# api-facts

A JSON document in, the fields you named out. Paths look like `order.id`, `order.items[0].price` and
`order.items[*].sku` (every element). A path that is not there is reported as `(not found)`, not as an error.

```bash
./scripts/merionctl deploy examples/merion-agents/api-facts --test
```

**Scenario.** Point a system's webhook at a **webhook trigger** (the body is the JSON file) and read the facts
in **Merion history**, without running your own receiver:

```bash
./scripts/merionctl trigger add-webhook api-facts        # prints a secret once
./scripts/merionctl trigger fire <trigger-id> <secret> order.json
```

Edit the `paths` in `pack.json` to your own payload. The whole file must be valid JSON and fit the size limit (200 KB).
