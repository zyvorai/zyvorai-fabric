// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// The price-watch example agent: a plain API-key connector (docs/merion/connectors/README.md), paired with calendar-suggestions as the
// second proactive finder. api.pricewatch.example is a placeholder host, not a real vendor; tested against a fake ctx.fetch only.

import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { buildBundle } from "../src/bundle.js";

const agentFile = fileURLToPath(new URL("../../../examples/merion-agents/price-watch/agent.ts", import.meta.url));
const load = async () => {
  const { bundle } = await buildBundle(agentFile);
  return await import(`data:text/javascript;base64,${Buffer.from(bundle).toString("base64")}#price-watch`);
};

/** A ctx whose fetch answers from `prices` (name -> price, or "throw"/[status, body]), and records every call. */
const fakeCtx = (input, prices) => {
  const calls = [];
  const suggested = [];
  return {
    calls,
    suggested,
    ctx: {
      input,
      emit: (kind, data) => {
        if (kind === "suggestion.propose") suggested.push(data);
      },
      fetch: async (url, init = {}) => {
        calls.push({ url: String(url), credential: init.credential });
        const item = new URL(String(url)).searchParams.get("item");
        const reply = prices[item];
        if (reply === "throw") throw new Error("not connected");
        if (Array.isArray(reply)) {
          const [status, body] = reply;
          return { ok: status >= 200 && status < 300, status, text: async () => JSON.stringify(body) };
        }
        return { ok: true, status: 200, text: async () => JSON.stringify({ price: reply }) };
      },
    },
  };
};

test("needsAlert: at or below the target, both must be finite numbers", async () => {
  const { needsAlert } = await load();
  assert.equal(needsAlert(150, 150), true, "at the target");
  assert.equal(needsAlert(149, 150), true, "below it");
  assert.equal(needsAlert(151, 150), false);
  assert.equal(needsAlert(undefined, 150), false);
  assert.equal(needsAlert(150, undefined), false);
  assert.equal(needsAlert(NaN, 150), false);
  assert.equal(needsAlert("150", 150), false, "a string is not a number");
});

test("with nothing tracked, it says so and calls nothing", async () => {
  const agent = (await load()).default;
  const { ctx, calls } = fakeCtx({}, {});
  assert.match(await agent.run(ctx), /^Nothing is being tracked\./);
  assert.equal(calls.length, 0);
});

test("checks each item with the read-only credential and proposes the ones at or below target", async () => {
  const agent = (await load()).default;
  const { ctx, calls, suggested } = fakeCtx(
    { items: [{ name: "headphones", alertBelow: 150 }, { name: "kettle", alertBelow: 30 }] },
    { headphones: 140, kettle: 45 },
  );
  const out = await agent.run(ctx);
  assert.equal(calls.length, 2);
  assert.ok(calls.every((c) => c.credential === "price-watch-read"));
  assert.match(calls[0].url, /^https:\/\/api\.pricewatch\.example\/v1\/price\?item=headphones$/);
  assert.match(out, /^I suggested 1 thing\./);
  assert.deepEqual(suggested, [{ title: '"headphones" dropped to 140', reason: "At or below your target of 150." }]);
});

test("nothing dropped: reported plainly, nothing proposed", async () => {
  const agent = (await load()).default;
  const { ctx, suggested } = fakeCtx({ items: [{ name: "headphones", alertBelow: 100 }] }, { headphones: 140 });
  assert.equal(await agent.run(ctx), "Nothing tracked has dropped to its target yet.");
  assert.equal(suggested.length, 0);
});

test("one item failing does not stop the others, and all failing is reported distinctly", async () => {
  const agent = (await load()).default;
  const mixed = fakeCtx(
    { items: [{ name: "headphones", alertBelow: 150 }, { name: "kettle", alertBelow: 30 }] },
    { headphones: "throw", kettle: 20 },
  );
  const out = await agent.run(mixed.ctx);
  assert.deepEqual(mixed.suggested, [{ title: '"kettle" dropped to 20', reason: "At or below your target of 30." }]);
  assert.match(out, /^I suggested 1 thing\./);

  const allDown = fakeCtx({ items: [{ name: "a", alertBelow: 1 }, { name: "b", alertBelow: 1 }] }, { a: "throw", b: [500, { error: "down" }] });
  assert.equal(await agent.run(allDown.ctx), "None of the tracked items could be read.");
});

test("at most 10 items, and one with no name is skipped", async () => {
  const agent = (await load()).default;
  const items = [{ alertBelow: 1 }, ...Array.from({ length: 12 }, (_, i) => ({ name: `i${i}`, alertBelow: 1000 }))];
  const { ctx, calls } = fakeCtx({ items }, Object.fromEntries(items.filter((i) => i.name).map((i) => [i.name, 1])));
  await agent.run(ctx);
  assert.equal(calls.length, 10, "the nameless one dropped, the rest capped at 10");
});
