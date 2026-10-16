# git-log-digest

`git log` output in, an activity summary out: commits per author and per month, the commit-type prefixes (`fix`, `docs`, `keep` and so on) and how many are merges.

```bash
git log --pretty=format:'%h|%an|%ad|%s' --date=short -n 500 > log.txt
./scripts/merionctl deploy examples/merion-agents/git-log-digest --test
```

Use exactly that format (`hash|author|date|subject`). It counts commits; it does not measure quality, and author names are as git records them, so the same person under two names counts twice. The sample follows output captured from a real run (names and ids replaced with placeholders).

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
