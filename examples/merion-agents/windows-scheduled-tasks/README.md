# windows-scheduled-tasks

The scheduled-tasks listing in, a view out: the tasks, their status and enabled state, the last result code and the account each runs as.

```powershell
schtasks /query /fo csv /v > tasks.csv
```
```bash
./scripts/merionctl deploy examples/merion-agents/windows-scheduled-tasks --test
```

The host name column is not listed. `schtasks` repeats the header row for each folder, so a `Status` or `TaskName` value may appear from those rows; ignore it. A last result of `0` is success; other codes are the task's own. Account names appear in the output. **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
