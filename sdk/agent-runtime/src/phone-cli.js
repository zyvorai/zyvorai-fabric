#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// keep-phone: a stand-in phone for testing Keep's signed approvals.
//
//   keep-phone keygen <file> [p256|ed25519]            make a device key
//   keep-phone enrol  <file> <device-id> [--push-kind K --push-token T]
//                                                       print the enrolment body (your gateway POSTs it)
//   keep-phone decide <file> <device-id> <approval.json> <approved|denied>
//                                                       print the body that decides an inbox approval

import { readFileSync, writeFileSync } from "node:fs";
import { decisionBody, deviceFrom, enrolBody, generateDevice } from "./phone.js";

const [cmd, file, ...rest] = process.argv.slice(2);
const usage = () => {
  console.error("usage: keep-phone keygen <file> [p256|ed25519] | enrol <file> <device-id> [--push-kind K --push-token T] | decide <file> <device-id> <approval.json> <approved|denied>");
  process.exit(64);
};
if (!cmd || !file) usage();

const load = () => {
  const d = JSON.parse(readFileSync(file, "utf8"));
  return deviceFrom(d.alg, d.private_key_pem);
};

if (cmd === "keygen") {
  const d = generateDevice(rest[0] ?? "p256");
  writeFileSync(file, JSON.stringify({ alg: d.alg, private_key_pem: d.privateKeyPem, public_key: d.publicKey }, null, 2), { mode: 0o600 });
  console.log(`wrote ${file} (${d.alg}); public key: ${d.publicKey}`);
} else if (cmd === "enrol") {
  const [deviceId, ...flags] = rest;
  if (!deviceId) usage();
  const at = (name) => flags[flags.indexOf(name) + 1];
  const push = flags.includes("--push-kind") ? { kind: at("--push-kind"), token: at("--push-token") } : undefined;
  console.log(JSON.stringify(enrolBody(load(), deviceId, push)));
} else if (cmd === "decide") {
  const [deviceId, approvalFile, decision] = rest;
  if (!deviceId || !approvalFile || !decision) usage();
  const approval = JSON.parse(readFileSync(approvalFile, "utf8"));
  console.log(JSON.stringify(decisionBody(load(), deviceId, approval, decision)));
} else usage();
