// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { createHmac } from "node:crypto";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createGateway } from "../src/gateway.js";
import { signJwt, toUserId, verifyJwt } from "../src/jwt.js";
import { pickShard } from "../src/shards.js";

const SECRET = "vendor-login-secret";
const OP_A = "operator-token-a";
const OP_B = "operator-token-b";

/** A fake Keep shard: records requests, mints fake user tokens for the operator token only. */
async function fakeShard(operatorToken) {
  const seen = [];
  let mints = 0;
  const server = http.createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const body = Buffer.concat(chunks).toString();
    const auth = (req.headers.authorization ?? "").replace("Bearer ", "");
    seen.push({ method: req.method, url: req.url, auth, body });
    const json = (status, value) => { res.writeHead(status, { "content-type": "application/json" }); res.end(JSON.stringify(value)); };
    if (req.url === "/v1/user-tokens") {
      if (auth !== operatorToken) return json(403, { error: "operator only" });
      mints += 1;
      return json(201, { token: `kut1.${JSON.parse(body).user_id}.${mints}` });
    }
    if (req.url === "/v1/sessions/redirect") {
      res.writeHead(302, { location: "http://127.0.0.1:1/steal" });
      return res.end();
    }
    if (req.url.startsWith("/v1/users/") && auth !== operatorToken && req.method !== "GET") return json(403, { error: "operator only" });
    if (req.url.startsWith("/v1/sessions") && auth.startsWith("kut1.stale")) return json(401, { error: "expired" });
    if (req.url.startsWith("/v1/usage")) return json(200, { usage: { runs: 3 } });
    return json(200, { ok: true, url: req.url });
  });
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  return { seen, url: `http://127.0.0.1:${server.address().port}`, mints: () => mints, close: () => server.close() };
}

async function setup(t, overrides = {}) {
  const eu = await fakeShard(OP_A);
  const us = await fakeShard(OP_B);
  t.after(() => { eu.close(); us.close(); });
  const dir = mkdtempSync(join(tmpdir(), "gw-"));
  const sent = [];
  const gw = createGateway({
    jwtSecret: SECRET, adminKey: "admin-secret", relaySecret: "relay-secret", defaultRegion: "eu",
    stateFile: join(dir, "users.json"), ratePerMinute: 1000,
    shards: [{ id: "eu-1", region: "eu", url: eu.url, token: OP_A }, { id: "us-1", region: "us", url: us.url, token: OP_B }],
    ...overrides,
  }, { adapters: { webhook: { async send(d, m) { sent.push([d, m]); } }, broken: { async send() { throw new Error("boom"); } } } });
  await new Promise((ok) => gw.server.listen(0, "127.0.0.1", ok));
  t.after(() => gw.server.close());
  const base = `http://127.0.0.1:${gw.server.address().port}`;
  const login = (claims = {}) => "Bearer " + signJwt({ sub: "ana@example.com", exp: Math.floor(Date.now() / 1000) + 600, ...claims }, SECRET);
  return { eu, us, gw, base, login, sent };
}

test("only a valid, unexpired login is accepted, and 'none' is refused", async (t) => {
  const { base, login } = await setup(t);
  const get = (auth) => fetch(`${base}/api/sessions`, { headers: auth ? { authorization: auth } : {} });
  assert.equal((await get(undefined)).status, 401);
  assert.equal((await get("Bearer nonsense")).status, 401);
  assert.equal((await get("Bearer " + signJwt({ sub: "x", exp: 1 }, SECRET))).status, 401, "expired");
  assert.equal((await get("Bearer " + signJwt({ sub: "x" }, "wrong-secret"))).status, 401, "wrong key");
  const none = Buffer.from(JSON.stringify({ alg: "none" })).toString("base64url") + "." + Buffer.from(JSON.stringify({ sub: "x" })).toString("base64url") + ".";
  assert.equal((await get("Bearer " + none)).status, 401, "alg none");
  assert.equal((await get(login())).status, 200);
  assert.throws(() => verifyJwt(none, SECRET), /unsupported algorithm|malformed/);
});

test("a user is placed once, in their region, and stays there", async (t) => {
  const { base, login, eu, us, gw } = await setup(t);
  await fetch(`${base}/api/sessions`, { headers: { authorization: login() } });
  await fetch(`${base}/api/sessions`, { headers: { authorization: login({ region: "us" }) } }); // region claim on a known user is ignored
  assert.equal(gw.placement.assigned["u-" + toUserId("ana@example.com").slice(2)].shard, "eu-1");
  assert.ok(eu.seen.some((r) => r.url === "/v1/sessions") && !us.seen.some((r) => r.url === "/v1/sessions"));
  // A new user in the us region goes to the us shard; a region with no shard gets a 503.
  const bob = await fetch(`${base}/api/sessions`, { headers: { authorization: login({ sub: "bob", region: "us" }) } });
  assert.equal(bob.status, 200);
  assert.ok(us.seen.some((r) => r.url === "/v1/sessions"));
  const nowhere = await fetch(`${base}/api/sessions`, { headers: { authorization: login({ sub: "cy", region: "mars" }) } });
  assert.equal(nowhere.status, 503);
});

