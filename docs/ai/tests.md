# AI tests (fabric)

_Generated 2026-10-20 by claude-opus-5-5 from a snapshot of tracked sources; verify before acting._

# Unit tests for the 8 most valuable untested behaviors in "fabric"

## What I could and could not test

Almost all of the input is documentation. Very little executable source is shown.

- The real logic (`agent-runtime/src/*.rs`, `sdk/agent-runtime/src/*.js`, `fluxvm-client`, `input-guard`, the agent `agent.ts` files) is **not shown**. I have not invented APIs for it.
- I picked the 8 most valuable behaviors that the shown files do define:
  - security-relevant config contracts
  - the `build.sh` CLI
  - the Python SDK package surface
  - the web entry aliases

These are the kind of things that regress silently. A secret default leaking into the prod compose file is one example. A Trivy allowlist that widens is another.

I used the repo's own frameworks:
- Python `unittest` (stdlib only), the same as `benchmarks.test_harness` and `examples.devops.test_contract`.
- Vitest for `web/`, the same as `src/api/__tests__/api-files.test.ts`.

**Assumptions are flagged at the top of each test file.**

| # | Unit / behavior | Why it matters |
|---|---|---|
| 1 | `deploy-op/keep.policy.yaml` | A packaged agent's policy must stay deny-by-default and read-only (Sentinel contract) |
| 2 | `docker-compose.prod.example.yml` | No baked-in secrets, a pinned image tag, and TLS health probes |
| 3 | `docker-compose.yml` (eval) | Port override is wired consistently; health checks both `/health` and `/readyz` |
| 4 | `trivy-secret.yaml` allowlist regex | The exemption must cover exactly one file and nothing else |
| 5 | `deny.toml` | Licence allowlist has no copyleft; unknown registries are denied; ignore IDs are well formed |
| 6 | `build.sh` argument parsing | `--help` and bad flags exit before any build |
| 7 | `sdk/python/fabricctl` package surface | `__version__` and the `ZyvorFabricClient` export |
| 8 | Web aliases | `API_BASE_URL`, `Login → SignIn`, `ui/WizardStepper` re-exports |

---

## 1. `tests/repo_contracts/test_keep_policy_deploy_op.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Contract: the deploy-op pack's Keep policy stays deny-by-default and read-only.

deploy-op's README promises "No host writes by default". The Sentinel docs say
`default_egress: deny`, ask levels always|first|never, and the taint guard
`block_egress_until_ask`. This test pins those properties on the shipped file.

Assumption: stdlib only (no PyYAML). The file is a small flat YAML, so it is
checked line by line.
"""
import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
POLICY = REPO_ROOT / "examples" / "keep-agents" / "deploy-op" / "keep.policy.yaml"

WRITE_METHODS = ("POST", "PUT", "PATCH", "DELETE")


class DeployOpPolicyTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = POLICY.read_text(encoding="utf-8")
        cls.lines = [l.rstrip() for l in cls.text.splitlines()]

    def test_version_is_1(self):
        self.assertIn("version: 1", self.lines)

    def test_default_egress_is_deny(self):
        self.assertIn("default_egress: deny", self.lines)
        self.assertNotRegex(self.text, r"default_egress:\s*allow")

    def test_no_write_methods_anywhere(self):
        for line in self.lines:
            m = re.search(r"methods:\s*\[(.*)\]", line)
            if not m:
                continue
            methods = [x.strip().upper() for x in m.group(1).split(",") if x.strip()]
            self.assertTrue(methods, "an empty methods list widens to any method")
            for w in WRITE_METHODS:
                self.assertNotIn(w, methods, f"write method {w} in read-only pack policy")

    def test_ask_values_are_valid(self):
        asks = re.findall(r"ask:\s*(\S+)", self.text)
        self.assertTrue(asks)
        for a in asks:
            self.assertIn(a, {"always", "first", "never"})

    def test_ask_never_only_on_get_rules(self):
        # Split the allow section into rule blocks starting at "- host:".
        allow_section = self.text.split("allow:", 1)[1].split("deny:", 1)[0]
        rules = [r for r in re.split(r"\n\s*-\s+", "\n" + allow_section) if "host:" in r]
        self.assertTrue(rules, "expected at least one allow rule")
        for rule in rules:
            if re.search(r"ask:\s*never", rule):
                self.assertRegex(rule, r"methods:\s*\[\s*GET\s*\]",
                                 "ask: never is only acceptable on GET-only rules")

    def test_allow_hosts_are_loopback_only(self):
        allow_section = self.text.split("allow:", 1)[1].split("deny:", 1)[0]
        hosts = re.findall(r'host:\s*"?([^"\s]+)"?', allow_section)
        self.assertEqual(hosts, ["127.0.0.1"])
        for h in hosts:
            self.assertNotIn("*", h, "wildcard host in allow list")

    def test_onion_denied(self):
        deny_section = self.text.split("deny:", 1)[1]
        self.assertRegex(deny_section, r'host:\s*"\*\.onion"')

    def test_taint_guard_present(self):
        self.assertRegex(self.text, r"on_untrusted_page:\s*block_egress_until_ask")


if __name__ == "__main__":
    unittest.main()
```

## 2. `tests/repo_contracts/test_compose_prod_example.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Contract: the production compose example has no baked-in secrets, requires
pinned images, and probes over HTTPS.

Assumption: stdlib only. Service blocks are extracted by indentation, matching
the 2-space layout of the shipped file.
"""
import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
COMPOSE = REPO_ROOT / "docker-compose.prod.example.yml"


