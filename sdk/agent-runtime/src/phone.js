// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// A reference phone client for Keep approvals, in Node.
//
// It does what a phone app does: hold a private key, take an approval from the inbox, build the exact
// text the runtime asks it to sign, sign it, and produce the request body that decides the approval.
// The runtime rebuilds the same text and checks the signature against the public key that was enrolled
// for the user. Keys: ECDSA P-256 (what Android Keystore holds) or Ed25519.

import { createHmac, createPrivateKey, createPublicKey, generateKeyPairSync, sign, timingSafeEqual, verify } from "node:crypto";

export const PAYLOAD_FORMAT_LEGACY = "keep-approval-v1";
export const PAYLOAD_FORMAT = "merion-approval-v1";

/**
 * The format to sign: `merion-approval-v1` when the host lists it in `sign.formats`, otherwise whatever
 * `sign.format` announces (older hosts only know the legacy name).
 */
export function pickFormat(sign) {
  if (Array.isArray(sign?.formats) && sign.formats.includes(PAYLOAD_FORMAT)) return PAYLOAD_FORMAT;
  return sign?.format ?? PAYLOAD_FORMAT_LEGACY;
}

/** A new device key. `privateKeyPem` stays on the phone; `publicKey` (base64) is what gets enrolled. */
export function generateDevice(alg = "p256") {
  if (alg !== "p256" && alg !== "ed25519") throw new Error(`unsupported alg ${alg}`);
  const { privateKey } = generateKeyPairSync(alg === "p256" ? "ec" : "ed25519", alg === "p256" ? { namedCurve: "P-256" } : {});
  return deviceFrom(alg, privateKey.export({ type: "pkcs8", format: "pem" }));
}

export function deviceFrom(alg, privateKeyPem) {
  const publicKey = createPublicKey(createPrivateKey(privateKeyPem))
    .export({ type: "spki", format: "der" })
    .toString("base64");
  return { alg, privateKeyPem, publicKey };
}

/** The body for `POST /v1/users/{id}/devices`. The operator (your gateway) sends it, not the phone. */
export function enrolBody(device, deviceId, push) {
  return { device_id: deviceId, alg: device.alg, public_key: device.publicKey, ...(push ? { push } : {}) };
}

/**
 * The exact text to sign. `approval` is an item from `GET /v1/inbox` (`pending_approvals[i]`), which
 * carries `id`, `kind`, `subject` and `sign`.
 */
export function signingPayload(approval, decision, sign = approval.sign, format = pickFormat(sign)) {
  if (decision !== "approved" && decision !== "denied") throw new Error("decision must be approved or denied");
  return (
    `${format}\n` +
    `approval: ${approval.id}\n` +
    `decision: ${decision}\n` +
    `kind: ${approval.kind}\n` +
    `subject: ${approval.subject ?? ""}\n` +
    `action-sha256: ${sign.action_sha256}\n` +
    `challenge: ${sign.challenge}\n` +
    `expires: ${sign.expires_at}\n`
  );
}

/** Sign a decision; returns base64 (DER for P-256, 64 raw bytes for Ed25519). */
export function signDecision(device, approval, decision, sign = approval.sign, format = pickFormat(sign)) {
  const payload = Buffer.from(signingPayload(approval, decision, sign, format), "utf8");
  const key = createPrivateKey(device.privateKeyPem);
  const sig =
    device.alg === "p256"
      ? sign_(payload, key, { dsaEncoding: "der" })
      : sign_(payload, key);
  return sig.toString("base64");
}

// `sign` is shadowed by the parameter name above, so keep a handle on the crypto function.
function sign_(payload, key, opts) {
  return opts ? sign("sha256", payload, { key, ...opts }) : sign(null, payload, key);
}

/** The body for `POST /v1/approvals/{id}`. */
export function decisionBody(device, deviceId, approval, decision) {
  return { decision, device_id: deviceId, signature: signDecision(device, approval, decision) };
}

/** Check a signature made by a device (what the runtime does; useful in tests and relays). */
export function verifyDecision(publicKeyB64, alg, payload, signatureB64) {
  let der = Buffer.from(publicKeyB64, "base64");
  // The runtime also accepts a raw 32-byte Ed25519 key; wrap it in the SubjectPublicKeyInfo header.
  if (alg === "ed25519" && der.length === 32) der = Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), der]);
  const key = createPublicKey({ key: der, format: "der", type: "spki" });
  const data = Buffer.from(payload, "utf8");
  const sig = Buffer.from(signatureB64, "base64");
  return alg === "p256" ? verify("sha256", data, { key, dsaEncoding: "der" }, sig) : verify(null, data, key, sig);
}

/** Check the HMAC on a message from the runtime to a push relay (`x-zyvor-signature: sha256=<hex>`). */
export function verifyRelaySignature(secret, body, header) {
  const want = createHmac("sha256", secret).update(body).digest();
  const got = Buffer.from((header ?? "").replace(/^sha256=/, ""), "hex");
  return got.length === want.length && timingSafeEqual(got, want);
}
