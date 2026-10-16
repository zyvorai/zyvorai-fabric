# meeting-notes-model (with a model)

Decisions and actions come from rules. The **model step** adds a five-bullet summary. It is set up for a **local
model** on `http://127.0.0.1:8080/v1` (an inference server you run on the same host), so the text never leaves the
machine. Point it at a hosted API instead only if you are happy for the transcript to go there.

Add a vault credential `llm` for `127.0.0.1` with `"allowed_ports": [8080]` (see
[MODEL.md](../../../docs/merion/MODEL.md#configure-the-endpoint)), then:

```bash
./scripts/merionctl deploy examples/merion-agents/meeting-notes-model
./scripts/merionctl run meeting-notes-model sample.txt      # first run waits for your approval
```

Without the credential the run is refused (403) before any cell starts. `sample.txt` is a made-up transcript.
Treat the generated summary as a draft: a model can be wrong, and it can be steered by text in the transcript.
