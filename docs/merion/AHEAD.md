# Ahead of Muse — what this PR does and does not claim

Muse owns distribution. Merion owns the cell.

| Claim | This PR | Not this PR |
|---|---|---|
| Policy you can diff | `merion.policy.yaml` hashed into the receipt | A new policy language |
| 0 CONNECT on the extractive path | Scoreboard asserts `egress_connects: 0` and freeze-on-slip | A new dataplane |
| Two models, one policy | Distinct `model_sockets` on one policy hash | Muse Spark parity |
| Leave | Pack round-trip of policy, manifest, notes | Secrets in the pack |
| Honesty | `evidence_class: software-test`, hardware flags false, `operator_can_read: true` | SNP/TDX verified launch |

Run:

```bash
./scripts/merion-scoreboard.sh
cargo test --manifest-path agent-runtime/Cargo.toml scoreboard -- --nocapture
```

A receipt that sets `snp_launch_verified` or `evidence_class: confidential`
is rejected by `ScoreboardReceipt::validate`. That gate stays until one
hardware run flips the FluxVM flags.

**What `./scripts/merion-scoreboard.sh` actually proves.** It is a wiring check: it writes a policy,
packs it, unpacks it onto a second directory, hashes it, and asserts the resulting JSON matches
`ScoreboardReceipt`'s own validation rules — including the model-sockets count, which the script
writes itself rather than reading from a real deployed agent. It does not exercise the real egress
broker, a live cell, or a real model socket configuration. Treat it the same way
[VERIFICATION.md](VERIFICATION.md) treats the stub-cell suites: it proves the shape of the claim,
not isolation or a live measurement.
