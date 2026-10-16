# github-issues

An issue listing in, a picture out: open versus closed, who opened them, the labels in use, issues opened per month and the titles.

```bash
gh issue list --state all --limit 100 --json number,title,author,state,labels,createdAt > issues.json
./scripts/merionctl deploy examples/merion-agents/github-issues --test
```

It reads the compact JSON `gh` prints; keep the upload under 200 KB. It counts and lists; it does not triage or prioritise. The sample follows output captured from a real run (names and ids replaced with placeholders).

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