def service_block(text: str, name: str) -> str:
    out, in_services, current = [], False, None
    for line in text.splitlines():
        if line.startswith("services:"):
            in_services = True
            continue
        if not in_services:
            continue
        if line and not line.startswith((" ", "#")):
            break  # next top-level key (e.g. volumes:)
        m = re.match(r"^  ([A-Za-z0-9_.-]+):\s*$", line)
        if m:
            current = m.group(1)
            continue
        if current == name:
            out.append(line)
    if not out:
        raise AssertionError(f"service {name!r} not found")
    return "\n".join(out)


class ProdComposeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = COMPOSE.read_text(encoding="utf-8")
        cls.fabricd = service_block(cls.text, "zyvor-fabricd")
        cls.fluxvm = service_block(cls.text, "fluxvm")

    def test_secrets_are_required_not_defaulted(self):
        for var in ("ZYVOR_FABRICD_ADMIN_PASSWORD", "ZYVOR_FABRICD_JWT_SECRET"):
            self.assertRegex(self.fabricd, rf"{var}:\s*\$\{{{var}:\?",
                             f"{var} must use ${{VAR:?...}} so compose fails when unset")
            self.assertNotRegex(self.text, rf"\$\{{{var}:-",
                                f"{var} must not have a default value")

    def test_eval_password_not_present(self):
        self.assertNotIn("eval-admin-only", self.text)

    def test_images_require_registry_and_tag_and_avoid_latest(self):
        images = re.findall(r"^\s*image:\s*(\S+)", self.text, flags=re.M)
        self.assertEqual(len(images), 2)
        for img in images:
            self.assertIn("${REGISTRY:?", img)
            self.assertIn("${TAG:?", img)
            self.assertNotRegex(img, r":latest\b")

    def test_no_build_from_source_in_prod(self):
        self.assertNotRegex(self.text, r"^\s*build:", "prod must use prebuilt images")

    def test_healthcheck_uses_https_and_both_probes(self):
        hc = re.search(r"test:\s*\[(.*)\]", self.fabricd).group(1)
        self.assertIn("https://localhost:${ZYVOR_FABRICD_PORT:-9095}/health", hc)
        self.assertIn("https://localhost:${ZYVOR_FABRICD_PORT:-9095}/readyz", hc)
        self.assertNotIn("http://localhost", hc)

    def test_prod_config_mounted_read_only(self):
        self.assertIn(
            "./configs/zyvor-fabricd-prod.toml:/etc/zyvor-fabricd/zyvor-fabricd.toml:ro",
            self.fabricd,
        )

    def test_listen_port_overridable(self):
        self.assertIn('ZYVOR_FABRICD_LISTEN: "0.0.0.0:${ZYVOR_FABRICD_PORT:-9095}"', self.fabricd)

    def test_fluxvm_has_kvm_and_fabricd_depends_on_it(self):
        self.assertIn("- /dev/kvm", self.fluxvm)
        self.assertRegex(self.fabricd, r"depends_on:\s*\n\s+fluxvm:")


if __name__ == "__main__":
    unittest.main()
```

## 3. `tests/repo_contracts/test_compose_eval.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Contract: the eval compose profile wires the port override consistently.

