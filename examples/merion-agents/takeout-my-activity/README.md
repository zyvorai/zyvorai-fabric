# takeout-my-activity

Google Takeout's `MyActivity.json` in, a usage summary out: which products you used (by entry count), what you searched or watched, and on which dates.

**Get the file:** takeout.google.com, choose **My Activity**, JSON format, and open the `MyActivity.json` for a product. The file is read as text: it counts the `header` and `title` fields and does not parse the JSON, so a very different layout may not match. This is a record of what you did online: handle the summary with the same care as the file.

```bash
./scripts/merionctl deploy examples/merion-agents/takeout-my-activity --test
./scripts/merionctl run takeout-my-activity <file>
```

Has a synthetic sample, so `--test` works.
These are personal files. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory. Do not promise more than that ([VENDORS.md](../../../docs/merion/VENDORS.md)).