test("requests reach the shard with a user token, never the operator token or the client's own", async (t) => {
  const { base, login, eu } = await setup(t);
  const r = await fetch(`${base}/api/sessions?limit=5`, { headers: { authorization: login(), "x-extra": "1" } });
  assert.equal(r.status, 200);
  const call = eu.seen.find((c) => c.url === "/v1/sessions?limit=5");
  assert.ok(call, "path and query are mapped");
  assert.match(call.auth, /^kut1\./, "a user token");
  assert.notEqual(call.auth, OP_A);
  assert.ok(!call.auth.includes("eyJ"), "the client's login JWT is not forwarded");
  // Minted once and reused.
  await fetch(`${base}/api/inbox`, { headers: { authorization: login() } });
  assert.equal(eu.mints(), 1);
});

test("only the phone-facing routes are exposed", async (t) => {
  const { base, login, eu } = await setup(t);
  for (const p of ["/api/keep/status", "/api/user-tokens", "/api/triggers", "/api/model-grants", "/api/users/ana/devices", "/api/vault/status", "/nope"]) {
    const r = await fetch(`${base}${p}`, { headers: { authorization: login() } });
    assert.equal(r.status, 404, p);
  }
  assert.ok(!eu.seen.some((c) => c.url.startsWith("/v1/keep") || c.url.startsWith("/v1/triggers")), "nothing operator-only was forwarded");
  for (const p of ["sessions", "approvals", "artifacts", "audit", "usage", "inbox", "demos"]) {
    assert.equal((await fetch(`${base}/api/${p}`, { headers: { authorization: login() } })).status, 200, p);
  }
});

test("only plain path segments reach a shard: traversal, encoded dots and slashes are refused", async (t) => {
  const { base, login, eu } = await setup(t);
  for (const p of [
    "/api/sessions/%2e%2e/keep/status",
    "/api/sessions/..%2fkeep/status",
    "/api/sessions/%2F%2Fevil.example",
    "/api/sessions/a%5Cb",
    "/api/sessions//x",
    "/api/sessions/a b",
  ]) {
    const r = await fetch(`${base}${p}`, { headers: { authorization: login() } });
    assert.ok([400, 404].includes(r.status), `${p} -> ${r.status}`);
  }
  assert.ok(!eu.seen.some((c) => c.url.includes("keep") || c.url.includes("evil")), "nothing odd reached the shard");
  // Ordinary nested paths, with ids and a query, still work.
  const ok = await fetch(`${base}/api/artifacts/11111111-1111-4111-8111-111111111111/diff/22222222-2222-4222-8222-222222222222?x=1`, { headers: { authorization: login() } });
  assert.equal(ok.status, 200);
  assert.ok(eu.seen.some((c) => c.url === "/v1/artifacts/11111111-1111-4111-8111-111111111111/diff/22222222-2222-4222-8222-222222222222?x=1"));
});

test("a POST body (an approval decision) is forwarded intact", async (t) => {
  const { base, login, eu } = await setup(t);
  const body = JSON.stringify({ decision: "approved", device_id: "d", signature: "AAAA" });
  const r = await fetch(`${base}/api/approvals/11111111-1111-4111-8111-111111111111`, { method: "POST", headers: { authorization: login(), "content-type": "application/json" }, body });
  assert.equal(r.status, 200);
  const call = eu.seen.find((c) => c.method === "POST" && c.url.startsWith("/v1/approvals/"));
  assert.equal(call.body, body);
});

test("upstream redirects are refused instead of forwarding user credentials", async (t) => {
  const { base, login, eu } = await setup(t);
  const r = await fetch(`${base}/api/sessions/redirect`, { headers: { authorization: login() } });
  assert.equal(r.status, 502);
  assert.equal(eu.seen.filter((call) => call.url === "/v1/sessions/redirect").length, 1);
});

test("an oversized relay body receives 413", async (t) => {
  const { base } = await setup(t);
  const r = await fetch(`${base}/relay/push`, { method: "POST", body: "x".repeat((1 << 20) + 1) });
  assert.equal(r.status, 413);
  assert.deepEqual(await r.json(), { error: "body too large" });
});

test("an oversized streamed body also receives 413", async (t) => {
  const { base } = await setup(t);
  const body = new ReadableStream({
    start(controller) {
      controller.enqueue(Buffer.alloc(700_000));
      controller.enqueue(Buffer.alloc(400_000));
      controller.close();
    },
  });
  const r = await fetch(`${base}/relay/push`, { method: "POST", body, duplex: "half" });
  assert.equal(r.status, 413);
});

test("a stale user token is replaced once", async (t) => {
  const { base, login, eu, gw } = await setup(t);
  // Poison the cache with a token the shard calls expired.
  gw.tokens.cache.set("eu-1/" + toUserId("ana@example.com"), { token: "kut1.stale", expires: Date.now() + 3_600_000 });
  const r = await fetch(`${base}/api/sessions`, { headers: { authorization: login() } });
  assert.equal(r.status, 200);
  assert.equal(eu.mints(), 1, "re-minted after the 401");
});