Checked properties:
- ZYVOR_FABRICD_PORT drives the listen address and both health probes.
- Both services use host networking, which the comments explain is required.
- FluxVM gets /dev/kvm and its dataplane config read-only.
"""
import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
COMPOSE = REPO_ROOT / "docker-compose.yml"
PORT_EXPR = "${ZYVOR_FABRICD_PORT:-9095}"


def service_block(text: str, name: str) -> str:
    out, in_services, current = [], False, None
    for line in text.splitlines():
        if line.startswith("services:"):
            in_services = True
            continue
        if not in_services:
            continue
        if line and not line.startswith((" ", "#")):
            break
        m = re.match(r"^  ([A-Za-z0-9_.-]+):\s*$", line)
        if m:
            current = m.group(1)
            continue
        if current == name:
            out.append(line)
    if not out:
        raise AssertionError(f"service {name!r} not found")
    return "\n".join(out)


class EvalComposeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = COMPOSE.read_text(encoding="utf-8")
        cls.fabricd = service_block(cls.text, "zyvor-fabricd")
        cls.fluxvm = service_block(cls.text, "fluxvm")

    def test_listen_uses_port_override(self):
        self.assertIn(f'ZYVOR_FABRICD_LISTEN: "0.0.0.0:{PORT_EXPR}"', self.fabricd)

    def test_healthcheck_follows_port_override_for_both_probes(self):
        hc = re.search(r"test:\s*\[(.*)\]", self.fabricd).group(1)
        self.assertIn(f"http://localhost:{PORT_EXPR}/health", hc)
        self.assertIn(f"http://localhost:{PORT_EXPR}/readyz", hc)
        # A hard-coded :9095 here would break ZYVOR_FABRICD_PORT=19095.
        self.assertNotRegex(hc, r"localhost:9095")

    def test_both_services_use_host_network(self):
        self.assertIn("network_mode: host", self.fabricd)
        self.assertIn("network_mode: host", self.fluxvm)

    def test_eval_password_is_the_documented_placeholder(self):
        self.assertIn(
            "ZYVOR_FABRICD_ADMIN_PASSWORD: ${ZYVOR_FABRICD_ADMIN_PASSWORD:-eval-admin-only}",
            self.fabricd,
        )

    def test_fluxvm_kvm_and_dataplane_config(self):
        self.assertIn("- /dev/kvm", self.fluxvm)
        self.assertIn("./configs/fluxvm-dataplane.toml:/etc/fluxvm.toml:ro", self.fluxvm)
        self.assertIn('"--config", "/etc/fluxvm.toml"', self.fluxvm)

    def test_fluxvm_memlock_unlimited_for_ebpf(self):
        self.assertRegex(self.fluxvm, r"memlock:\s*\n\s+soft:\s*-1\s*\n\s+hard:\s*-1")

    def test_fabricd_depends_on_fluxvm(self):
        self.assertRegex(self.fabricd, r"depends_on:\s*\n\s+fluxvm:")


if __name__ == "__main__":
    unittest.main()
```

## 4. `tests/repo_contracts/test_trivy_secret_allowlist.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Contract: the Trivy secret-scan exemption covers exactly memory.rs.

The config comment says "Allow only that file; every other path is still
scanned". This test compiles the shipped regex and probes near-miss paths.

Assumption: Trivy uses Go RE2. The pattern here (anchors, literal text,
escaped dot) has identical semantics in Python's `re`.
"""
import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
CONFIG = REPO_ROOT / "trivy-secret.yaml"


class TrivyAllowlistTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = CONFIG.read_text(encoding="utf-8")
        paths = re.findall(r"^\s*path:\s*(\S+)\s*$", cls.text, flags=re.M)
        cls.paths = paths
        cls.rx = re.compile(paths[0]) if paths else None

    def test_exactly_one_allow_rule(self):
        self.assertEqual(len(re.findall(r"^\s*-\s*id:", self.text, flags=re.M)), 1)
        self.assertEqual(len(self.paths), 1)

    def test_rule_is_path_scoped(self):
        # A rule without a path would silence the finding repo-wide.
        self.assertIsNotNone(self.rx)
        self.assertTrue(self.paths[0].startswith("^"))
        self.assertTrue(self.paths[0].endswith("$"))

    def test_matches_the_intended_file(self):
        self.assertTrue(self.rx.search("agent-runtime/src/memory.rs"))

    def test_rejects_near_misses(self):
        for p in [
            "agent-runtime/src/memory.rs.bak",
            "agent-runtime/src/memoryXrs",          # unescaped dot would match this
            "agent-runtime/src/memory_rs",
            "x/agent-runtime/src/memory.rs",        # unanchored start would match this
            "agent-runtime/src/memory/mod.rs",
            "agent-runtime/src/receipts.rs",
            "backend/zyvor-fabricd/src/memory.rs",
            "agent-runtime/src/memory.rs/../../.env",
        ]:
            with self.subTest(path=p):
                self.assertIsNone(self.rx.search(p))

    def test_rule_has_description(self):
        self.assertRegex(self.text, r"description:\s*\S+")


