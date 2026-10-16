// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

// The Outlook example agents against a fake `ctx.fetch`: what they ask for, which credential they name, what they refuse to build.

import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { buildBundle } from "../src/bundle.js";

const load = async (name) => {
  const { bundle } = await buildBundle(fileURLToPath(new URL(`../../../examples/merion-agents/${name}/agent.ts`, import.meta.url)));
  return (await import(`data:text/javascript;base64,${Buffer.from(bundle).toString("base64")}#${name}`)).default;
};

/** A ctx whose fetch answers from `replies` in order ([status, body]; status "throw" is the host refusing) and records every call. */
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
        calls.push({ url: String(url), method: init.method ?? "GET", credential: init.credential, body: init.body, headers: init.headers });
        const [status, body] = replies.shift() ?? [200, {}];
        if (status === "throw") throw new Error(body);
        const text = typeof body === "string" ? body : JSON.stringify(body);
        return { ok: status >= 200 && status < 300, status, text: async () => text };
      },
    },
  };
};

test("outlook-triage lists unread mail in one read-only request and prints other people's text as plain data", async () => {
  const agent = await load("outlook-triage");
  const { ctx, calls, events } = fakeCtx({ max: 5 }, [
    [200, { value: [
      { from: { emailAddress: { name: "Ana", address: "ana@example.com" } }, subject: "Lunch‮?\n", receivedDateTime: "2026-09-28T08:00:00Z" },
      { from: { emailAddress: { address: "no-name@example.com" } }, subject: "" },
      {},
    ] }],
  ]);
  const out = await agent.run(ctx);
  assert.match(out, /^3 unread messages/);
  assert.match(out, /- Ana <ana@example\.com>: Lunch \? \(2026-09-28T08:00:00Z\)/);
  assert.match(out, /- no-name@example\.com: \(no subject\)/);
  assert.match(out, /- \(unknown sender\): \(no subject\)/);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].credential, "outlook-read");
  assert.equal(calls[0].method, "GET");
  assert.match(calls[0].url, /^https:\/\/graph\.microsoft\.com\/v1\.0\/me\/mailFolders\/inbox\/messages\?/);
  assert.match(calls[0].url, /\$filter=isRead%20eq%20false&\$top=5&/, "spaces are %20, as OData documents them");
  assert.deepEqual(events, [["outlook.triage.done", { unread_listed: 3 }]]);
});

test("outlook-triage says so when there is nothing, and reports a refusal instead of hiding it", async () => {
  const agent = await load("outlook-triage");
  assert.match(await agent.run(fakeCtx({}, [[200, { value: [] }]]).ctx), /No unread mail/);
  assert.match(await agent.run(fakeCtx({}, [[403, "quota"]]).ctx), /answer \(403\): quota/);
  assert.match(await agent.run(fakeCtx({}, [["throw", "connect your microsoft account first"]]).ctx), /^connect your microsoft account first/);
});

test("outlook-compose saves a draft: the message itself, the draft credential, plain text", async () => {
  const agent = await load("outlook-compose");
  const { ctx, calls, events } = fakeCtx(
    { message: "draft\nto: ana@example.com, ben@example.com\nsubject: Lunch?\n\nAre you free at noon?\nSecond line." },
    [[201, { id: "AAMkAGI2THVSAAA=" }]],
  );
  const out = await agent.run(ctx);
  assert.equal(out, "Saved as a draft (AAMkAGI2THVS…). Nothing was sent.");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].method, "POST");
  assert.equal(calls[0].credential, "outlook-draft");
  assert.equal(calls[0].url, "https://graph.microsoft.com/v1.0/me/messages");
  assert.deepEqual(JSON.parse(calls[0].body), {
    subject: "Lunch?",
    body: { contentType: "Text", content: "Are you free at noon?\nSecond line." },
    toRecipients: [{ emailAddress: { address: "ana@example.com" } }, { emailAddress: { address: "ben@example.com" } }],
  });
  assert.deepEqual(events, [["outlook.compose.done", { action: "draft", recipients: 2 }]], "counts only, no addresses or text in events");
});

test("outlook-compose sends with the send credential and a wrapped message, and reports a denial", async () => {
  const agent = await load("outlook-compose");
  const sent = fakeCtx({ action: "send", to: "ana@example.com", subject: "Hi", body: "Hello" }, [[202, ""]]);
  assert.equal(await agent.run(sent.ctx), "Sent to 1 recipient.");
  assert.equal(sent.calls[0].credential, "outlook-send");
  assert.equal(sent.calls[0].url, "https://graph.microsoft.com/v1.0/me/sendMail");
  const body = JSON.parse(sent.calls[0].body);
  assert.equal(body.saveToSentItems, true);
  assert.equal(body.message.subject, "Hi");
  const denied = fakeCtx({ action: "send", to: "ana@example.com", subject: "Hi", body: "Hello" }, [["throw", "send to graph.microsoft.com was denied by an operator"]]);
  assert.match(await agent.run(denied.ctx), /^Not sent: send to graph\.microsoft\.com was denied/);
  const drafted = fakeCtx({ action: "draft", to: "ana@example.com", subject: "Hi", body: "Hello" }, [["throw", "approval expired"]]);
  assert.equal(await agent.run(drafted.ctx), "Not saved as a draft: approval expired");
});

