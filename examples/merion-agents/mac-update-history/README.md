# mac-update-history

The output of `softwareupdate --history` in, a summary out: each update that was installed (repeats counted), its versions,
the dates, which were betas and which were Command Line Tools.

```bash
softwareupdate --history > history.txt
./scripts/merionctl deploy examples/merion-agents/mac-update-history --test
```

The sample is real `softwareupdate --history` output. Dates print in the Mac's regional order (`dd/mm/yyyy` or
`mm/dd/yyyy`) and are shown as printed, not converted.

**Not this:** Keep does not connect to your Mac. It reads a file you export, in a sealed Linux cell.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
