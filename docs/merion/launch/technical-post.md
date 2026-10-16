# Your files should not have to leave your machine to be summarised

*Draft. Edit freely; check every number against a run before you publish.*

Most "summarise this" tools work by sending your file somewhere. That is a hard sell for a bank statement, a contract or a phone export.
Keep does the reading in a **sealed cell**: a throw-away microVM with no network, on a host you run. The file goes in, a summary comes
out, the cell is deleted, and every result says how many outbound connections the cell made. The answer is zero, and you can check it.

## What is actually in the cell

- A read-only extractor chosen by the use case (PDF text, Word, Excel, PowerPoint, HTML, mail, OCR for photos). The cell never runs
  code from the file and never gets a command from the pack.
- No network. The host applies a deny-all policy before the file is copied in, and the run fails closed and deletes the cell if the policy
  cannot be applied.
- No model, unless a use case declares one and your host allows it. The summaries are extractive: keywords, patterns, counts, tables.

## How to check it, not just believe it

```bash
./scripts/merion-e2e.sh                 # 40+ checks against a real cell
./scripts/merion-live-scenarios.sh      # every shipped use case, each in its own cell, with the egress count asserted
```

Both print the outbound-connection count for every run. The count comes from the host's egress broker, so it is not the cell's own word for it. Be precise about what it is: a cross-check. The guarantee itself is the deny-all network policy the host applies before the file enters the cell (and the run aborts if it cannot be applied); the count alone would not prove the cell had no network.

## What we do not claim

The evidence class is `software-test`. The cell has no network, but whoever operates the host could still read a cell's memory. Hardware
attestation (AMD SEV-SNP or Intel TDX) is designed but gated on a verified hardware run, and until that exists Keep does not say "confidential".
That is why you run the host yourself.

## Try it

Solvor is a Mac app for it: drop a file, get the answer, see the proof. Install steps and the honest list of what is and is not verified
are in the Solvor README. The Keep host is one command on a Linux machine with KVM (`scripts/merion-up.sh`; see the TODO for what is tested).

[TODO before posting: a screen recording, the release link, and a run of the two commands above on the machine you are quoting from.]
