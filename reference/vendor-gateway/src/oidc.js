// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Pinned OIDC JWT verifier. Only the operator-configured JWKS URL is fetched; untrusted
// token headers (jku, x5u, kid) can never choose a network destination.
import { createPublicKey, verify as verifySignature } from "node:crypto";

const MAX_TOKEN = 16 * 1024;
const MAX_JWKS = 1024 * 1024;
const CACHE_MS = 5 * 60_000;
const SKEW_SECONDS = 60;

function parseSegment(segment) {
  if (!/^[A-Za-z0-9_-]+$/.test(segment)) throw new Error("malformed JWT segment");
  return JSON.parse(Buffer.from(segment, "base64url").toString("utf8"));
}

function pinnedUrl(value, allowLoopbackHttp) {
  const url = new URL(value);
  if (url.username || url.password || url.hash || url.search) throw new Error("JWKS URL must not contain credentials, query or fragment");
  if (url.protocol !== "https:" && !(allowLoopbackHttp && url.protocol === "http:" &&
    ["127.0.0.1", "[::1]", "localhost"].includes(url.hostname))) {
    throw new Error("JWKS URL must use HTTPS (or explicit loopback development mode)");
  }
  return url.href;
}

function pinnedIssuer(value, allowLoopbackHttp) {
  const url = new URL(value);
  if (url.username || url.password || url.search || url.hash ||
      (url.protocol !== "https:" && !(allowLoopbackHttp && url.protocol === "http:" &&
        ["127.0.0.1", "[::1]", "localhost"].includes(url.hostname)))) {
    throw new Error("OIDC issuer must use HTTPS (or explicit loopback development mode)");
  }
  return value; // exact configured spelling matters for the `iss` claim
}

async function limitedJson(response) {
  if (!response.ok) throw new Error(`JWKS returned HTTP ${response.status}`);
  const chunks = [];
  let size = 0;
  if (!response.body) throw new Error("empty JWKS response");
  for await (const part of response.body) {
    size += part.byteLength;
    if (size > MAX_JWKS) throw new Error("JWKS response too large");
    chunks.push(Buffer.from(part));
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

export class OidcVerifier {
  constructor({ issuer, audience, jwksUrl, allowLoopbackHttp = false },
    { fetchImpl = fetch, now = () => Date.now() } = {}) {
    if (typeof issuer !== "string" || !issuer || typeof audience !== "string" || !audience) {
      throw new Error("OIDC issuer and audience are required");
    }
    this.issuer = pinnedIssuer(issuer, allowLoopbackHttp);
    this.audience = audience;
    this.jwksUrl = pinnedUrl(jwksUrl, allowLoopbackHttp);
    this.fetch = fetchImpl;
    this.now = now;
    this.keys = new Map();
    this.expires = 0;
    this.refreshing = null;
    this.unknownKidRetryAfter = 0;
  }

  async refresh() {
    if (this.refreshing) return this.refreshing;
    this.refreshing = (async () => {
      const response = await this.fetch(this.jwksUrl, {
        redirect: "manual", signal: AbortSignal.timeout(5000),
        headers: { accept: "application/json" },
      });
      if (response.status >= 300 && response.status < 400) throw new Error("JWKS redirect refused");
      const jwks = await limitedJson(response);
      if (!Array.isArray(jwks.keys) || jwks.keys.length > 64) throw new Error("invalid JWKS keys");
      const next = new Map();
      for (const key of jwks.keys) {
        if (!key || typeof key.kid !== "string" || !key.kid || key.kid.length > 128 || next.has(key.kid)) {
          throw new Error("missing or duplicate JWKS key id");
        }
        if (key.d || (key.use && key.use !== "sig") || (key.key_ops && !key.key_ops.includes("verify"))) continue;
        if (key.kty !== "RSA" && !(key.kty === "EC" && key.crv === "P-256")) continue;
        next.set(key.kid, key);
      }
      this.keys = next;
      this.expires = this.now() + CACHE_MS;
    })().finally(() => { this.refreshing = null; });
    return this.refreshing;
  }

  async verify(token) {
    if (typeof token !== "string" || token.length > MAX_TOKEN) throw new Error("malformed token");
    const parts = token.split(".");
    if (parts.length !== 3) throw new Error("malformed token");
    const [head, body, signature] = parts;
    const header = parseSegment(head);
    if (!header || !["RS256", "ES256"].includes(header.alg) ||
      (header.typ !== undefined && header.typ !== "JWT") ||
      typeof header.kid !== "string" || !header.kid || header.kid.length > 128 ||
      header.crit !== undefined) throw new Error("unsupported JWT header");
    if (!/^[A-Za-z0-9_-]+$/.test(body) || !/^[A-Za-z0-9_-]+$/.test(signature)) throw new Error("malformed token");
    if (this.now() >= this.expires) await this.refresh();
    else if (!this.keys.has(header.kid)) {
      // One random kid per request must not turn the login endpoint into a JWKS fetch loop.
      if (this.now() < this.unknownKidRetryAfter) throw new Error("untrusted JWT key or algorithm");
      this.unknownKidRetryAfter = this.now() + 5_000;
      await this.refresh();
    }
    const jwk = this.keys.get(header.kid);
    if (!jwk || (jwk.alg && jwk.alg !== header.alg) ||
      (header.alg === "RS256" && jwk.kty !== "RSA") ||
      (header.alg === "ES256" && (jwk.kty !== "EC" || jwk.crv !== "P-256"))) {
      throw new Error("untrusted JWT key or algorithm");
    }
    const publicKey = createPublicKey({ key: jwk, format: "jwk" });
    const valid = verifySignature("sha256", Buffer.from(`${head}.${body}`),
      header.alg === "ES256" ? { key: publicKey, dsaEncoding: "ieee-p1363" } : publicKey,
      Buffer.from(signature, "base64url"));
    if (!valid) throw new Error("bad JWT signature");
    const claims = parseSegment(body);
    const now = this.now() / 1000;
    if (!claims || typeof claims.sub !== "string" || !claims.sub || claims.iss !== this.issuer ||
      !(claims.aud === this.audience || (Array.isArray(claims.aud) && claims.aud.includes(this.audience))) ||
      (Array.isArray(claims.aud) && claims.aud.length > 1 && claims.azp !== this.audience) ||
      typeof claims.exp !== "number" || !Number.isFinite(claims.exp) || claims.exp <= now - SKEW_SECONDS ||
      (claims.nbf !== undefined && (typeof claims.nbf !== "number" || claims.nbf > now + SKEW_SECONDS)) ||
      (claims.iat !== undefined && (typeof claims.iat !== "number" || claims.iat > now + SKEW_SECONDS))) {
      throw new Error("invalid JWT claims");
    }
    return claims;
  }
}
