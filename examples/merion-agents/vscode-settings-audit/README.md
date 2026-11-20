# vscode-settings-audit

A `settings.json` in, an audit out: every setting name that is set, the lines about telemetry, workspace trust and updates, saving and formatting, and setting **names** that look like secrets.

```bash
./scripts/merionctl deploy examples/merion-agents/vscode-settings-audit --test
./scripts/merionctl run vscode-settings-audit ~/Library/Application\ Support/Code/User/settings.json
```

VS Code settings are JSONC (comments allowed), so the file is read as text. **Values of secret-looking settings are not listed, but other lines are shown as written, so remove tokens before you share the file.** **The sample follows the documented layout of this output; it has not been checked against a real export**, so try the pack on your own file and adjust the patterns in `pack.json` if a label differs.

**Not this:** Merion does not connect to your accounts or tools. It reads a file you export or save, in a sealed Linux cell, and lists and counts what is in it. It is not a scanner or a replacement for the tool that produced the file.

The output can name people, repositories, apps and paths. The cell has no network (the run reports `0` outbound connections) and no model reads the file, but the evidence class is `software-test`: whoever operates the host could still read a cell's memory ([VENDORS.md](../../../docs/merion/VENDORS.md)). Merion secrets, tokens and private keys out of the files.
