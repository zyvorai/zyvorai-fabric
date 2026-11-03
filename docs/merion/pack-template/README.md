# my-pack

One sentence: what goes in, what comes out.

```bash
./scripts/merionctl deploy docs/merion/pack-template --dry-run   # validate the pack
./scripts/merionctl deploy examples/merion-agents/my-pack --test # run its sample in a real cell
```

- **Reads:** which file types, and how a person gets one (a command, an export menu).
- **Gives:** the sections in the summary.
- **Does not do:** what it cannot read (scans, other languages, other layouts).

These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is
`software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
