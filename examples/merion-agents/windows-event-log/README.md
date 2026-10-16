# windows-event-log

An event log export in, a triage out: counts by level, provider and event id, and the error and warning rows.

```powershell
Get-WinEvent -LogName System -MaxEvents 500 |
  Select-Object TimeCreated, Id, LevelDisplayName, ProviderName, Message |
  Export-Csv -NoTypeInformation events.csv
```
```bash
./scripts/merionctl deploy examples/merion-agents/windows-event-log --test
```

Select those columns as shown so the layout matches. The upload limit is 200 KB, so export a few hundred events. The
error and warning sections list the first matching rows, message included, which can name files, accounts and hosts.
It does not read `.evtx` (a binary format).

**Not this:** Keep does not connect to your PC. It reads a file you export, in a sealed Linux cell. **The sample is written from the command's documented layout; it has not been checked against an export from a real Windows machine**, so try the pack on your own export before relying on it.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
