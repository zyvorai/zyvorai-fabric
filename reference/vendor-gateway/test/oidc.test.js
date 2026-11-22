// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { generateKeyPairSync, sign as signBytes } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { OidcVerifier } from "../src/oidc.js";
import { createGateway } from "../src/gateway.js";

const ISSUER = "https://login.example/realms/zyvor";
const AUDIENCE = "keep-phone";
const rsa = generateKeyPairSync("rsa", { modulusLength: 2048 });
const rotated = generateKeyPairSync("rsa", { modulusLength: 2048 });
const ec = generateKeyPairSync("ec", { namedCurve: "prime256v1" });

function publicJwk(pair, kid, alg) {
  return { ...pair.publicKey.export({ format: "jwk" }), kid, use: "sig", alg };
}

function jwt(pair, kid, claims = {}, alg = "RS256", header = {}) {
  const h = Buffer.from(JSON.stringify({ alg, kid, typ: "JWT", ...header })).toString("base64url");
  const b = Buffer.from(JSON.stringify({ sub: "ana", iss: ISSUER, aud: AUDIENCE,
    exp: Math.floor(Date.now() / 1000) + 600, ...claims })).toString("base64url");
  const bytes = Buffer.from(`${h}.${b}`);
  const signature = signBytes("sha256", bytes, alg === "ES256" ?
    { key: pair.privateKey, dsaEncoding: "ieee-p1363" } : pair.privateKey);
  return `${h}.${b}.${signature.toString("base64url")}`;
}

async function jwksServer(t, keys = [publicJwk(rsa, "key-1", "RS256")]) {
  let current = keys;
  let requests = 0;
  let mode = "normal";
  const server = http.createServer((req, res) => {
    requests++;
    if (mode === "redirect") { res.writeHead(302, { location: "https://evil.example/jwks" }); res.end(); return; }
    res.setHeader("content-type", "application/json");
    res.end(mode === "oversize" ? " ".repeat(1024 * 1024 + 1) : JSON.stringify({ keys: current }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => server.close());
  return {
    url: `http://127.0.0.1:${server.address().port}/keys`,
    setKeys(value) { current = value; },
    setMode(value) { mode = value; },
    requests() { return requests; },
  };
}

function verifier(server) {
  return new OidcVerifier({ issuer: ISSUER, audience: AUDIENCE, jwksUrl: server.url, allowLoopbackHttp: true });
}

test("valid RS256 and ES256 tokens use pinned JWKS and a bounded key cache", async (t) => {
  const server = await jwksServer(t, [publicJwk(rsa, "key-1", "RS256"), publicJwk(ec, "ec-1", "ES256")]);
  const v = verifier(server);
  assert.equal((await v.verify(jwt(rsa, "key-1"))).sub, "ana");
  assert.equal((await v.verify(jwt(ec, "ec-1", {}, "ES256"))).sub, "ana");
  assert.equal(server.requests(), 1, "cached JWKS reused");
  await assert.rejects(v.verify(jwt(rsa, "key-1", {}, "HS256")), /unsupported JWT header/);
  assert.equal(server.requests(), 1, "algorithm confusion never fetches a URL");
});

test("rejects wrong issuer, audience, expiry, future nbf and multi-audience without azp", async (t) => {
  const server = await jwksServer(t);
  const v = verifier(server);
  for (const claims of [
    { iss: "https://evil.example" }, { aud: "wrong" }, { exp: 1 },
    { nbf: Math.floor(Date.now() / 1000) + 600 },
    { aud: [AUDIENCE, "other"] },
  ]) await assert.rejects(v.verify(jwt(rsa, "key-1", claims)), /invalid JWT claims/);
  assert.equal((await v.verify(jwt(rsa, "key-1", { aud: [AUDIENCE, "other"], azp: AUDIENCE }))).sub, "ana");
});

test("unknown kid refreshes only the pinned URL, and an untrusted jku header is ignored", async (t) => {
  const server = await jwksServer(t);
  const v = verifier(server);
  await v.verify(jwt(rsa, "key-1"));
  server.setKeys([publicJwk(rotated, "key-2", "RS256")]);
  assert.equal((await v.verify(jwt(rotated, "key-2", {}, "RS256", { jku: "https://evil.example/keys" }))).sub, "ana");
  assert.equal(server.requests(), 2);
  await assert.rejects(v.verify(jwt(rsa, "key-1")), /untrusted JWT key/);
  assert.equal(server.requests(), 2, "unknown key refreshes are briefly throttled");
});

test("an expired key cache must refresh and refuses login if JWKS is unavailable", async (t) => {
  const server = await jwksServer(t);
  let clock = Date.now();
  const v = new OidcVerifier({ issuer: ISSUER, audience: AUDIENCE,
    jwksUrl: server.url, allowLoopbackHttp: true }, { now: () => clock });
  await v.verify(jwt(rsa, "key-1"));
  clock += 5 * 60_000 + 1;
  server.setMode("redirect");
  await assert.rejects(v.verify(jwt(rsa, "key-1", { exp: Math.floor(clock / 1000) + 600 })), /redirect refused/);
  assert.equal(server.requests(), 2);
});

test("bad signatures, redirect, oversized JWKS and unsafe endpoint fail closed", async (t) => {
  const server = await jwksServer(t);
  const v = verifier(server);
  await assert.rejects(v.verify(jwt(rotated, "key-1")), /bad JWT signature/);
  assert.throws(() => new OidcVerifier({ issuer: ISSUER, audience: AUDIENCE,
    jwksUrl: "http://169.254.169.254/latest", allowLoopbackHttp: true }), /HTTPS/);
  assert.throws(() => new OidcVerifier({ issuer: "http://login.example", audience: AUDIENCE,
    jwksUrl: server.url, allowLoopbackHttp: true }), /issuer must use HTTPS/);
  server.setMode("redirect");
  await assert.rejects(verifier(server).verify(jwt(rsa, "key-1")), /redirect refused/);
  server.setMode("oversize");
  await assert.rejects(verifier(server).verify(jwt(rsa, "key-1")), /too large/);
});

test("gateway accepts a pinned OIDC login and rejects an HS256 login", async (t) => {
  const keys = await jwksServer(t);
  const shard = http.createServer(async (req, res) => {
    res.setHeader("content-type", "application/json");
    res.end(req.url === "/v1/user-tokens" ? JSON.stringify({ token: "kut1.user" }) : JSON.stringify({ ok: true }));
  });
  await new Promise((resolve) => shard.listen(0, "127.0.0.1", resolve));
  t.after(() => shard.close());
  const dir = mkdtempSync(join(tmpdir(), "oidc-gateway-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const config = { oidc: { issuer: ISSUER, audience: AUDIENCE, jwksUrl: keys.url, allowLoopbackHttp: true },
    relaySecret: "relay", stateFile: join(dir, "users.json"), defaultRegion: "test",
    shards: [{ id: "s1", region: "test", url: `http://127.0.0.1:${shard.address().port}`, token: "operator" }] };
  assert.throws(() => createGateway({ ...config, jwtSecret: "secret" }), /exactly one/);
  const gw = createGateway(config);
  await new Promise((resolve) => gw.server.listen(0, "127.0.0.1", resolve));
  t.after(() => gw.server.close());
  const url = `http://127.0.0.1:${gw.server.address().port}/api/sessions`;
  assert.equal((await fetch(url, { headers: { authorization: `Bearer ${jwt(rsa, "key-1")}` } })).status, 200);
  assert.equal((await fetch(url, { headers: { authorization: "Bearer a.b.c" } })).status, 401);
});
