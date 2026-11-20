// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import assert from "node:assert/strict";
import test from "node:test";
import { buildManifest, parseRule } from "../src/manifest.js";

const base = () => ({ template: "t", credential: [], allowHost: [], skill: [], taintTrust: [], rule: [], allowPrivateNetwork: false });

test("a deploy with no newer flags keeps the original manifest shape", () => {
  const manifest = buildManifest(base(), "node");
  for (const key of ["egress_mode", "home_volume", "resources", "skills", "skill_scope", "egress_approval_timeout_seconds"]) {
    assert.equal(key in manifest, false, key);
  }
  assert.equal(manifest.warm_pool_size, 0);
  assert.equal(manifest.runtime_port, 8080);
});

test("egress mode and approval timeout", () => {
  const m = buildManifest({ ...base(), egressMode: "sentinel", egressApprovalTimeout: "60" }, "node");
  assert.equal(m.egress_mode, "sentinel");
  assert.equal(m.egress_approval_timeout_seconds, 60);
  assert.throws(() => buildManifest({ ...base(), egressMode: "allow" }, "node"), /--egress-mode/);
  assert.throws(() => buildManifest({ ...base(), egressApprovalTimeout: "0" }, "node"), /positive integer/);
});

test("home volume flags", () => {
  assert.deepEqual(buildManifest({ ...base(), homeVolume: true }, "node").home_volume, {});
  assert.deepEqual(
    buildManifest({ ...base(), perUserHome: true, homeVolumeName: "browser-home", homePath: "/home/agent" }, "node").home_volume,
    { name: "browser-home", guest_path: "/home/agent", per_user: true },
  );
});

test("sandbox size needs both numbers", () => {
  const m = buildManifest({ ...base(), vcpus: "2", memoryMib: "7900" }, "node");
  assert.deepEqual(m.resources, { vcpus: 2, memory_mib: 7900 });
  assert.throws(() => buildManifest({ ...base(), vcpus: "2" }, "node"), /together/);
  assert.throws(() => buildManifest({ ...base(), vcpus: "x", memoryMib: "1" }, "node"), /positive integer/);
});

test("skills", () => {
  const m = buildManifest({ ...base(), skill: ["review", "lint@abc123"], skillScope: "team" }, "node");
  assert.deepEqual(m.skills, ["review", "lint@abc123"]);
  assert.equal(m.skill_scope, "team");
});

test("containment flags", () => {
  const off = buildManifest({ ...base(), confinement: "off" }, "node");
  assert.equal("confinement" in off, false);
  const m = buildManifest(
    { ...base(), confinement: "strict", dlp: true, taintTrust: ["docs.example.com"], rule: ["api.example.com:post:/v1/messages:1024"] },
    "node",
  );
  assert.equal(m.confinement, "strict");
  assert.equal(m.dlp, true);
  assert.deepEqual(m.taint, { trusted_hosts: ["docs.example.com"] });
  assert.deepEqual(m.egress_rules, [{ host: "api.example.com", methods: ["POST"], path_prefixes: ["/v1/messages"], max_body_bytes: 1024 }]);
  assert.deepEqual(buildManifest({ ...base(), taint: true }, "node").taint, { trusted_hosts: [] });
  assert.throws(() => buildManifest({ ...base(), confinement: "loose" }, "node"), /--confinement/);
});

test("rule specs", () => {
  assert.deepEqual(parseRule("api.example.com"), { host: "api.example.com" });
  assert.deepEqual(parseRule("api.example.com:GET,HEAD"), { host: "api.example.com", methods: ["GET", "HEAD"] });
  assert.throws(() => parseRule(":GET"), /host is required/);
  assert.throws(() => parseRule("h:GET::0"), /positive integer/);
});

test("confidential modes", () => {
  assert.equal("confidential" in buildManifest({ ...base(), confidential: "off" }, "node"), false);
  assert.equal(buildManifest({ ...base(), confidential: "auto" }, "node").confidential, "auto");
  assert.equal(buildManifest({ ...base(), confidential: "required" }, "node").confidential, "required");
  assert.throws(() => buildManifest({ ...base(), confidential: "yes" }, "node"), /--confidential/);
});

test("inner container", () => {
  assert.equal("inner_container" in buildManifest({ ...base(), innerContainer: "off" }, "node"), false);
  assert.equal(buildManifest({ ...base(), innerContainer: "strict" }, "node").inner_container, "strict");
  assert.throws(() => buildManifest({ ...base(), innerContainer: "x" }, "node"), /--inner-container/);
});

test("persistent and browser port", () => {
  const plain = buildManifest(base(), "node");
  assert.equal("persistent" in plain || "browser_port" in plain, false);
  const m = buildManifest({ ...base(), persistent: true, browserPort: "9222" }, "node");
  assert.equal(m.persistent, true);
  assert.equal(m.browser_port, 9222);
  assert.throws(() => buildManifest({ ...base(), browserPort: "0" }, "node"), /positive integer/);
});