if __name__ == "__main__":
    unittest.main()
```

## 5. `tests/repo_contracts/test_cargo_deny_policy.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Contract for deny.toml (cargo-deny v2): supply-chain policy invariants.

Assumption: `tomllib` (Python 3.11+). On older interpreters the suite is skipped.
"""
import re
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DENY = REPO_ROOT / "deny.toml"

try:
    import tomllib  # type: ignore[import-not-found]
except ModuleNotFoundError:  # pragma: no cover
    tomllib = None

COPYLEFT = ("GPL", "AGPL", "LGPL", "SSPL", "EUPL", "CDDL")


@unittest.skipIf(tomllib is None, f"tomllib needs Python 3.11+, have {sys.version}")
class CargoDenyPolicyTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with DENY.open("rb") as fh:
            cls.cfg = tomllib.load(fh)

    def test_schema_versions_are_v2(self):
        self.assertEqual(self.cfg["advisories"]["version"], 2)
        self.assertEqual(self.cfg["licenses"]["version"], 2)

    def test_advisory_ignores_are_wellformed_and_unique(self):
        ids = self.cfg["advisories"]["ignore"]
        self.assertEqual(len(ids), len(set(ids)), "duplicate RUSTSEC ignore")
        for i in ids:
            with self.subTest(id=i):
                self.assertRegex(i, r"^RUSTSEC-\d{4}-\d{4}$")

    def test_no_copyleft_license_allowed(self):
        for lic in self.cfg["licenses"]["allow"]:
            for bad in COPYLEFT:
                with self.subTest(license=lic):
                    self.assertNotIn(bad, lic.upper())

    def test_project_license_is_allowed(self):
        allow = self.cfg["licenses"]["allow"]
        self.assertIn("Apache-2.0", allow)
        self.assertIn("MIT", allow)

    def test_confidence_threshold_sane(self):
        t = self.cfg["licenses"]["confidence-threshold"]
        self.assertGreaterEqual(t, 0.8)
        self.assertLessEqual(t, 1.0)

    def test_unknown_registry_denied_and_no_git_sources_allowed(self):
        src = self.cfg["sources"]
        self.assertEqual(src["unknown-registry"], "deny")
        self.assertEqual(src["allow-git"], [])


if __name__ == "__main__":
    unittest.main()
```

## 6. `tests/repo_contracts/test_build_sh_cli.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""build.sh argument handling: --help and unknown flags must exit *before*
any cargo/npm invocation.

Assumption: bash is on PATH (POSIX hosts). Skipped otherwise.
"""
import os
import shutil
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
BUILD_SH = REPO_ROOT / "build.sh"
BASH = shutil.which("bash")


def run(*args, timeout=15):
    # Strip cargo/npm from PATH, so that if parsing were broken and a build
    # started, the test would fail fast instead of building the workspace.
    env = dict(os.environ, PATH=os.path.dirname(BASH))
    return subprocess.run([BASH, str(BUILD_SH), *args], capture_output=True,
                          text=True, timeout=timeout, env=env)


@unittest.skipIf(BASH is None, "bash not available")
class BuildShCliTest(unittest.TestCase):
    def test_help_exits_zero_with_usage(self):
        for flag in ("--help", "-h"):
            with self.subTest(flag=flag):
                r = run(flag)
                self.assertEqual(r.returncode, 0, r.stderr)
                self.assertIn("Usage:", r.stdout)
                self.assertIn("--backend|--web", r.stdout)
                self.assertNotIn("Build complete", r.stdout)

    def test_help_after_other_flags_still_exits_before_building(self):
        r = run("--backend", "--debug", "--help")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Usage:", r.stdout)
        self.assertNotIn("Building backend", r.stdout)

    def test_unknown_option_fails_with_message(self):
        r = run("--bogus")
        self.assertEqual(r.returncode, 1)
        self.assertIn("Unknown option: --bogus", r.stdout)
        self.assertNotIn("Build complete", r.stdout)

    def test_unknown_option_after_valid_flag_fails(self):
        r = run("--web", "--nope")
        self.assertEqual(r.returncode, 1)
        self.assertIn("Unknown option: --nope", r.stdout)


if __name__ == "__main__":
    unittest.main()
```

## 7. `sdk/python/tests/test_package.py`

```python
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Public surface of the fabricctl Python SDK package.

Assumptions (flagged):
- sdk/python/fabricctl/client.py exists and defines ZyvorFabricClient.
  __init__.py imports it, but client.py is not shown.
