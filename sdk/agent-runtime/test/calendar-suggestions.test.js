// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// The calendar-suggestions example agent: a heuristic proactive finder (docs/merion/REMAINING.md's "a real finder"). Pure decision
// functions tested directly, then the whole agent against a fake `ctx.fetch`, the same pattern google-agents.test.js uses.

import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { buildBundle } from "../src/bundle.js";

const agentFile = fileURLToPath(new URL("../../../examples/merion-agents/calendar-suggestions/agent.ts", import.meta.url));
const load = async () => {
  const { bundle } = await buildBundle(agentFile);
  return await import(`data:text/javascript;base64,${Buffer.from(bundle).toString("base64")}#calendar-suggestions`);
};

/** A ctx whose fetch answers once with `events` as a Google Calendar `events.list` response, and records the request. */
const fakeCtx = (input, events) => {
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
        return { ok: true, status: 200, text: async () => JSON.stringify({ items: events }) };
      },
    },
  };
};

test("needsPrepReminder: no notes and not all-day is the only case that needs one", async () => {
  const { needsPrepReminder } = await load();
  assert.equal(needsPrepReminder({ start: { dateTime: "2026-10-01T10:00:00Z" } }), true, "no description at all");
  assert.equal(needsPrepReminder({ start: { dateTime: "2026-10-01T10:00:00Z" }, description: "" }), true, "blank description");
  assert.equal(needsPrepReminder({ start: { dateTime: "2026-10-01T10:00:00Z" }, description: "bring the slides" }), false);
  assert.equal(needsPrepReminder({ start: { date: "2026-10-01" } }), false, "an all-day event is not a meeting to prep for");
});

test("needsAPlace: guests with nowhere to meet, but not a video link or a solo event", async () => {
  const { needsAPlace } = await load();
  assert.equal(needsAPlace({ attendees: [{ email: "a@example.com" }] }), true);
  assert.equal(needsAPlace({ attendees: [{ email: "a@example.com" }], location: "Room 2" }), false);
  assert.equal(needsAPlace({ attendees: [{ email: "a@example.com" }], hangoutLink: "https://meet.example/x" }), false);
  assert.equal(needsAPlace({ attendees: [{ email: "a@example.com" }], conferenceData: {} }), false);
  assert.equal(needsAPlace({}), false, "nobody invited, nothing to flag");
});

test("findSuggestions: at most one of each rule per event, at most 5 total, in order", async () => {
  const { findSuggestions } = await load();
  const bare = { summary: "1:1", start: { dateTime: "2026-10-01T10:00:00Z" }, attendees: [{ email: "a@example.com" }] };
  const fine = { summary: "Standup", start: { dateTime: "2026-10-01T09:00:00Z" }, description: "notes here", location: "Room 1" };
  const out = findSuggestions([bare, fine]);
  assert.deepEqual(out, [
    { title: 'Prepare for "1:1"', reason: "It's coming up with no notes attached." },
    { title: 'Add a place or link to "1:1"', reason: "Other people are invited, but there is nowhere to meet." },
  ]);
  assert.deepEqual(findSuggestions([fine]), [], "nothing wrong with it");
  const many = Array.from({ length: 10 }, (_, i) => ({ summary: `e${i}`, start: { dateTime: "2026-10-01T10:00:00Z" } }));
  assert.equal(findSuggestions(many).length, 5, "capped");
  assert.equal(findSuggestions([{ start: { dateTime: "2026-10-01T10:00:00Z" } }])[0].title, 'Prepare for "(no title)"');
});

test("the agent reads the calendar with the read-only credential and proposes what it found", async () => {
  const agent = (await load()).default;
  const { ctx, calls, suggested } = fakeCtx(
    { days: 2 },
    [{ summary: "Board review", start: { dateTime: "2026-10-01T10:00:00Z" } }],
  );
  const out = await agent.run(ctx);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].credential, "calendar-read");
  assert.match(calls[0].url, /^https:\/\/www\.googleapis\.com\/calendar\/v3\/calendars\/primary\/events\?/);
  assert.match(out, /^I suggested 1 thing\./);
  assert.deepEqual(suggested, [{ title: 'Prepare for "Board review"', reason: "It's coming up with no notes attached." }]);
});

test("nothing to suggest says so, and proposes nothing", async () => {
  const agent = (await load()).default;
  const { ctx, suggested } = fakeCtx({}, [{ summary: "Standup", start: { dateTime: "2026-10-01T09:00:00Z" }, description: "notes", location: "Room 1" }]);
  assert.equal(await agent.run(ctx), "Nothing on your calendar needed a suggestion.");
  assert.equal(suggested.length, 0);
});

test("a broker/network failure is reported in words, not thrown", async () => {
  const agent = (await load()).default;
  const out = await agent.run({ input: {}, emit: () => {}, fetch: async () => { throw new Error("not connected"); } });
  assert.match(out, /^Calendar was not read: not connected/);
});

test("days is clamped to 1..3, and defaults to 1", async () => {
  const agent = (await load()).default;
  for (const [given, wantDays] of [[undefined, 1], [0, 1], [-5, 1], [3, 3], [30, 3]]) {
    const { ctx, calls } = fakeCtx({ days: given }, []);
    await agent.run(ctx);
    const q = new URL(calls[0].url).searchParams;
    const spanDays = (new Date(q.get("timeMax")) - new Date(q.get("timeMin"))) / 86400_000;
    assert.ok(Math.abs(spanDays - wantDays) < 0.01, `days=${given} -> span ${spanDays}, wanted ${wantDays}`);
  }
});
