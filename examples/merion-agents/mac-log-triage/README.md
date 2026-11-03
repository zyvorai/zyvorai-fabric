# mac-log-triage

Compact unified-log output in, a triage out: which processes logged errors or faults, the most repeated error lines,
sandbox denials, kernel, memory and thermal trouble, and the busiest processes.

```bash
/usr/bin/log show --last 5m --style compact | head -c 190000 > mac.log
./scripts/merionctl deploy examples/merion-agents/mac-log-triage --test
```

Use `/usr/bin/log` (in zsh a bare `log` is a different builtin). The upload limit is 200 KB, so keep the window short or cut
the file as above; a whole hour is many megabytes. The layout is `--style compact`'s (`date time type process[pid:tid] message`,
where the type is `E` error, `F` fault, `Df`, `I`, `Db`); the sample uses placeholder processes. Messages can name
files, accounts and apps, so read the output before sharing it.

**Not this:** Merion does not connect to your Mac. It reads a file you export, in a sealed Linux cell.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
