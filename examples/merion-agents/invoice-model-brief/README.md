# invoice-brief (with a model step)

The rule sections work on their own. The **model step** adds a short generated summary, and it is the only part
that sends anything out, so it is off until you configure and approve it.

**Before you deploy**, edit `model.base_url` and `model.model` to your provider (or a local server on
`http://127.0.0.1:8080/v1`), and add a vault credential named `llm` on the runtime host. See
[MODEL.md](../../../docs/merion/MODEL.md#configure-the-endpoint) for the credentials file.

```bash
./scripts/merionctl deploy examples/merion-agents/invoice-model-brief      # validates; sends nothing
./scripts/merionctl run invoice-model-brief invoice.pdf                  # first run waits for your approval
./scripts/merionctl grants list                                          # what you approved, revocable
```

What happens: the cell extracts the PDF text and finishes (still 0 CONNECT). The host then sends the text, cut to
12 000 characters, to the endpoint the vault allows, after you approve it once. The run response and cockpit say
where the text went. If the vault has no `llm` credential the run is refused (403) before a cell is created.

Do not use `--test`: there is no bundled sample, and a test run would ask for the approval.
