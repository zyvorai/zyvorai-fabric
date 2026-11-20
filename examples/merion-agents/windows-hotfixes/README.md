# windows-hotfixes

A `Get-HotFix` export in, an update audit out: the KB numbers, the kinds of update, who installed them, and the install dates.

```powershell
Get-HotFix | Export-Csv -NoTypeInformation hotfixes.csv
```
```bash
./scripts/merionctl deploy examples/merion-agents/windows-hotfixes --test
```

It expects the default columns (`HotFixID`, `Description`, `InstalledBy`, `InstalledOn`) and needs `-NoTypeInformation`
(without it the first line is a `#TYPE` comment and the header is not found). It counts and lists; it does not know
which KBs your machine should have. The summary shows the account names in `InstalledBy`.

**Not this:** Merion does not connect to your PC. It reads a file you export, in a sealed Linux cell. **The sample is written from the command's documented layout; it has not been checked against an export from a real Windows machine**, so try the pack on your own export before relying on it.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
