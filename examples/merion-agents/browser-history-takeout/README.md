# browser-history-takeout

Browser history from Google Takeout in, a browsing summary out: the sites visited most, how pages were reached (link, typed and so on) and the page titles.

```bash
./scripts/merionctl deploy examples/merion-agents/browser-history-takeout --test
```

**Browsing history is highly personal.** The summary lists page titles, so read it before sharing. The upload limit is 300 KB, so a full history must be cut down first. It does not convert times or judge sites. **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Keep does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Keep secrets, tokens and private keys out of the files.
