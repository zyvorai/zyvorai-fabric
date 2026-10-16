# github-prs

A pull-request listing in, a picture out: how many are merged, open or closed, who opened them, the labels in use, merges per month and the titles.

```bash
gh pr list --state all --limit 100 --json number,title,author,state,createdAt,mergedAt,labels > prs.json
./scripts/merionctl deploy examples/merion-agents/github-prs --test
```

The pack reads the compact JSON that `gh` prints. Keep the upload under 200 KB (about 150 pull requests). It counts and lists; it does not rank reviewers or judge a change. The sample follows output captured from a real run (names and ids replaced with placeholders).

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
