# mac-apps-inventory

The installed-applications report in, an inventory out: the apps, where each came from (Apple, App Store, Identified Developer, Unknown), the first signer, the kind (Apple Silicon or Intel) and the top-level folder they live in.

```bash
system_profiler SPApplicationsDataType > apps.txt
./scripts/merionctl deploy examples/merion-agents/mac-apps-inventory --test
```

The upload limit is 200 KB, and a full report is larger, so filter it (for example `sed -n '1,600p'`) or run it for one folder. Only the first folder of each location is listed, so a home-folder name is not echoed. It counts and lists; it is not malware detection. The sample follows output captured from a real run (names and ids replaced with placeholders).

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
