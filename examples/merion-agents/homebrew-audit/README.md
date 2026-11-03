# homebrew-audit

A Homebrew listing in, an audit out: how many packages, which ones keep more than one version installed (a sign of
old versions worth cleaning up), and the toolchains and runtimes present. It reads either of:

```bash
brew list --versions > brew.txt          # what is installed
brew outdated --verbose > outdated.txt   # what has a newer version
./scripts/merionctl deploy examples/merion-agents/homebrew-audit --test
```

The "Outdated" section fills in only for `brew outdated --verbose` output, and "keep more than one version" only for
`brew list --versions`; the other shows "(no matches)". The bundled sample is written from Homebrew's documented output
layout (one package per line, name then versions).

**Not this:** Merion does not connect to your Mac. It reads a file you export, in a sealed Linux cell.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