test("enrolling a device needs a strong login and uses the operator token; reading uses the user's", async (t) => {
  const { base, login, eu } = await setup(t);
  const enrol = (auth) => fetch(`${base}/api/devices`, { method: "POST", headers: { authorization: auth, "content-type": "application/json" }, body: JSON.stringify({ device_id: "p", alg: "p256", public_key: "AAAA" }) });
  assert.equal((await enrol(login())).status, 403, "an ordinary login cannot enrol");
  assert.ok(!eu.seen.some((c) => c.url.startsWith("/v1/users/")));
  assert.equal((await enrol(login({ acr: "strong" }))).status, 200);
  const post = eu.seen.find((c) => c.method === "POST" && c.url.startsWith("/v1/users/"));
  assert.equal(post.auth, OP_A, "the gateway enrols with the operator token");
  assert.equal(post.url, `/v1/users/${toUserId("ana@example.com")}/devices`);
  await fetch(`${base}/api/devices`, { headers: { authorization: login() } });
  const get = eu.seen.find((c) => c.method === "GET" && c.url.includes("/devices"));
  assert.match(get.auth, /^kut1\./, "listing uses the user's own token");
  assert.equal((await fetch(`${base}/api/devices/p`, { method: "DELETE", headers: { authorization: login() } })).status, 403);
  assert.equal((await fetch(`${base}/api/devices/p`, { method: "DELETE", headers: { authorization: login({ acr: "strong" }) } })).status, 200);
});

test("the push relay checks the runtime's signature and dispatches by kind", async (t) => {
  const { base, sent } = await setup(t);
  const post = (body, secret = "relay-secret") => {
    const raw = JSON.stringify(body);
    return fetch(`${base}/relay/push`, { method: "POST", headers: { "x-zyvor-signature": "sha256=" + createHmac("sha256", secret).update(raw).digest("hex") }, body: raw });
  };
  const msg = (kind) => ({ device: { id: "d1", push: { kind, token: "T" } }, approval: { id: "a", kind: "send", prompt: "send it", session_id: "s" }, sign: { challenge: "c" } });
  assert.equal((await post(msg("webhook"), "wrong")).status, 401);
  assert.equal((await post(msg("webhook"))).status, 202);
  assert.equal(sent.length, 1);
  assert.equal(sent[0][1].approval.prompt, "send it");
  assert.equal((await post(msg("carrier-pigeon"))).status, 422);
  assert.equal((await post(msg("broken"))).status, 502, "a failed push is a 5xx so the runtime retries");
});

test("a user is rate limited", async (t) => {
  const { base, login } = await setup(t, { ratePerMinute: 3 });
  const codes = [];
  for (let i = 0; i < 5; i++) codes.push((await fetch(`${base}/api/sessions`, { headers: { authorization: login() } })).status);
  assert.deepEqual(codes, [200, 200, 200, 429, 429]);
  assert.equal((await fetch(`${base}/api/sessions`, { headers: { authorization: login({ sub: "someone-else" }) } })).status, 200, "other users are unaffected");
});

test("admin routes need the admin key, list shards without their tokens, and roll up usage", async (t) => {
  const { base, login } = await setup(t);
  await fetch(`${base}/api/sessions`, { headers: { authorization: login() } });
  assert.equal((await fetch(`${base}/admin/shards`)).status, 401);
  const shards = await (await fetch(`${base}/admin/shards`, { headers: { "x-admin-key": "admin-secret" } })).json();
  assert.equal(shards.shards.find((s) => s.id === "eu-1").users, 1);
  assert.ok(!JSON.stringify(shards).includes(OP_A), "operator tokens never leave the gateway");
  const usage = await (await fetch(`${base}/admin/usage?user_id=${toUserId("ana@example.com")}`, { headers: { "x-admin-key": "admin-secret" } })).json();
  assert.equal(usage.usage.runs, 3);
  assert.equal((await fetch(`${base}/admin/usage?user_id=nobody`, { headers: { "x-admin-key": "admin-secret" } })).status, 404);
});

test("account ids map to valid, distinct runtime user ids", () => {
  assert.equal(toUserId("ana"), "ana");
  const a = toUserId("Ana@Example.com"), b = toUserId("ana@example.com");
  for (const id of [a, b]) assert.match(id, /^[a-z0-9][a-z0-9._-]{0,31}$/);
  assert.notEqual(a, b, "different accounts never share an id");
  assert.equal(toUserId("Ana@Example.com"), a, "and the mapping is stable");
});

test("placement is stable when a shard is added", () => {
  const shards = [{ id: "a" }, { id: "b" }, { id: "c" }];
  const users = Array.from({ length: 300 }, (_, i) => `user-${i}`);
  const before = new Map(users.map((u) => [u, pickShard(u, shards).id]));
  const after = new Map(users.map((u) => [u, pickShard(u, [...shards, { id: "d" }]).id]));
  const moved = users.filter((u) => before.get(u) !== after.get(u));
  assert.ok(moved.every((u) => after.get(u) === "d"), "a user only ever moves to the new shard");
  assert.ok(moved.length > 30 && moved.length < 120, `about a quarter move (${moved.length})`);
});
