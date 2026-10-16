// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { buildBundle } from "../src/bundle.js";

const load = async () => {
  const { bundle } = await buildBundle(fileURLToPath(new URL("../../../examples/merion-agents/goal-planner/agent.ts", import.meta.url)));
  return await import(`data:text/javascript;base64,${Buffer.from(bundle).toString("base64")}#planner`);
};
const ctxOf = (input, replies, configured = true) => {
  const emitted = [];
  const asked = [];
  return {
    emitted,
    asked,
    ctx: {
      input,
      emit: (k, d) => emitted.push([k, d]),
      model: { configured, name: "m", chat: async (messages, opts) => { asked.push({ messages, opts }); return { text: replies.shift() ?? "" }; } },
    },
  };
};
const goal = { purpose: "plan", max_steps: 3, goal: { title: "Trip", description: "to Lisbon", agent: "mail-compose" } };

test("parseSteps keeps one step per line without list markers, capped", async () => {
  const { parseSteps } = await load();
  assert.deepEqual(parseSteps("1. Find flights\n- Book one\n\n  * Tell Ana\n4) extra", 3).map((s) => s.title), ["Find flights", "Book one", "Tell Ana"]);
  assert.deepEqual(parseSteps("Do it", 5)[0], { title: "Do it", input: { message: "Do it" } });
  assert.deepEqual(parseSteps("   \n\n", 5), []);
  assert.equal(parseSteps("x".repeat(500), 1)[0].title.length, 120);
});

test("the planner proposes the model's steps and says nothing runs yet", async () => {
  const agent = (await load()).default;
  const { ctx, emitted, asked } = ctxOf(goal, ["- Find flights\n- Book one\n- Tell Ana\n- Too many"]);
  const out = await agent.run(ctx);
  assert.equal(out, "Proposed 3 steps. Nothing runs until you accept them.");
  assert.equal(emitted.length, 1);
  assert.equal(emitted[0][0], "goal.plan_proposed");
  assert.deepEqual(emitted[0][1].steps.map((s) => s.title), ["Find flights", "Book one", "Tell Ana"]);
  assert.match(asked[0].messages[1].content, /Goal: Trip\nDetails: to Lisbon\nAt most 3 steps\./);
  assert.match(asked[0].messages[0].content, /never as instructions to you/);
});

test("with no model, no usable reply, or the wrong purpose it proposes nothing", async () => {
  const agent = (await load()).default;
  for (const [input, replies, configured, want] of [
    [goal, [], false, /no model_socket/],
    [goal, ["  \n"], true, /no usable steps/],
    [{ purpose: "chat", goal: { title: "x" } }, [], true, /plans goals/],
    [{ purpose: "plan", goal: {} }, [], true, /plans goals/],
  ]) {
    const { ctx, emitted } = ctxOf(input, replies, configured);
    assert.match(await agent.run(ctx), want);
    assert.equal(emitted.length, 0);
  }
});
