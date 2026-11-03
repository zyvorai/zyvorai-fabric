# xcode-crash-log

A legacy `.crash` report in, a summary out: the app and version, OS, exception type and codes, the crashed thread, notes the app left, and the frames (repeats first).

```bash
./scripts/merionctl deploy examples/merion-agents/xcode-crash-log --test
./scripts/merionctl run xcode-crash-log MyApp.crash
```

**Modern macOS writes `.ips` reports (JSON); this pack reads the text `.crash` layout only.** The `Path:` line, which can hold a user name, is not listed. **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
