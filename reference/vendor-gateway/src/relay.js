// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// Push relay: the runtime POSTs a signed "approval.requested" message here; this turns it into a push.
// Keep embeds no vendor push SDK. Each `push.kind` is an adapter: implement `send(device, message)`.

import { createHmac, timingSafeEqual } from "node:crypto";
import { sendWebhook } from "./webhook.js";

export function verifyRelaySignature(secret, rawBody, header) {
  const want = createHmac("sha256", secret).update(rawBody).digest();
  const got = Buffer.from(String(header ?? "").replace(/^sha256=/, ""), "hex");
  return got.length === want.length && timingSafeEqual(got, want);
}

/** What the phone is shown. Never the request's contents: the runtime does not send them. */
export function notification(message) {
  const a = message.approval;
  return {
    title: `${a.kind} approval`,
    body: a.prompt,
    data: { approval_id: a.id, session_id: a.session_id, device_id: message.device.id, sign: message.sign },
  };
}

/** Adapters by `push.kind`. Webhook delivery requires an explicit host allowlist. */
export function defaultAdapters({ log = console.log, webhookAllowedHosts = [], dnsLookup, httpsRequest } = {}) {
  const needsVendor = (name) => async () => {
    throw new Error(`the ${name} adapter is not implemented: it needs the vendor's push credentials (see README)`);
  };
  return {
    // POST only to your approved push delivery service, never to an arbitrary enrolled URL.
    webhook: {
      async send(device, message) {
        await sendWebhook(device.push.token, notification(message), {
          allowedHosts: webhookAllowedHosts, dnsLookup, httpsRequest,
        });
      },
    },
    log: { async send(device, message) { log("push", device.id, JSON.stringify(notification(message))); } },
    fcm: { send: needsVendor("fcm") },
    mipush: { send: needsVendor("mipush") },
    hms: { send: needsVendor("hms") },
    oppo: { send: needsVendor("oppo") },
    vivo: { send: needsVendor("vivo") },
  };
}

/** Handle one relay request. Returns [status, body]. */
export async function handleRelay({ secret, adapters, rawBody, signature }) {
  if (!verifyRelaySignature(secret, rawBody, signature)) return [401, { error: "bad signature" }];
  let message;
  try { message = JSON.parse(rawBody); } catch { return [400, { error: "not JSON" }]; }
  const kind = message?.device?.push?.kind;
  const adapter = adapters[kind];
  if (!adapter) return [422, { error: `no adapter for push kind ${JSON.stringify(kind)}` }];
  try {
    await adapter.send(message.device, message);
    return [202, { sent: true, kind }];
  } catch (e) {
    // A 5xx makes the runtime retry (twice) and then journal the failure.
    return [502, { error: String(e.message ?? e) }];
  }
}
