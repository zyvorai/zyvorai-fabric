// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// Identity adapter: verify the vendor's own login token (HS256 JWT here). Replace this file with your
// account system's verifier (OIDC, a session lookup, ...). The gateway only needs `{ sub, region? }`.

import { createHash, createHmac, timingSafeEqual } from "node:crypto";

const b64u = (buf) => Buffer.from(buf).toString("base64url");

/** Sign an HS256 JWT (used by tests and by vendors who issue their own). */
export function signJwt(claims, secret) {
  const head = b64u(JSON.stringify({ alg: "HS256", typ: "JWT" }));
  const body = b64u(JSON.stringify(claims));
  const sig = createHmac("sha256", secret).update(`${head}.${body}`).digest();
  return `${head}.${body}.${b64u(sig)}`;
}

/** Returns the claims, or throws. Refuses other algorithms (including "none") and expired tokens. */
export function verifyJwt(token, secret, now = Date.now()) {
  const parts = String(token ?? "").split(".");
  if (parts.length !== 3) throw new Error("malformed token");
  const [head, body, sig] = parts;
  const header = JSON.parse(Buffer.from(head, "base64url").toString());
  if (header.alg !== "HS256") throw new Error("unsupported algorithm");
  const want = createHmac("sha256", secret).update(`${head}.${body}`).digest();
  const got = Buffer.from(sig, "base64url");
  if (got.length !== want.length || !timingSafeEqual(got, want)) throw new Error("bad signature");
  const claims = JSON.parse(Buffer.from(body, "base64url").toString());
  if (typeof claims.sub !== "string" || claims.sub === "") throw new Error("no subject");
  if (typeof claims.exp === "number" && claims.exp * 1000 <= now) throw new Error("token expired");
  return claims;
}

/**
 * The runtime's user id is `[a-z0-9._-]`, at most 32 characters. A vendor account id rarely fits, so
 * map it: keep it when it already fits, otherwise a stable hash. Two accounts never share an id.
 */
export function toUserId(sub) {
  if (/^[a-z0-9][a-z0-9._-]{0,31}$/.test(sub)) return sub;
  return "u-" + createHash("sha256").update(sub).digest("hex").slice(0, 24);
}
