# bookmarks-digest

A bookmarks export in, a tidy-up view out: the sites you bookmark most, the folders and the titles. It reads the Netscape bookmarks HTML that Safari, Chrome, Edge and Firefox all export.

```bash
./scripts/merionctl deploy examples/merion-agents/bookmarks-digest --test
```

Export from the browser's bookmarks menu (Safari: File, Export, Bookmarks). The upload limit is 300 KB, so a very large collection needs splitting. It lists and counts; it does not check whether a link still works. **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
