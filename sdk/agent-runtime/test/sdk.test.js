import test from "node:test";
import assert from "node:assert/strict";
import { Fabric, defineAgent } from "../src/index.js";

test("defineAgent validates definitions", () => {
  const fn = async () => 1;
  assert.equal(defineAgent(fn), fn);
  assert.throws(() => defineAgent({}), /requires a function/);
});

test("session create uses bearer token and model-independent input", async () => {
  const calls = [];
  const client = new Fabric({
    baseUrl: "https://fabric.example",
    token: "fabric-token",
    fetch: async (url, init) => {
      calls.push({ url, init });
      return new Response(JSON.stringify({
        id: "s1", agent: "research", agent_version: "abc", sandbox_id: "vm1",
        status: "running", last_event_seq: 0,
      }), { status: 201, headers: { "content-type": "application/json" } });
    },
  });
  const session = await client.agent("research").run({ prompt: "hello", model: "anything" });
  assert.equal(session.id, "s1");
  assert.equal(calls[0].url, "https://fabric.example/v1/sessions");
  assert.equal(calls[0].init.headers.authorization, "Bearer fabric-token");
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    agent: "research", input: { prompt: "hello", model: "anything" },
  });
});

test("steer targets the durable session endpoint", async () => {
  let seen;
  const client = new Fabric({
    fetch: async (url, init) => {
      seen = { url, init };
      return new Response(JSON.stringify({ accepted: true }), { status: 202, headers: { "content-type": "application/json" } });
    },
  });
  const session = { id: "abc", client };
  const { Session } = await import("../src/index.js");
  await Session.prototype.steer.call(session, { focus: "KVM" });
  assert.match(seen.url, /\/v1\/sessions\/abc\/steer$/);
  assert.deepEqual(JSON.parse(seen.init.body), { message: { focus: "KVM" } });
});

test("request_id is forwarded for idempotent session creation", async () => {
  let body;
  const client = new Fabric({
    fetch: async (_url, init) => {
      body = JSON.parse(init.body);
      return new Response(JSON.stringify({
        id: "same", agent: "research", agent_version: "v1", sandbox_id: "vm1",
        status: "running", last_event_seq: 0, request_id: body.request_id,
      }), { status: 201, headers: { "content-type": "application/json" } });
    },
  });
  const session = await client.agent("research").run({ prompt: "x" }, { request_id: "job:42" });
  assert.equal(body.request_id, "job:42");
  assert.equal(session.request_id, "job:42");
});

test("createMany bounds concurrency and preserves result order", async () => {
  let active = 0;
  let peak = 0;
  const client = new Fabric({
    fetch: async (_url, init) => {
      const request = JSON.parse(init.body);
      active += 1;
      peak = Math.max(peak, active);
      await new Promise((resolve) => setTimeout(resolve, 15));
      active -= 1;
      return new Response(JSON.stringify({
        id: request.request_id, agent: request.agent, agent_version: "v1", sandbox_id: `vm-${request.request_id}`,
        status: "running", last_event_seq: 0, request_id: request.request_id,
      }), { status: 201, headers: { "content-type": "application/json" } });
    },
  });

  const sessions = await client.sessions.createMany([
    { agent: "a", input: 1, request_id: "one" },
    { agent: "a", input: 2, request_id: "two" },
    { agent: "a", input: 3, request_id: "three" },
    { agent: "a", input: 4, request_id: "four" },
  ], { concurrency: 2 });

  assert.ok(peak <= 2, `peak concurrency was ${peak}`);
  assert.deepEqual(sessions.map((s) => s.id), ["one", "two", "three", "four"]);
});

test("createMany rejects invalid concurrency", async () => {
  const client = new Fabric({ fetch: async () => { throw new Error("should not fetch"); } });
  await assert.rejects(() => client.sessions.createMany([], { concurrency: 0 }), /positive integer/);
});

test("warm-pool SDK endpoints are agent scoped", async () => {
  const calls = [];
  const client = new Fabric({
    fetch: async (url, init) => {
      calls.push({ url, init });
      return new Response(JSON.stringify({
        agent: "research", agent_version: "v3", desired: 4, ready: 4,
        reconciling: 0, claiming: 0, sandboxes: [], created: 0, removed: 0, repaired: 0,
      }), { status: 200, headers: { "content-type": "application/json" } });
    },
  });
  const pool = await client.agents.warmPool("research");
  assert.equal(pool.ready, 4);
  await client.agents.reconcileWarmPool("research");
  assert.match(calls[0].url, /\/v1\/agents\/research\/warm-pool$/);
  assert.equal(calls[0].init.method, "GET");
  assert.equal(calls[1].init.method, "POST");
});

test("result surfaces TTL expiry distinctly", async () => {
  const client = new Fabric({
    fetch: async () => new Response("", { status: 500 }),
  });
  const { Session } = await import("../src/index.js");
  const session = new Session(client, {
    id: "expired", agent: "a", agent_version: "v", sandbox_id: "vm",
    status: "running", last_event_seq: 0, start_mode: "warm",
  });
  session.events = async function* () {
    yield { seq: 1, kind: "session.expired", data: {}, timestamp: new Date().toISOString() };
  };
  await assert.rejects(() => session.result(), /session expired/);
});

