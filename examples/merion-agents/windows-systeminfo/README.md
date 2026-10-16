# windows-systeminfo

`systeminfo` output in, a machine summary out: OS and build, install date, last boot, maker, model, architecture, BIOS,
memory, domain, and the list of installed hotfixes (KB numbers).

```powershell
systeminfo > si.txt
```
```bash
./scripts/merionctl deploy examples/merion-agents/windows-systeminfo --test
```

**Host name, registered owner, logon server and network details (IP addresses) are not listed**: no rule matches them.
It expects the English `systeminfo` labels; another display language needs its own patterns in `pack.json`.

**Not this:** Keep does not connect to your PC. It reads a file you export, in a sealed Linux cell. **The sample is written from the command's documented layout; it has not been checked against an export from a real Windows machine**, so try the pack on your own export before relying on it.

These files describe a real machine and can contain hostnames, user names and installed software, so treat the summary as sensitive as the export. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)).
