# github-actions-log

A failed CI log in, the failure out: the `##[error]` lines, which job and step failed, exit codes, and the compiler errors and warnings that repeat most.

```bash
gh run view <run-id> --log-failed > actions.log
./scripts/merionctl deploy examples/merion-agents/github-actions-log --test
```

Lines look like `job<TAB>step<TAB>timestamp text`. The upload limit is 200 KB, so use `--log-failed`, not the full log. The error patterns fit Rust-style `error:` and `warning:` lines; add your compiler's wording in `pack.json`. The sample follows output captured from a real run (names and ids replaced with placeholders).

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