test("outlook-compose defaults to a draft, and refuses anything that could hide a recipient, before any request", async () => {
  const agent = await load("outlook-compose");
  const { ctx, calls } = fakeCtx({ to: "ana@example.com", subject: "s", body: "x" }, [[201, {}]]);
  await agent.run(ctx);
  assert.equal(calls[0].credential, "outlook-draft");
  for (const input of [
    { to: "ana@example.com\nbcc@evil.test", subject: "s", body: "b" },
    { to: "ana@example.com", subject: "s\r\nBcc: x", body: "b" },
    { to: "Ana <ana@example.com>", subject: "s", body: "b" },
    { to: "", subject: "s", body: "b" },
    { to: "ana@example.com", subject: "s", body: "  " },
    { to: "ana@example.com", subject: "s", body: "x".repeat(20001) },
    { action: "forward", to: "ana@example.com", subject: "s", body: "b" },
  ]) {
    const f = fakeCtx(input);
    assert.ok((await agent.run(f.ctx)).length > 10);
    assert.equal(f.calls.length, 0, JSON.stringify(input).slice(0, 80));
  }
});

test("outlook-calendar reads the agenda with the read credential in UTC", async () => {
  const agent = await load("outlook-calendar");
  const { ctx, calls } = fakeCtx({ message: "agenda" }, [
    [200, { value: [{ subject: "Dinner", start: { dateTime: "2026-10-01T17:00:00.0000000" }, location: { displayName: "Home" } }, { start: { dateTime: "2026-10-02T00:00:00.0000000" } }] }],
  ]);
  const out = await agent.run(ctx);
  assert.equal(out, "2 events:\n- 2026-10-01T17:00:00.0000000 UTC: Dinner @ Home\n- 2026-10-02T00:00:00.0000000 UTC: (no title)");
  assert.equal(calls[0].credential, "outlook-calendar-read");
  assert.match(calls[0].url, /^https:\/\/graph\.microsoft\.com\/v1\.0\/me\/calendarView\?startDateTime=/);
  assert.match(calls[0].headers.prefer, /UTC/);
});

test("outlook-calendar adds an event in UTC, and only invites guests when told to, because Microsoft emails them", async () => {
  const agent = await load("outlook-calendar");
  const msg = (extra = "") => `add\ntitle: Dinner\nstart: 2026-10-01T19:00:00+02:00\nend: 2026-10-01T21:00:00+02:00\nwhere: Home\n${extra}`;
  const solo = fakeCtx({ message: msg() }, [[201, {}]]);
  assert.equal(await agent.run(solo.ctx), 'Added "Dinner".');
  assert.equal(solo.calls[0].credential, "outlook-calendar-write");
  assert.equal(solo.calls[0].url, "https://graph.microsoft.com/v1.0/me/events");
  assert.deepEqual(JSON.parse(solo.calls[0].body), {
    subject: "Dinner",
    start: { dateTime: "2026-10-01T17:00:00", timeZone: "UTC" },
    end: { dateTime: "2026-10-01T19:00:00", timeZone: "UTC" },
    location: { displayName: "Home" },
  });
  const unasked = fakeCtx({ message: msg("guests: ana@example.com") });
  assert.match(await agent.run(unasked.ctx), /Microsoft emails every guest/);
  assert.equal(unasked.calls.length, 0, "no request until the person said guests may be emailed");
  const invited = fakeCtx({ message: msg("guests: ana@example.com\nnotify: yes") }, [[201, {}]]);
  assert.equal(await agent.run(invited.ctx), 'Added "Dinner". Your guests were emailed.');
  assert.deepEqual(JSON.parse(invited.calls[0].body).attendees, [{ emailAddress: { address: "ana@example.com" }, type: "required" }]);
  const allDay = fakeCtx({ message: "add\ntitle: Run\nstart: 2026-10-01\nend: 2026-10-02" }, [[201, {}]]);
  await agent.run(allDay.ctx);
  const b = JSON.parse(allDay.calls[0].body);
  assert.equal(b.isAllDay, true);
  assert.deepEqual(b.start, { dateTime: "2026-10-01T00:00:00", timeZone: "UTC" });
});

test("outlook-calendar refuses a malformed event before any request and reports a denial", async () => {
  const agent = await load("outlook-calendar");
  const ok = { action: "add", title: "T", start: "2026-10-01T19:00:00Z", end: "2026-10-01T20:00:00Z" };
  for (const bad of [
    { ...ok, title: "" },
    { ...ok, start: "tomorrow" },
    { ...ok, end: "2026-10-01" },
    { ...ok, end: "2026-10-01T18:00:00Z" },
    { ...ok, guests: "not an address", notify: "yes" },
    { action: "delete" },
  ]) {
    const f = fakeCtx(bad);
    assert.ok((await agent.run(f.ctx)).length > 10);
    assert.equal(f.calls.length, 0, JSON.stringify(bad));
  }
  const denied = fakeCtx(ok, [["throw", "send to graph.microsoft.com was denied by an operator"]]);
  assert.match(await agent.run(denied.ctx), /^Event not added: send to graph\.microsoft\.com was denied/);
});
