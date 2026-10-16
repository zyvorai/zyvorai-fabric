# windows-services

A service listing in, a summary out: how many are running or stopped, the start types, and the service names.

```powershell
Get-Service | Select-Object Name,DisplayName,Status,ServiceType,StartType | Export-Csv -NoTypeInformation services.csv
```
```bash
./scripts/merionctl deploy examples/merion-agents/windows-services --test
```

Select those columns so the layout matches (the default export also carries the machine name). It counts; it does not say which stopped automatic services matter. **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
