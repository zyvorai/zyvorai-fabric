# Show HN draft

**Title:** Show HN: Merion, read your files in a network-less microVM and see the proof (open source)

**Body**

I built Merion because I wanted to summarise bank statements, contracts and phone exports without uploading them anywhere. Each file is read
inside a throw-away microVM with no network, on a host you run. The result page shows the number of outbound connections the cell made
(zero), counted by the host's egress broker rather than by the cell.

It is extractive on purpose: keywords, patterns, tables, counts. No model reads your file unless a use case declares one and your host
allows it. There are 60+ use cases as declarative JSON packs (statements, chat exports, logs, decks, receipts and bills from photos via
OCR, bank operations files) and you can add your own.

Solvor is the Mac app (macOS 26): drop a file, get the answer, see the proof. There is also an email-from-your-browser flow with redaction
and a preview, and approvals signed with Touch ID.

What it is not: not confidential computing yet. The evidence class is `software-test`, meaning whoever runs the host could read cell
memory. Hardware attestation is designed but not verified. The Solvor build is ad-hoc signed for now.

Repo: https://github.com/zyvorai/fabric · Mac app: https://github.com/zyvorai/solvor

## Questions to be ready for

- **Why not just run a local model?** You can, as a use case's model step; the point here is proving the reading step has no network.
- **How do I know the count is honest?** The host counts CONNECT attempts at the egress broker, and `merion-e2e.sh` asserts it. But the count is a cross-check: the guarantee is the deny-all policy applied on the host before the file enters the cell, and the run fails closed if it cannot be applied.
- **Can the operator see my file?** Yes (`software-test`). That is why the host is yours.
- **Windows or Linux client?** Not yet. The API is plain HTTP; a client mirrors the small `MerionKit` library.
- **Cost / hosted version?** No hosted service; self-hosted only.