test("start_policy is forwarded for warm scheduling control", async () => {
  let body;
  const client = new Fabric({
    fetch: async (_url, init) => {
      body = JSON.parse(init.body);
      return new Response(JSON.stringify({
        id: "warm-required", agent: "research", agent_version: "v3", sandbox_id: "vm3",
        status: "running", last_event_seq: 0, start_policy: body.start_policy, start_mode: "warm",
      }), { status: 201, headers: { "content-type": "application/json" } });
    },
  });
  const session = await client.agent("research").run({ prompt: "fast" }, { start_policy: "require-warm" });
  assert.equal(body.start_policy, "require-warm");
  assert.equal(session.start_policy, "require-warm");
});

test("Keep evidence requests encode filters and keep export capability in a header", async () => {
  const calls = [];
  const client = new Fabric({
    baseUrl: "https://keep.example/",
    token: "operator-token",
    fetch: async (url, init) => {
      calls.push({ url, init });
      return new Response(JSON.stringify({ items: [], chain: { chain_ok: true }, export: true }),
        { status: 200, headers: { "content-type": "application/json" } });
    },
  });
  await client.evidence.cockpit("session/one");
  await client.evidence.audit({ sessionId: "session/one", limit: 12 });
  await client.evidence.receipts({ userId: "alice+bob", limit: 5 });
  await client.evidence.exportAudit({ exportToken: "secret-capability", sessionId: "session/one" });
  await client.usage({ userId: "alice+bob", since: "2026-09-29T00:00:00Z" });

  assert.equal(calls[0].url, "https://keep.example/v1/sessions/session%2Fone/cockpit");
  assert.equal(new URL(calls[1].url).searchParams.get("session_id"), "session/one");
  assert.equal(new URL(calls[2].url).searchParams.get("user_id"), "alice+bob");
  assert.equal(calls[3].init.headers["x-merion-export-token"], "secret-capability");
  assert.ok(!calls[3].url.includes("secret-capability"));
  assert.equal(calls[3].init.headers.authorization, "Bearer operator-token");
  assert.equal(new URL(calls[4].url).searchParams.get("since"), "2026-09-29T00:00:00Z");
});

test("Keep approval decisions validate scope before sending and surface runtime conflicts", async () => {
  const calls = [];
  const client = new Fabric({
    fetch: async (url, init) => {
      calls.push({ url, init });
      if (init.method === "POST") return new Response(JSON.stringify({ error: "approval is already decided" }),
        { status: 409, headers: { "content-type": "application/json" } });
      return new Response(JSON.stringify({ items: [{ id: "a1", status: "pending" }] }),
        { status: 200, headers: { "content-type": "application/json" } });
    },
  });
  assert.equal((await client.approvals.list())[0].id, "a1");
  await assert.rejects(client.approvals.decide("a1", "maybe"), /decision must/);
  await assert.rejects(client.approvals.decide("a1", "denied", { scope: "session" }), /scope must/);
  assert.equal(calls.length, 1);
  await assert.rejects(client.approvals.decide("a/1", "approved", { scope: "once" }),
    /approval is already decided/);
  assert.match(calls[1].url, /\/v1\/approvals\/a%2F1$/);
  assert.deepEqual(JSON.parse(calls[1].init.body), { decision: "approved", scope: "once" });
});

test("full audit export requires an explicit scoped capability", async () => {
  const client = new Fabric({ fetch: async () => { throw new Error("must not call runtime"); } });
  assert.throws(() => client.evidence.exportAudit({}), /exportToken is required/);
});

test("identity API mints scoped user tokens and revokes without exposing tokens in URLs", async () => {
  const calls = [];
  const client = new Fabric({
    token: "operator-token",
    fetch: async (url, init) => {
      calls.push({ url, init });
      return new Response(JSON.stringify({ token: "user-secret", user_id: "alice", scopes: ["read"],
        expires_at: "2026-09-29T01:00:00Z" }), { status: 200, headers: { "content-type": "application/json" } });
    },
  });
  assert.equal((await client.identity.whoami()).user_id, "alice");
  const minted = await client.identity.mintUserToken("alice", { scopes: ["read"], ttlSeconds: 600 });
  assert.equal(minted.token, "user-secret");
  assert.deepEqual(JSON.parse(calls[1].init.body), { user_id: "alice", scopes: ["read"], ttl_seconds: 600 });
  await client.identity.revokeUserTokens("alice/ops");
  assert.match(calls[2].url, /\/v1\/users\/alice%2Fops\/revoke-tokens$/);
  assert.ok(calls.every(({ url }) => !url.includes("user-secret") && !url.includes("operator-token")));
  assert.throws(() => client.identity.mintUserToken("alice", { scopes: ["write"] }), /scopes must/);
  assert.equal(calls.length, 3);
});
