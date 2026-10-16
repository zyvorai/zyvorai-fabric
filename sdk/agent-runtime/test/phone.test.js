// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { readFileSync } from "node:fs";
import { decisionBody, generateDevice, signDecision, signingPayload, verifyDecision, verifyRelaySignature } from "../src/phone.js";

// The runtime's test suite writes these; both sides must agree on them.
const vectors = JSON.parse(readFileSync(new URL("../../../docs/merion/mobile/test-vectors.json", import.meta.url), "utf8"));
const approval = { ...vectors.approval, sign: vectors.sign };

test("the payload matches the runtime's byte for byte", () => {
  for (const c of vectors.cases) {
    assert.equal(signingPayload(approval, c.decision), c.payload, `${c.alg} ${c.decision}`);
  }
});

test("signatures made by the runtime's test keys verify here", () => {
  for (const c of vectors.cases) {
    assert.equal(verifyDecision(c.public_key, c.alg, c.payload, c.signature), true, `${c.alg} ${c.decision}`);
    // and not for the other decision
    const other = c.decision === "approved" ? "denied" : "approved";
    assert.equal(verifyDecision(c.public_key, c.alg, signingPayload(approval, other), c.signature), false);
  }
});

test("a signature this phone makes verifies, and a changed decision does not", () => {
  for (const alg of ["p256", "ed25519"]) {
    const dev = generateDevice(alg);
    const sig = signDecision(dev, approval, "approved");
    assert.equal(verifyDecision(dev.publicKey, alg, signingPayload(approval, "approved"), sig), true, alg);
    assert.equal(verifyDecision(dev.publicKey, alg, signingPayload(approval, "denied"), sig), false, alg);
    const body = decisionBody(dev, "ana-phone", approval, "denied");
    assert.deepEqual(Object.keys(body).sort(), ["decision", "device_id", "signature"]);
  }
});

test("only approved or denied can be signed", () => {
  assert.throws(() => signingPayload(approval, "maybe"), /approved or denied/);
});

test("relay messages are checked against their HMAC", () => {
  const body = '{"event":"approval.requested"}';
  const header = "sha256=" + createHmac("sha256", "s3cret").update(body).digest("hex");
  assert.equal(verifyRelaySignature("s3cret", body, header), true);
  assert.equal(verifyRelaySignature("other", body, header), false);
  assert.equal(verifyRelaySignature("s3cret", body + " ", header), false);
  assert.equal(verifyRelaySignature("s3cret", body, undefined), false);
});
