# xcodebuild-log

`xcodebuild` output in, a build triage out: the result (`** BUILD FAILED **`), errors by file and line, error and warning messages (repeats first), failed targets and failed tests.

```bash
xcodebuild -scheme MyApp build > build.log 2>&1
./scripts/merionctl deploy examples/merion-agents/xcodebuild-log --test
```

File paths are shown without their directories, so a home-folder name is not echoed. The upload limit is 200 KB, so cut a long log to the failing part. It reads the standard compiler-diagnostic layout (`file:line:col: error: message`). **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
