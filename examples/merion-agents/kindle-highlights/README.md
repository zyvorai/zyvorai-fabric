# kindle-highlights

Your Kindle's `My Clippings.txt` in, a reading digest out: books ranked by how many clippings each has, how many highlights, notes and bookmarks, and the dates.

**Get the file:** connect the Kindle by USB and copy `documents/My Clippings.txt`. The book title is the line that repeats for each clipping, so the ranking counts clippings, not pages. It does not read the highlight text for meaning.

```bash
./scripts/merionctl deploy examples/merion-agents/kindle-highlights --test
./scripts/merionctl run kindle-highlights <file>
```

Has a synthetic sample, so `--test` works.
These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