- Importing client.py needs no running daemon. If it needs a third-party
  dependency (e.g. `requests`) that is not installed, the suite is skipped
  rather than failing.
"""
import re
import sys
import unittest
from pathlib import Path

SDK_ROOT = Path(__file__).resolve().parents[1]
if str(SDK_ROOT) not in sys.path:
    sys.path.insert(0, str(SDK_ROOT))

try:
    import fabricctl  # noqa: E402
    IMPORT_ERROR = None
except ModuleNotFoundError as e:  # missing third-party dep of client.py
    fabricctl = None
    IMPORT_ERROR = e


@unittest.skipIf(fabricctl is None, f"fabricctl import failed: {IMPORT_ERROR}")
class FabricctlPackageTest(unittest.TestCase):
    def test_version(self):
        self.assertEqual(fabricctl.__version__, "0.1.0")
        self.assertRegex(fabricctl.__version__, r"^\d+\.\d+\.\d+$")

    def test_client_is_exported_at_top_level(self):
        self.assertTrue(hasattr(fabricctl, "ZyvorFabricClient"))
        self.assertTrue(isinstance(fabricctl.ZyvorFabricClient, type))

    def test_client_is_same_object_as_submodule(self):
        from fabricctl.client import ZyvorFabricClient
        self.assertIs(fabricctl.ZyvorFabricClient, ZyvorFabricClient)


class InitSourceTest(unittest.TestCase):
    """Runs even when client.py's dependencies are missing."""

    def test_init_declares_version_and_export(self):
        src = (SDK_ROOT / "fabricctl" / "__init__.py").read_text(encoding="utf-8")
        self.assertRegex(src, r'__version__\s*=\s*"0\.1\.0"')
        self.assertTrue(re.search(r"from \.client import ZyvorFabricClient", src))


if __name__ == "__main__":
    unittest.main()
```

## 8. `web/src/__tests__/entry-aliases.test.ts`

```ts
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0
//
// Contract for thin alias modules and the API base path.
//
// Assumptions (flagged): `web/src/pages/SignIn` and
// `web/src/components/WizardStepper` exist with default exports. Both alias
// files re-export them, but neither is shown. The existing Vitest setup
// (environment, path resolution) handles .tsx page imports.
import { describe, expect, it } from 'vitest'

import { API_BASE_URL } from '../api/config'
import Login from '../pages/Login'
import SignIn from '../pages/SignIn'
import UiWizardStepper from '../components/ui/WizardStepper'
import WizardStepper from '../components/WizardStepper'

describe('API_BASE_URL', () => {
  it('is the relative /api prefix proxied to fabricd', () => {
    expect(API_BASE_URL).toBe('/api')
  })

  it('has no trailing slash, so `${API_BASE_URL}/vms` is well formed', () => {
    expect(API_BASE_URL.endsWith('/')).toBe(false)
    expect(`${API_BASE_URL}/vms`).toBe('/api/vms')
  })

  it('is relative, so tokens are never sent to another origin', () => {
    expect(API_BASE_URL.startsWith('/')).toBe(true)
    expect(API_BASE_URL).not.toMatch(/^[a-z]+:\/\//i)
  })
})

describe('alias modules', () => {
  it('Login re-exports SignIn as default', () => {
    expect(Login).toBeDefined()
    expect(Login).toBe(SignIn)
  })

  it('components/ui/WizardStepper re-exports components/WizardStepper', () => {
    expect(UiWizardStepper).toBeDefined()
    expect(UiWizardStepper).toBe(WizardStepper)
  })
})
```

---

## How to run

**Python contract tests (1–6).** These use the stdlib only; `test_cargo_deny_policy.py` needs Python 3.11+ for `tomllib`. Run from the repo root:

```bash
touch tests/repo_contracts/__init__.py
python3 -m unittest discover -s tests/repo_contracts -t . -v
```

**Python SDK (7):**

```bash
python3 -m unittest discover -s sdk/python/tests -v
```

**Web (8):**

```bash
cd web && npx vitest run src/__tests__/entry-aliases.test.ts
```

All of them are offline and need no daemon, KVM or FluxVM. They fit next to the existing `make test` / `ci.yml` jobs.

## Gaps worth covering next

These are the highest-value units, but I could not test them without seeing their code:
- `agent-runtime` Sentinel risk check (`default_egress_allow`, `methods_widened`, …)
- Receipts idempotency (`receipts::tests` already exists)
- The `l7::scan` secret detector
- `input-guard`
- The `calendar-suggestions` / `price-watch` decision functions (`needsPrepReminder`, `needsAPlace`, `findSuggestions`, `needsAlert`)
