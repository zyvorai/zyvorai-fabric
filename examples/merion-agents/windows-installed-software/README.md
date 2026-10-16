# windows-installed-software

An installed-programs CSV in, an inventory out: the most common publishers and program names, and the first rows.

```powershell
Get-ItemProperty HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* |
  Select-Object DisplayName, DisplayVersion, Publisher, InstallDate |
  Export-Csv -NoTypeInformation software.csv
```
```bash
./scripts/merionctl deploy examples/merion-agents/windows-installed-software --test
```

It expects the columns `DisplayName`, `DisplayVersion`, `Publisher`, `InstallDate`; rename them in `pack.json` for another
export. That query lists 64-bit machine-wide programs; add the `WOW6432Node` and per-user hives for a fuller list. It
counts and lists; it does not judge a program.

**Not this:** Keep does not connect to your PC. It reads a file you export, in a sealed Linux cell. **The sample is written from the command's documented layout; it has not been checked against an export from a real Windows machine**, so try the pack on your own export before relying on it.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
