// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// The Gmail and Calendar example agents against a fake `ctx.fetch`: what they ask for, which credential they name, what they refuse to build.

import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { buildBundle } from "../src/bundle.js";

const agentFile = (name) => fileURLToPath(new URL(`../../../examples/merion-agents/${name}/agent.ts`, import.meta.url));
const load = async (name) => {
  const { bundle } = await buildBundle(agentFile(name));
  return (await import(`data:text/javascript;base64,${Buffer.from(bundle).toString("base64")}#${name}`)).default;
};

/** A ctx whose fetch answers from `replies` in order (each a [status, body] pair) and records every call. */
const fakeCtx = (input, replies = []) => {
  const calls = [];
  const events = [];
  return {
    calls,
    events,
    ctx: {
      input,
      emit: (kind, data) => events.push([kind, data]),
      fetch: async (url, init = {}) => {
        calls.push({ url: String(url), method: init.method ?? "GET", credential: init.credential, body: init.body });
        const [status, body] = replies.shift() ?? [200, {}];
        // the worker's brokerFetch throws when the HOST refuses (not connected, denied, expired): status "throw" stands for that
        if (status === "throw") throw new Error(body);
        const text = typeof body === "string" ? body : JSON.stringify(body);
        return { ok: status >= 200 && status < 300, status, text: async () => text };
      },
    },
  };
};
const decodeRaw = (raw) => Buffer.from(raw.replace(/-/g, "+").replace(/_/g, "/"), "base64").toString("utf8");

test("gmail-triage lists unread mail with the read-only credential and prints other people's text as plain data", async () => {
  const agent = await load("gmail-triage");
  const { ctx, calls } = fakeCtx({}, [
    [200, { messages: [{ id: "m1" }, { id: "../evil" }, { id: "m2" }] }],
    [200, { payload: { headers: [{ name: "From", value: "Ana <ana@example.com>" }, { name: "Subject", value: "Lunch‮?\n" }, { name: "Date", value: "Mon" }] } }],
    [200, { payload: { headers: [] } }],
  ]);
  const out = await agent.run(ctx);
  assert.match(out, /^2 unread messages/);
  assert.match(out, /- Ana <ana@example.com>: Lunch \? \(Mon\)/);
  assert.match(out, /- \(unknown sender\): \(no subject\)/);
  assert.equal(calls.length, 3, "an id that is not a plain Gmail id is never requested");
  assert.ok(calls.every((c) => c.credential === "gmail-read" && c.method === "GET"));
  assert.match(calls[0].url, /^https:\/\/gmail\.googleapis\.com\/gmail\/v1\/users\/me\/messages\?q=is%3Aunread\+in%3Ainbox|is%3Aunread%20in%3Ainbox/);
  assert.match(calls[1].url, /\/messages\/m1\?format=metadata/);
});

test("gmail-triage says so when there is nothing, and reports a refusal instead of hiding it", async () => {
  const agent = await load("gmail-triage");
  assert.match(await agent.run(fakeCtx({}, [[200, {}]]).ctx), /No unread mail/);
  assert.match(await agent.run(fakeCtx({}, [[403, "quota"]]).ctx), /answer \(403\): quota/);
  assert.match(await agent.run(fakeCtx({}, [["throw", "connect your google account first"]]).ctx), /^connect your google account first/);
});

test("mail-compose saves a draft: nested raw, the draft credential, and a message that reads back as written", async () => {
  const agent = await load("mail-compose");
  const { ctx, calls, events } = fakeCtx(
    { message: "draft\nto: ana@example.com, ben@example.com\nsubject: Lunch?\n\nAre you free at noon?\nSecond line." },
    [[200, { id: "d1" }]],
  );
  const out = await agent.run(ctx);
  assert.equal(out, "Saved as a draft (draft d1). Nothing was sent.");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].method, "POST");
  assert.equal(calls[0].credential, "gmail-draft");
  assert.equal(calls[0].url, "https://gmail.googleapis.com/gmail/v1/users/me/drafts");
  const raw = decodeRaw(JSON.parse(calls[0].body).message.raw);
  assert.equal(
    raw,
    "To: ana@example.com, ben@example.com\r\nSubject: Lunch?\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nAre you free at noon?\r\nSecond line.",
  );
  assert.deepEqual(events, [["mail.compose.done", { action: "draft", recipients: 2 }]], "counts only, no addresses or text in events");
});

test("mail-compose sends with the send credential and a top-level raw, and reports a denial", async () => {
  const agent = await load("mail-compose");
  const sent = fakeCtx({ action: "send", to: "ana@example.com", subject: "Hi", body: "Hello" }, [[200, { id: "s1" }]]);
  assert.equal(await agent.run(sent.ctx), "Sent to 1 recipient (message s1).");
  assert.equal(sent.calls[0].credential, "gmail-send");
  assert.equal(sent.calls[0].url, "https://gmail.googleapis.com/gmail/v1/users/me/messages/send");
  assert.ok(typeof JSON.parse(sent.calls[0].body).raw === "string" && !("message" in JSON.parse(sent.calls[0].body)));
  const denied = fakeCtx({ action: "send", to: "ana@example.com", subject: "Hi", body: "Hello" }, [["throw", "send to gmail.googleapis.com was denied by an operator"]]);
  assert.match(await agent.run(denied.ctx), /^Not sent: send to gmail\.googleapis\.com was denied/);
  const drafted = fakeCtx({ action: "draft", to: "ana@example.com", subject: "Hi", body: "Hello" }, [["throw", "approval expired"]]);
  assert.equal(await agent.run(drafted.ctx), "Not saved as a draft: approval expired");
});

