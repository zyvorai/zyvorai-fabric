// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// Validates an AG-UI event stream (server-sent events on stdin) against the official @ag-ui/core schemas, and checks the ordering rules a client
// relies on: RUN_STARTED first, text messages opened, filled and closed in order, and exactly one terminal RUN_FINISHED or RUN_ERROR, last.
// usage: node agui-conformance.mjs <dir with node_modules/@ag-ui/core and zod> < stream
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
const require = createRequire(process.argv[2] + "/");
const { EventSchemas } = require("@ag-ui/core/schemas");

const events = [];
for (const line of readFileSync(0, "utf8").split("\n")) {
  if (!line.startsWith("data:")) continue;
  events.push(JSON.parse(line.slice(5).trim()));
}
const fail = (m) => { console.error("agui-conformance: FAIL: " + m); process.exit(1); };
if (events.length < 2) fail("expected at least RUN_STARTED and a terminal event, got " + events.length);
events.forEach((e, i) => {
  const r = EventSchemas.safeParse(e);
  if (!r.success) fail(`event ${i} (${e.type}) is not a valid AG-UI event: ${JSON.stringify(r.error.issues).slice(0, 300)}`);
});
if (events[0].type !== "RUN_STARTED") fail("the first event must be RUN_STARTED, got " + events[0].type);
const last = events[events.length - 1].type;
if (last !== "RUN_FINISHED" && last !== "RUN_ERROR") fail("the last event must be RUN_FINISHED or RUN_ERROR, got " + last);
const terminals = events.filter((e) => e.type === "RUN_FINISHED" || e.type === "RUN_ERROR").length;
if (terminals !== 1) fail("expected exactly one terminal event, got " + terminals);
let open = null;
for (const e of events) {
  if (e.type === "TEXT_MESSAGE_START") { if (open) fail("a text message started inside another"); open = e.messageId; }
  else if (e.type === "TEXT_MESSAGE_CONTENT") { if (open !== e.messageId) fail("content for a message that is not open"); }
  else if (e.type === "TEXT_MESSAGE_END") { if (open !== e.messageId) fail("end of a message that is not open"); open = null; }
  else if ((e.type === "RUN_FINISHED" || e.type === "RUN_ERROR") && open) fail("the run ended with a text message still open");
}
console.log(`agui-conformance: ${events.length} events valid (${events.map((e) => e.type).join(", ")})`);
