# Fabric benchmarks

Issue #14. Offline-safe harness for API latency and inventory scale.

```bash
# Unit tests (no daemon)
python3 -m unittest benchmarks.test_harness -v

# Against a running daemon
python3 benchmarks/harness.py --base-url http://127.0.0.1:9095 \
  --out benchmarks/baselines/local.json

# Token if auth is on
python3 benchmarks/harness.py --base-url http://127.0.0.1:9095 \
  --token "$TOKEN" --out benchmarks/baselines/local.json
```

Modes:

| Mode | What it measures |
|------|------------------|
| `health` | `GET /health` and `GET /readyz` p50/p99 |
| `inventory` | repeated `GET /api/vms` (or `--inventory-path`) |
| `concurrent` | thread pool against health |

FluxVM-dependent VM-start figures are **not** invented here. Pass `--label-fluxvm-dependent` when you add a custom start probe so published JSON stays honest.