test("mail-compose defaults to a draft, never a send, and non-ASCII subjects are encoded", async () => {
  const agent = await load("mail-compose");
  const { ctx, calls } = fakeCtx({ to: "ana@example.com", subject: "Café ☕ und Übergabe", body: "x" }, [[200, {}]]);
  await agent.run(ctx);
  assert.equal(calls[0].credential, "gmail-draft");
  const raw = decodeRaw(JSON.parse(calls[0].body).message.raw);
  assert.match(raw, /Subject: =\?UTF-8\?B\?[A-Za-z0-9+/=]+\?=/);
  assert.ok(!raw.includes("Café"), "the subject is not sent as raw non-ASCII");
});

test("mail-compose refuses anything that could add headers or hide a recipient, before any request", async () => {
  const agent = await load("mail-compose");
  const cases = [
    { to: "ana@example.com\nBcc: spy@evil.test", subject: "s", body: "b" },
    { to: "ana@example.com", subject: "s\r\nBcc: spy@evil.test", body: "b" },
    { to: "Ana <ana@example.com>", subject: "s", body: "b" },
    { to: "a@b.co,c@d.co,e@f.co,g@h.co,i@j.co,k@l.co,m@n.co,o@p.co,q@r.co,s@t.co,u@v.co", subject: "s", body: "b" },
    { to: "", subject: "s", body: "b" },
    { to: "ana@example.com", subject: "s", body: "  " },
    { to: "ana@example.com", subject: "s", body: "x".repeat(20001) },
    { action: "forward", to: "ana@example.com", subject: "s", body: "b" },
  ];
  for (const input of cases) {
    const { ctx, calls } = fakeCtx(input);
    const out = await agent.run(ctx);
    assert.equal(calls.length, 0, JSON.stringify(input).slice(0, 80));
    assert.ok(out.length > 10);
  }
});

test("calendar-agent reads the agenda with the read credential", async () => {
  const agent = await load("calendar-agent");
  const { ctx, calls } = fakeCtx({ message: "agenda" }, [
    [200, { items: [{ start: { dateTime: "2026-10-01T19:00:00+02:00" }, summary: "Dinner", location: "Home" }, { start: { date: "2026-10-02" } }] }],
  ]);
  const out = await agent.run(ctx);
  assert.equal(out, "2 events:\n- 2026-10-01T19:00:00+02:00: Dinner @ Home\n- 2026-10-02: (no title)");
  assert.equal(calls[0].credential, "calendar-read");
  assert.match(calls[0].url, /^https:\/\/www\.googleapis\.com\/calendar\/v3\/calendars\/primary\/events\?timeMin=.*&singleEvents=true&orderBy=startTime/);
});

test("calendar-agent adds an event with the write credential, and emails guests only when asked", async () => {
  const agent = await load("calendar-agent");
  const msg = (extra = "") => `add\ntitle: Dinner\nstart: 2026-10-01T19:00:00+02:00\nend: 2026-10-01T21:00:00+02:00\nguests: ana@example.com\nwhere: Home\n${extra}`;
  const quiet = fakeCtx({ message: msg() }, [[200, {}]]);
  assert.equal(await agent.run(quiet.ctx), 'Added "Dinner". Your guests were not emailed.');
  assert.equal(quiet.calls[0].credential, "calendar-write");
  assert.equal(quiet.calls[0].method, "POST");
  assert.equal(quiet.calls[0].url, "https://www.googleapis.com/calendar/v3/calendars/primary/events");
  assert.deepEqual(JSON.parse(quiet.calls[0].body), {
    summary: "Dinner",
    start: { dateTime: "2026-10-01T19:00:00+02:00" },
    end: { dateTime: "2026-10-01T21:00:00+02:00" },
    attendees: [{ email: "ana@example.com" }],
    location: "Home",
  });
  const loud = fakeCtx({ message: msg("notify: yes") }, [[200, {}]]);
  assert.equal(await agent.run(loud.ctx), 'Added "Dinner". Your guests were emailed.');
  assert.equal(loud.calls[0].url, "https://www.googleapis.com/calendar/v3/calendars/primary/events?sendUpdates=all");
  const noGuests = fakeCtx({ message: "add\ntitle: Run\nstart: 2026-10-01\nend: 2026-10-02\nnotify: yes" }, [[200, {}]]);
  await agent.run(noGuests.ctx);
  assert.ok(!noGuests.calls[0].url.includes("sendUpdates"), "notify with nobody to notify sends nothing extra");
  assert.deepEqual(JSON.parse(noGuests.calls[0].body).start, { date: "2026-10-01" });
});

test("calendar-agent refuses a malformed event before any request", async () => {
  const agent = await load("calendar-agent");
  const ok = { action: "add", title: "T", start: "2026-10-01T19:00:00Z", end: "2026-10-01T20:00:00Z" };
  for (const bad of [
    { ...ok, title: "" },
    { ...ok, start: "tomorrow" },
    { ...ok, end: "2026-10-01" },
    { ...ok, end: "2026-10-01T18:00:00Z" },
    { ...ok, guests: "not an address" },
    { ...ok, guests: "a@b.co\nc@d.co" },
    { action: "delete" },
  ]) {
    const { ctx, calls } = fakeCtx(bad);
    const out = await agent.run(ctx);
    assert.equal(calls.length, 0, JSON.stringify(bad));
    assert.ok(out.length > 10);
  }
});
