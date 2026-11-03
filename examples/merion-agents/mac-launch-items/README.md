# mac-launch-items

`launchctl list` output in, a persistence view out: which items are not from Apple (background updaters, helpers and the like), the exit statuses, and the labels that ended with a non-zero status or a signal.

```bash
launchctl list > launch.txt
./scripts/merionctl deploy examples/merion-agents/mac-launch-items --test
```

It lists labels only. Labels that do not begin `com.apple.` show as third-party, which includes some system entries such as `application.` instance labels; read the list, do not treat it as a verdict. It is not malware detection. The sample follows output captured from a real run (names and ids replaced with placeholders).

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
