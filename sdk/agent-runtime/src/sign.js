// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import { createPrivateKey, createPublicKey, sign } from "node:crypto";

// PKCS#8 wrapper for a raw 32-byte Ed25519 seed. This is what lets Node sign
// with the same seed `merion_sign_policy` uses, with no Rust toolchain.
const PKCS8_ED25519_PREFIX = Buffer.from("302e020100300506032b657004220420", "hex");

export function seedFromHex(hex) {
  const clean = String(hex ?? "").trim();
  if (!/^[0-9a-fA-F]{64}$/.test(clean)) {
    throw new Error("the signer seed must be 32 bytes as 64 hex characters");
  }
  return Buffer.from(clean, "hex");
}

function privateKey(seedHex) {
  return createPrivateKey({
    key: Buffer.concat([PKCS8_ED25519_PREFIX, seedFromHex(seedHex)]),
    format: "der",
    type: "pkcs8",
  });
}

/** Ed25519 signature over the exact bytes, hex-encoded (what the runtime verifies). */
export function signBytes(bytes, seedHex) {
  return sign(null, Buffer.from(bytes), privateKey(seedHex)).toString("hex");
}

/** The public key to register as a trusted signer on the runtime. */
export function publicKeyHex(seedHex) {
  const der = createPublicKey(privateKey(seedHex)).export({ format: "der", type: "spki" });
  return Buffer.from(der).subarray(-32).toString("hex");
}
