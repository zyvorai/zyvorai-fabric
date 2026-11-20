// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import test from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { approvedUrl, isPublicIp, sendWebhook } from "../src/webhook.js";
import { defaultAdapters } from "../src/relay.js";
import { TokenBroker } from "../src/shards.js";

const allowedHosts = ["push.example.com"];

function transport(statusCode = 202) {
  const calls = [];
  const httpsRequest = (url, options, response) => {
    const req = new EventEmitter();
    req.end = (body) => {
      calls.push({ url, options, body });
      queueMicrotask(() => response({ statusCode, resume() {} }));
    };
    req.destroy = (error) => req.emit("error", error);
    return req;
  };
  return { calls, httpsRequest };
}

test("webhook destinations require an exact approved HTTPS name", () => {
  for (const url of [
    "http://push.example.com/a", "https://push.example.com.evil.test/a",
    "https://user:pass@push.example.com/a", "https://push.example.com:8443/a",
    "https://push.example.com./a", "https://push.example.com/a#frag",
    "https://127.0.0.1/a", "file:///etc/passwd",
  ]) assert.throws(() => approvedUrl(url, allowedHosts), url);
  assert.equal(approvedUrl("https://PUSH.example.com/a", allowedHosts).hostname, "push.example.com");
  assert.throws(() => approvedUrl("https://push.example.com/a", []));
});

test("reserved addresses and mixed DNS answers are rejected before any connection", async () => {
  for (const address of ["127.0.0.1", "10.1.2.3", "169.254.169.254", "192.0.2.1", "::1", "fc00::1", "::ffff:127.0.0.1"]) {
    assert.equal(isPublicIp(address), false, address);
  }
  assert.equal(isPublicIp("8.8.8.8"), true);
  const transportStub = transport();
  await assert.rejects(sendWebhook("https://push.example.com/send", { ok: true }, {
    allowedHosts, dnsLookup: async () => [{ address: "8.8.8.8", family: 4 }, { address: "127.0.0.1", family: 4 }],
    httpsRequest: transportStub.httpsRequest,
  }), /public addresses/);
  assert.equal(transportStub.calls.length, 0);
});

test("delivery pins resolved public IP while keeping the HTTPS hostname, and refuses redirects", async () => {
  const sent = transport(202);
  const options = { allowedHosts, dnsLookup: async () => [{ address: "8.8.8.8", family: 4 }], httpsRequest: sent.httpsRequest };
  await sendWebhook("https://push.example.com/send", { id: "a" }, options);
  assert.equal(sent.calls.length, 1);
  assert.equal(sent.calls[0].url.hostname, "push.example.com");
  assert.equal(sent.calls[0].options.agent, false);
  await new Promise((resolve) => sent.calls[0].options.lookup("push.example.com", {}, (_err, address, family) => {
    assert.equal(address, "8.8.8.8");
    assert.equal(family, 4);
    resolve();
  }));
  assert.deepEqual(JSON.parse(sent.calls[0].body), { id: "a" });
  const redirected = transport(302);
  await assert.rejects(sendWebhook("https://push.example.com/send", {}, { ...options, httpsRequest: redirected.httpsRequest }), /HTTP 302/);
  assert.equal(redirected.calls.length, 1);
});

test("default webhook adapter fails closed without configured hosts", async () => {
  const adapter = defaultAdapters().webhook;
  await assert.rejects(adapter.send({ push: { token: "https://push.example.com/send" } }, {
    approval: { kind: "run", prompt: "approve", id: "a", session_id: "s" }, device: { id: "d" }, sign: {},
  }), /explicitly approved/);
});

test("token mint requests use manual redirect mode", async () => {
  let redirect;
  const broker = new TokenBroker({ fetchImpl: async (_url, init) => {
    redirect = init.redirect;
    return new Response(null, { status: 302, headers: { location: "https://elsewhere.example/" } });
  } });
  await assert.rejects(broker.tokenFor({ id: "s", url: "https://shard.example", token: "operator" }, "alice"), /HTTP 302/);
  assert.equal(redirect, "manual");
});
